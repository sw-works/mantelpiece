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
      Environment: { Variables: Match.objectLike({ LINK_BASE: "https://link.example.com", POLL_HOLD_SECONDS: "25" }) },
    });
    template.hasResourceProperties("AWS::Lambda::Function", {
      MemorySize: 2048,
      Environment: { Variables: Match.objectLike({ EVENT_BUS: Match.anyValue() }) },
    });
  });

  it("gives videos a poster frame from an ffmpeg layer after signal", () => {
    template.resourceCountIs("AWS::Lambda::LayerVersion", 1);
    template.hasResourceProperties("AWS::Lambda::LayerVersion", { CompatibleArchitectures: ["arm64"] });
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
    const policies = template.findResources("AWS::BedrockAgentCore::Policy");
    expect(Object.keys(policies)).toHaveLength(1);
    const policy = Object.values(policies)[0];
    if (!policy) throw new Error("no policy");
    const statement = policy.Properties.Definition.Cedar.Statement as string;
    for (const tool of irreversibleTools) {
      expect(statement).toContain(`api___${tool.name}`);
    }
    expect(statement).toContain("context.input.confirmation");
    template.hasResourceProperties("AWS::BedrockAgentCore::PolicyEngine", { Name: "memorylane_tools" });
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
});
