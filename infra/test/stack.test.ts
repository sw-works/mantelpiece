import { App } from "aws-cdk-lib";
import { Match, Template } from "aws-cdk-lib/assertions";
import { describe, expect, it } from "vitest";
import { MemoryLaneStack } from "../lib/memorylane-stack.js";
import { irreversibleTools, toolDefinitions } from "../lib/tools.js";

function synth(extra: Partial<ConstructorParameters<typeof MemoryLaneStack>[2]> = {}) {
  const app = new App();
  const stack = new MemoryLaneStack(app, "Test", {
    env: { account: "123456789012", region: "us-east-1" },
    linkBase: "https://link.example.com",
    uploadBase: "https://upload.example.com",
    ...extra,
  });
  return Template.fromStack(stack);
}

describe("MemoryLane stack", () => {
  const template = synth();

  it("hosts the phone pages on two CloudFront distributions and uses their URLs when no domain is given", () => {
    template.resourceCountIs("AWS::CloudFront::Distribution", 2);
    template.hasResourceProperties("AWS::CloudFront::Distribution", { DistributionConfig: Match.objectLike({ DefaultRootObject: "link.html" }) });
    template.hasResourceProperties("AWS::CloudFront::Distribution", { DistributionConfig: Match.objectLike({ DefaultRootObject: "index.html" }) });
    template.resourceCountIs("Custom::CDKBucketDeployment", 1);
    const app = new App();
    const stack = new MemoryLaneStack(app, "NoDomain", { env: { account: "123456789012", region: "us-east-1" } });
    const t = Template.fromStack(stack);
    const fns = Object.values(t.findResources("AWS::Lambda::Function", { Properties: { Handler: "bootstrap" } }));
    const api = fns.find((f) => f.Properties.Environment?.Variables?.LINK_BASE !== undefined);
    if (!api) throw new Error("no API function");
    const linkBase = api.Properties.Environment.Variables.LINK_BASE;
    expect(JSON.stringify(linkBase)).toContain("DomainName");
    t.hasOutput("LinkBase", {});
    t.hasOutput("UploadBase", {});
  });

  it("retains its data stores by default and destroys them only when asked", () => {
    template.hasResource("AWS::DynamoDB::Table", { DeletionPolicy: "Retain" });
    template.hasResource("AWS::Cognito::UserPool", { DeletionPolicy: "Retain" });
    const throwaway = synth({ retainData: false });
    throwaway.hasResource("AWS::DynamoDB::Table", { DeletionPolicy: "Delete" });
    throwaway.hasResource("AWS::Cognito::UserPool", { DeletionPolicy: "Delete" });
    const media = Object.values(throwaway.findResources("AWS::S3::Bucket")).filter((b) => b.Properties.CorsConfiguration);
    expect(media).toHaveLength(1);
    expect(media[0]!.DeletionPolicy).toBe("Delete");
  });

  it("has one household-partitioned table with the lookup index and TTL", () => {
    template.hasResourceProperties("AWS::DynamoDB::Table", {
      KeySchema: [
        { AttributeName: "pk", KeyType: "HASH" },
        { AttributeName: "sk", KeyType: "RANGE" },
      ],
      GlobalSecondaryIndexes: [Match.objectLike({ IndexName: "gsi1" })],
      TimeToLiveSpecification: { AttributeName: "ttl", Enabled: true },
      BillingMode: "PAY_PER_REQUEST",
    });
  });

  it("media bucket is private, sends events to EventBridge and allows browser uploads", () => {
    template.hasResourceProperties("AWS::S3::Bucket", {
      PublicAccessBlockConfiguration: Match.objectLike({ BlockPublicAcls: true }),
      CorsConfiguration: {
        CorsRules: [
          Match.objectLike({
            AllowedOrigins: ["https://upload.example.com", "https://link.example.com"],
            AllowedMethods: Match.arrayWith(["PUT"]),
            ExposedHeaders: ["ETag"],
          }),
        ],
      },
    });
    // EventBridge notifications are applied by CDK's bucket-notifications custom resource.
    template.hasResourceProperties("Custom::S3BucketNotifications", {
      NotificationConfiguration: { EventBridgeConfiguration: {} },
    });
  });

  it("runs three Rust Lambdas on arm64 with the placeholder asset", () => {
    const rust = template.findResources("AWS::Lambda::Function", {
      Properties: { Runtime: "provided.al2023" },
    });
    expect(Object.keys(rust)).toHaveLength(3);
    template.hasResourceProperties("AWS::Lambda::Function", {
      Runtime: "provided.al2023",
      Architectures: ["arm64"],
      Handler: "bootstrap",
      Environment: { Variables: Match.objectLike({ LINK_BASE: "https://link.example.com", POLL_HOLD_SECONDS: "25", EVENT_BUS: Match.anyValue() }) },
    });
    template.hasResourceProperties("AWS::Lambda::Function", {
      MemorySize: 2048,
      Environment: { Variables: Match.objectLike({ EVENT_BUS: Match.anyValue() }) },
    });
  });

  it("gives videos a poster frame from an ffmpeg layer after signal", () => {
    // One arm64 layer of ours; the web deployment brings its own AWS CLI layer.
    expect(Object.keys(template.findResources("AWS::Lambda::LayerVersion", { Properties: { CompatibleArchitectures: ["arm64"] } }))).toHaveLength(1);
    template.hasResourceProperties("AWS::Lambda::Function", {
      EphemeralStorage: { Size: 2048 },
      Environment: { Variables: Match.objectLike({ FFMPEG: "/opt/bin/ffmpeg" }) },
      Layers: Match.anyValue(),
    });
    const machines = template.findResources("AWS::StepFunctions::StateMachine");
    const definitions = Object.values(machines).map((m) => JSON.stringify(m.Properties.DefinitionString));
    const signal = definitions.find((d) => d.includes("NeedsPoster"));
    expect(signal).toBeDefined();
    expect(signal).toContain("$.poster");
    expect(signal).toContain("PhotoDone");
  });

  it("starts the signal machine from S3 object-created events under originals/", () => {
    template.resourceCountIs("AWS::StepFunctions::StateMachine", 2);
    template.hasResourceProperties("AWS::Events::Rule", {
      EventPattern: {
        source: ["aws.s3"],
        "detail-type": ["Object Created"],
        detail: Match.objectLike({ object: { key: [{ prefix: "originals/" }] } }),
      },
      Targets: [Match.objectLike({ InputTransformer: Match.objectLike({ InputTemplate: Match.stringLikeRegexp('"task":"signal"') }) })],
    });
  });

  it("organize machine waits, organises, enriches keepers, organises again", () => {
    const machines = template.findResources("AWS::StepFunctions::StateMachine");
    const definitions = Object.values(machines).map((m) => JSON.stringify(m.Properties.DefinitionString));
    const organize = definitions.find((d) => d.includes("SettleUploads"));
    expect(organize).toBeDefined();
    expect(organize).toContain("EnrichKeepers");
    expect(organize).toContain("OrganizeFinal");
    expect(organize).toContain("$.organize.keepers_manifest");
    // F-108: Distributed Map reading the manifest from S3, four keepers in flight.
    expect(organize).toContain("DISTRIBUTED");
    expect(organize).toContain("ItemReader");
    expect(organize).toContain("MaxConcurrency");
  });

  it("exposes the API with JWT-guarded approve and join routes plus a catch-all", () => {
    template.resourceCountIs("AWS::ApiGatewayV2::Api", 1);
    template.hasResourceProperties("AWS::ApiGatewayV2::Authorizer", { AuthorizerType: "JWT" });
    template.hasResourceProperties("AWS::ApiGatewayV2::Route", { RouteKey: "POST /device/approve", AuthorizationType: "JWT" });
    template.hasResourceProperties("AWS::ApiGatewayV2::Route", { RouteKey: "POST /join/amazon", AuthorizationType: "JWT" });
    template.hasResourceProperties("AWS::ApiGatewayV2::Route", { RouteKey: "POST /session/amazon", AuthorizationType: "JWT" });
    template.hasResourceProperties("AWS::ApiGatewayV2::Route", { RouteKey: "ANY /{proxy+}", AuthorizationType: "NONE" });
    // Guest sessions are bound to a browser install through this header (F-153).
    template.hasResourceProperties("AWS::ApiGatewayV2::Api", {
      CorsConfiguration: Match.objectLike({ AllowHeaders: Match.arrayWith(["x-device-id"]) }),
    });
  });

  it("API Lambda may start the organize machine and the ingest Lambda may call the model backends", () => {
    template.hasResourceProperties("AWS::IAM::Policy", {
      PolicyDocument: { Statement: Match.arrayWith([Match.objectLike({ Action: "states:StartExecution" })]) },
    });
    template.hasResourceProperties("AWS::IAM::Policy", {
      PolicyDocument: {
        Statement: Match.arrayWith([
          Match.objectLike({ Action: Match.arrayWith(["rekognition:DetectLabels", "rekognition:DetectText", "bedrock:InvokeModel", "geo-maps:GetStaticMap"]) }),
        ]),
      },
    });
    template.hasResourceProperties("AWS::IAM::Policy", {
      PolicyDocument: {
        Statement: Match.arrayWith([Match.objectLike({ Action: Match.arrayWith(["rekognition:CreateCollection", "polly:SynthesizeSpeech"]) })]),
      },
    });
  });

  it("exposes every contract tool through one MCP Gateway target on the API Lambda", () => {
    template.resourceCountIs("AWS::BedrockAgentCore::Gateway", 1);
    template.hasResourceProperties("AWS::BedrockAgentCore::Gateway", {
      ProtocolType: "MCP",
      AuthorizerType: "CUSTOM_JWT",
      PolicyEngineConfiguration: Match.objectLike({ Mode: "ENFORCE" }),
    });
    const targets = template.findResources("AWS::BedrockAgentCore::GatewayTarget");
    expect(Object.keys(targets)).toHaveLength(1);
    const target = Object.values(targets)[0];
    if (!target) throw new Error("no gateway target");
    const tools = target.Properties.TargetConfiguration.Mcp.Lambda.ToolSchema.InlinePayload as Array<{ Name: string }>;
    expect(tools.map((t) => t.Name)).toEqual(toolDefinitions.map((t) => t.name));
    expect(tools.map((t) => t.Name)).toEqual(expect.arrayContaining(["get_now_showing", "describe_photo", "apply_cleanup", "purge_expired"]));
  });

  it("loads the Cedar policy that gates every irreversible tool", () => {
    // One Cedar statement per Policy resource (AgentCore rejects several in one).
    const policies = Object.values(template.findResources("AWS::BedrockAgentCore::Policy"));
    expect(policies).toHaveLength(3);
    // Statements with the Gateway ARN token render as Fn::Join; flatten them to text.
    const statements = policies.map((p) => {
      const raw = p.Properties.Definition.Cedar.Statement as unknown;
      return typeof raw === "string" ? raw : JSON.stringify(raw);
    });
    for (const statement of statements) {
      expect(statement.match(/(permit|forbid)\s*\(/g) ?? []).toHaveLength(1);
    }
    const all = statements.join("\n");
    for (const tool of irreversibleTools) {
      expect(all).toContain(`api___${tool.name}`);
    }
    expect(all).toContain("context.input.confirmation");
    // Tool-specific rules name the Gateway ARN; nothing leaks the placeholder.
    expect(all).not.toContain("${GATEWAY_ARN}");
    expect(JSON.stringify(policies)).toContain("GatewayArn");
    // The permit lists every contract tool for OAuth callers, no wildcards.
    const permit = statements.find((s) => s.includes("permit("));
    if (!permit) throw new Error("no permit policy");
    expect(permit).toContain("principal is AgentCore::OAuthUser");
    for (const tool of toolDefinitions) expect(permit).toContain(`api___${tool.name}`);
    template.hasResourceProperties("AWS::BedrockAgentCore::PolicyEngine", { Name: "memorylane_tools" });
    // The Gateway's execution role may read the engine and ask it for decisions.
    template.hasResourceProperties("AWS::IAM::Policy", {
      PolicyDocument: {
        Statement: Match.arrayWith([Match.objectLike({ Action: Match.arrayWith(["bedrock-agentcore:GetPolicyEngine", "bedrock-agentcore:AuthorizeAction", "bedrock-agentcore:PartiallyAuthorizeActions"]) })]),
      },
    });
  });

  it("emits nightly.tick on the bus from a schedule, with no timer on any Lambda", () => {
    template.hasResourceProperties("AWS::Scheduler::Schedule", {
      ScheduleExpression: "cron(0 3 * * ? *)",
      Target: Match.objectLike({ EventBridgeParameters: { DetailType: "nightly.tick", Source: "memorylane.schedule" } }),
    });
    const rules = template.findResources("AWS::Events::Rule");
    for (const rule of Object.values(rules)) {
      expect(rule.Properties.ScheduleExpression).toBeUndefined();
    }
  });

  it("federates to Login with Amazon when credentials are given", () => {
    template.resourceCountIs("AWS::Cognito::UserPoolIdentityProvider", 0);
    const federated = synth({ amazonClientId: "amzn1.application-oa2-client.x", amazonClientSecret: "s" });
    federated.hasResourceProperties("AWS::Cognito::UserPoolIdentityProvider", { ProviderType: "LoginWithAmazon" });
    federated.hasResourceProperties("AWS::Cognito::UserPoolClient", {
      SupportedIdentityProviders: ["LoginWithAmazon"],
      CallbackURLs: ["https://link.example.com/callback", "https://upload.example.com/callback"],
    });
  });

  it("runs the Alexa skill as a Python Lambda that only Alexa may invoke", () => {
    template.hasResourceProperties("AWS::Lambda::Function", {
      Runtime: "python3.13",
      Handler: "memorylane_skill.handler.lambda_handler",
      Environment: { Variables: Match.objectLike({ API_BASE: Match.anyValue(), SKILL_ID: "" }) },
    });
    template.hasResourceProperties("AWS::Lambda::Permission", { Principal: "alexa-appkit.amazon.com", Action: "lambda:InvokeFunction" });
    const pinned = synth({ alexaSkillId: "amzn1.ask.skill.abc" });
    pinned.hasResourceProperties("AWS::Lambda::Permission", { Principal: "alexa-appkit.amazon.com", EventSourceToken: "amzn1.ask.skill.abc" });
    pinned.hasResourceProperties("AWS::Lambda::Function", { Environment: { Variables: Match.objectLike({ SKILL_ID: "amzn1.ask.skill.abc" }) } });
  });

  it("emails share links only from the verified sender, and not at all without one", () => {
    expect(template.findResources("AWS::SES::EmailIdentity")).toEqual({});
    template.hasResourceProperties("AWS::Lambda::Function", {
      Environment: { Variables: Match.objectLike({ SHARE_SENDER: "", LINK_BASE: "https://link.example.com" }) },
    });
    const withSender = synth({ shareSender: "hello@example.com" });
    withSender.hasResourceProperties("AWS::SES::EmailIdentity", { EmailIdentity: "hello@example.com" });
    withSender.hasResourceProperties("AWS::Lambda::Function", {
      Environment: { Variables: Match.objectLike({ SHARE_SENDER: "hello@example.com" }) },
    });
    const policies = JSON.stringify(withSender.findResources("AWS::IAM::Policy"));
    expect(policies).toContain("ses:SendEmail");
    expect(policies).toContain("ses:FromAddress");
  });

  it("gives Alexa+ a PKCE-only OAuth client with the memorylane/tools scope on the Gateway", () => {
    template.hasResourceProperties("AWS::Cognito::UserPoolResourceServer", {
      Identifier: "memorylane",
      Scopes: [Match.objectLike({ ScopeName: "tools" })],
    });
    const clients = Object.values(template.findResources("AWS::Cognito::UserPoolClient"));
    expect(clients).toHaveLength(3);
    const alexa = clients.find((c) => JSON.stringify(c.Properties.AllowedOAuthScopes).includes("tools"));
    if (!alexa) throw new Error("no Alexa+ client");
    expect(alexa.Properties.AllowedOAuthFlows).toEqual(["code"]);
    expect(alexa.Properties.GenerateSecret ?? false).toBe(false);
    expect(JSON.stringify(alexa.Properties.AllowedOAuthScopes)).toContain("openid");
    const gateway = Object.values(template.findResources("AWS::BedrockAgentCore::Gateway"))[0];
    if (!gateway) throw new Error("no gateway");
    expect(gateway.Properties.AuthorizerConfiguration.CustomJWTAuthorizer.AllowedClients).toHaveLength(2);
    // The skill's confidential client is a JWT audience for /session/amazon but not a Gateway caller.
    const skill = clients.find((c) => c.Properties.GenerateSecret === true);
    if (!skill) throw new Error("no Alexa skill client");
    expect(skill.Properties.AllowedOAuthFlows).toEqual(["code"]);
    const authorizer = Object.values(template.findResources("AWS::ApiGatewayV2::Authorizer"))[0];
    if (!authorizer) throw new Error("no JWT authorizer");
    expect(authorizer.Properties.JwtConfiguration.Audience).toHaveLength(2);
    template.hasResourceProperties("AWS::Lambda::Function", {
      Environment: { Variables: Match.objectLike({ OAUTH_ISSUER: Match.anyValue(), OAUTH_DOMAIN: Match.anyValue(), GATEWAY_URL: Match.anyValue() }) },
    });
  });
});
