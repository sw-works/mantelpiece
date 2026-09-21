import { mkdtempSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import * as path from "node:path";
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
    // Hermetic: never pick up a locally built agent package by accident.
    agentPackageDir: path.join(tmpdir(), "memorylane-no-agent-package"),
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

  it("runs four Rust Lambdas on arm64 with the placeholder asset", () => {
    const rust = template.findResources("AWS::Lambda::Function", {
      Properties: { Runtime: "provided.al2023" },
    });
    // api, ingest, poster, and the api binary again as the Gateway's interceptor.
    expect(Object.keys(rust)).toHaveLength(4);
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

  it("runs the Concierge on AgentCore Runtime once its package is built, fed by trip.detected", () => {
    template.resourceCountIs("AWS::BedrockAgentCore::Runtime", 0);
    const dir = mkdtempSync(path.join(tmpdir(), "agent-"));
    writeFileSync(path.join(dir, "main.py"), "print('agent')\n");
    const withAgent = synth({ agentPackageDir: dir, agentModelId: "test.model" });
    withAgent.hasResourceProperties("AWS::BedrockAgentCore::Runtime", {
      AgentRuntimeName: "memorylane_concierge",
      NetworkConfiguration: { NetworkMode: "PUBLIC" },
      AgentRuntimeArtifact: { CodeConfiguration: { EntryPoint: ["main.py"], Runtime: "PYTHON_3_13", Code: { S3: Match.objectLike({ Bucket: Match.anyValue() }) } } },
      EnvironmentVariables: Match.objectLike({ GATEWAY_URL: Match.anyValue(), GATEWAY_TOKEN_URL: Match.anyValue(), GATEWAY_CLIENT_ID: Match.anyValue(), MODEL_ID: "test.model" }),
    });
    withAgent.hasResourceProperties("AWS::Events::Rule", {
      EventPattern: { source: ["memorylane"], "detail-type": ["trip.detected"] },
    });
    withAgent.hasResourceProperties("AWS::IAM::Policy", {
      PolicyDocument: { Statement: Match.arrayWith([Match.objectLike({ Action: "bedrock-agentcore:InvokeAgentRuntime" })]) },
    });
  });

  it("makes the API Lambda the Gateway's request interceptor so tool calls carry a verified caller", () => {
    template.hasResourceProperties("AWS::BedrockAgentCore::Gateway", {
      InterceptorConfigurations: [
        { InterceptionPoints: ["REQUEST"], InputConfiguration: { PassRequestHeaders: true }, Interceptor: { Lambda: { Arn: Match.anyValue() } } },
      ],
    });
    template.hasResourceProperties("AWS::Lambda::Function", {
      Environment: { Variables: Match.objectLike({ AGENT_CLIENT_ID: Match.anyValue(), OAUTH_ISSUER: Match.anyValue() }) },
    });
    // People act for their own household, so no tool makes the caller name one.
    for (const tool of toolDefinitions) expect(tool.inputSchema.required ?? []).not.toContain("household");
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
    // Policies are validated against registered actions, so each waits for the api target.
    const targetId = Object.keys(template.findResources("AWS::BedrockAgentCore::GatewayTarget"))[0];
    for (const policy of policies) expect(policy.DependsOn).toContain(targetId);
    // Forbids name the permitted principal and wait for the permit (else "Overly Restrictive").
    const permitId = Object.keys(template.findResources("AWS::BedrockAgentCore::Policy")).find((k) => k.includes("permit"));
    for (const [id, policy] of Object.entries(template.findResources("AWS::BedrockAgentCore::Policy"))) {
      if (id === permitId) continue;
      expect(policy.DependsOn).toContain(permitId);
      expect(JSON.stringify(policy.Properties.Definition.Cedar.Statement)).toContain("principal is AgentCore::OAuthUser");
    }
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
    expect(clients).toHaveLength(4);
    const alexa = clients.find((c) => JSON.stringify(c.Properties.AllowedOAuthScopes).includes("tools") && c.Properties.GenerateSecret !== true);
    if (!alexa) throw new Error("no Alexa+ client");
    expect(alexa.Properties.AllowedOAuthFlows).toEqual(["code"]);
    expect(alexa.Properties.GenerateSecret ?? false).toBe(false);
    expect(JSON.stringify(alexa.Properties.AllowedOAuthScopes)).toContain("openid");
    const gateway = Object.values(template.findResources("AWS::BedrockAgentCore::Gateway"))[0];
    if (!gateway) throw new Error("no gateway");
    expect(gateway.Properties.AuthorizerConfiguration.CustomJWTAuthorizer.AllowedClients).toHaveLength(3);
    // The Concierge agent calls the Gateway as itself: client credentials, tools scope only.
    const agent = clients.find((c) => JSON.stringify(c.Properties.AllowedOAuthFlows) === JSON.stringify(["client_credentials"]));
    if (!agent) throw new Error("no Concierge client");
    expect(agent.Properties.GenerateSecret).toBe(true);
    expect(JSON.stringify(agent.Properties.AllowedOAuthScopes)).toContain("tools");
    // The skill's confidential client is a JWT audience for /session/amazon but not a Gateway caller.
    const skill = clients.find((c) => c.Properties.GenerateSecret === true && JSON.stringify(c.Properties.AllowedOAuthFlows) === JSON.stringify(["code"]));
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
