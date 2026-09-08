import { App } from "aws-cdk-lib";
import { Match, Template } from "aws-cdk-lib/assertions";
import { describe, expect, it } from "vitest";
import { MemoryLaneStack } from "../lib/memorylane-stack.js";

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

  it("runs two Rust Lambdas on arm64 with the placeholder asset", () => {
    const rust = template.findResources("AWS::Lambda::Function", {
      Properties: { Runtime: "provided.al2023" },
    });
    expect(Object.keys(rust)).toHaveLength(2);
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
    expect(organize).toContain("$.organize.keepers");
  });

  it("exposes the API with JWT-guarded approve and join routes plus a catch-all", () => {
    template.resourceCountIs("AWS::ApiGatewayV2::Api", 1);
    template.hasResourceProperties("AWS::ApiGatewayV2::Authorizer", { AuthorizerType: "JWT" });
    template.hasResourceProperties("AWS::ApiGatewayV2::Route", { RouteKey: "POST /device/approve", AuthorizationType: "JWT" });
    template.hasResourceProperties("AWS::ApiGatewayV2::Route", { RouteKey: "POST /join/amazon", AuthorizationType: "JWT" });
    template.hasResourceProperties("AWS::ApiGatewayV2::Route", { RouteKey: "ANY /{proxy+}", AuthorizationType: "NONE" });
  });

  it("API Lambda may start the organize machine and the ingest Lambda may call Rekognition", () => {
    template.hasResourceProperties("AWS::IAM::Policy", {
      PolicyDocument: { Statement: Match.arrayWith([Match.objectLike({ Action: "states:StartExecution" })]) },
    });
    template.hasResourceProperties("AWS::IAM::Policy", {
      PolicyDocument: {
        Statement: Match.arrayWith([Match.objectLike({ Action: ["rekognition:DetectLabels", "rekognition:DetectFaces"] })]),
      },
    });
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
