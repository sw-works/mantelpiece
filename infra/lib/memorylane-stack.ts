import * as path from "node:path";
import { existsSync, readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { CfnOutput, Duration, RemovalPolicy, Size, Stack, type StackProps } from "aws-cdk-lib";
import * as apigw from "aws-cdk-lib/aws-apigatewayv2";
import * as agentcore from "aws-cdk-lib/aws-bedrockagentcore";
import { HttpJwtAuthorizer } from "aws-cdk-lib/aws-apigatewayv2-authorizers";
import { HttpLambdaIntegration } from "aws-cdk-lib/aws-apigatewayv2-integrations";
import * as cognito from "aws-cdk-lib/aws-cognito";
import * as dynamodb from "aws-cdk-lib/aws-dynamodb";
import * as events from "aws-cdk-lib/aws-events";
import * as targets from "aws-cdk-lib/aws-events-targets";
import * as iam from "aws-cdk-lib/aws-iam";
import * as lambda from "aws-cdk-lib/aws-lambda";
import * as s3 from "aws-cdk-lib/aws-s3";
import * as scheduler from "aws-cdk-lib/aws-scheduler";
import * as schedulerTargets from "aws-cdk-lib/aws-scheduler-targets";
import * as secretsmanager from "aws-cdk-lib/aws-secretsmanager";
import * as sfn from "aws-cdk-lib/aws-stepfunctions";
import * as sfnTasks from "aws-cdk-lib/aws-stepfunctions-tasks";
import type { Construct } from "constructs";
import { toolDefinitions } from "./tools.js";

export interface MemoryLaneStackProps extends StackProps {
  /** Base URL of the sign-in approval page, e.g. https://link.example.com */
  linkBase: string;
  /** Base URL of the uploader page, e.g. https://upload.example.com */
  uploadBase: string;
  /** Login with Amazon credentials. Without them the user pool has no federated sign-in. */
  amazonClientId?: string;
  amazonClientSecret?: string;
  /**
   * Root of the cargo-lambda output (`services/target/lambda`). Each function is read
   * from `<root>/<name>/bootstrap`. Missing directories fall back to a placeholder so
   * `cdk synth` and the tests work without a Rust build.
   */
  lambdaAssetDir?: string;
}

const here = path.dirname(fileURLToPath(import.meta.url));
const placeholderDir = path.resolve(here, "..", "assets", "placeholder");
const cedarPolicyPath = path.resolve(here, "..", "policy", "tools.cedar");
const ffmpegLayerDir = path.resolve(here, "..", "assets", "ffmpeg-layer");

export class MemoryLaneStack extends Stack {
  readonly table: dynamodb.Table;
  readonly bucket: s3.Bucket;
  readonly bus: events.EventBus;
  readonly userPool: cognito.UserPool;
  readonly api: apigw.HttpApi;
  readonly gateway: agentcore.CfnGateway;

  constructor(scope: Construct, id: string, props: MemoryLaneStackProps) {
    super(scope, id, props);

    // Storage: one table, one bucket, one event bus. All partitioned by household.
    this.table = new dynamodb.Table(this, "Table", {
      partitionKey: { name: "pk", type: dynamodb.AttributeType.STRING },
      sortKey: { name: "sk", type: dynamodb.AttributeType.STRING },
      billingMode: dynamodb.BillingMode.PAY_PER_REQUEST,
      timeToLiveAttribute: "ttl",
      pointInTimeRecoverySpecification: { pointInTimeRecoveryEnabled: true },
      removalPolicy: RemovalPolicy.RETAIN,
    });
    this.table.addGlobalSecondaryIndex({
      indexName: "gsi1",
      partitionKey: { name: "gsi1pk", type: dynamodb.AttributeType.STRING },
      sortKey: { name: "gsi1sk", type: dynamodb.AttributeType.STRING },
    });

    this.bucket = new s3.Bucket(this, "Media", {
      blockPublicAccess: s3.BlockPublicAccess.BLOCK_ALL,
      encryption: s3.BucketEncryption.S3_MANAGED,
      enforceSSL: true,
      eventBridgeEnabled: true,
      removalPolicy: RemovalPolicy.RETAIN,
      cors: [
        {
          allowedOrigins: [props.uploadBase, props.linkBase],
          allowedMethods: [s3.HttpMethods.PUT, s3.HttpMethods.GET, s3.HttpMethods.HEAD],
          allowedHeaders: ["*"],
          exposedHeaders: ["ETag"],
          maxAge: 3600,
        },
      ],
      lifecycleRules: [{ abortIncompleteMultipartUploadAfter: Duration.days(7) }],
    });

    this.bus = new events.EventBus(this, "Bus");

    // Sign-in: Cognito user pool federated to Login with Amazon.
    this.userPool = new cognito.UserPool(this, "Users", {
      selfSignUpEnabled: false,
      signInAliases: { email: true },
      removalPolicy: RemovalPolicy.RETAIN,
    });
    const providers: cognito.UserPoolClientIdentityProvider[] = [];
    let amazonProvider: cognito.UserPoolIdentityProviderAmazon | undefined;
    if (props.amazonClientId && props.amazonClientSecret) {
      amazonProvider = new cognito.UserPoolIdentityProviderAmazon(this, "Amazon", {
        userPool: this.userPool,
        clientId: props.amazonClientId,
        clientSecret: props.amazonClientSecret,
        scopes: ["profile"],
        attributeMapping: {
          email: cognito.ProviderAttribute.AMAZON_EMAIL,
          fullname: cognito.ProviderAttribute.AMAZON_NAME,
        },
      });
      providers.push(cognito.UserPoolClientIdentityProvider.AMAZON);
    } else {
      providers.push(cognito.UserPoolClientIdentityProvider.COGNITO);
    }
    const client = this.userPool.addClient("Web", {
      supportedIdentityProviders: providers,
      oAuth: {
        flows: { authorizationCodeGrant: true },
        scopes: [cognito.OAuthScope.OPENID, cognito.OAuthScope.PROFILE, cognito.OAuthScope.EMAIL],
        callbackUrls: [`${props.linkBase}/callback`, `${props.uploadBase}/callback`],
        logoutUrls: [props.linkBase, props.uploadBase],
      },
    });
    if (amazonProvider) client.node.addDependency(amazonProvider);
    this.userPool.addDomain("Domain", {
      cognitoDomain: { domainPrefix: `memorylane-${this.account}`.toLowerCase() },
    });

    // Hackathon shortcut: the HMAC secret for TV and upload sessions rides in the Lambda
    // environment. Production would fetch it from Secrets Manager at cold start.
    const tokenSecret = new secretsmanager.Secret(this, "TokenSecret", {
      generateSecretString: { passwordLength: 48, excludePunctuation: true },
    });

    // Rust Lambdas.
    const assetRoot = props.lambdaAssetDir ?? path.resolve(here, "..", "..", "services", "target", "lambda");
    const rustFunction = (id: string, name: string, extra: Partial<lambda.FunctionProps>) => {
      const dir = path.join(assetRoot, name);
      const code = existsSync(path.join(dir, "bootstrap")) ? dir : placeholderDir;
      return new lambda.Function(this, id, {
        runtime: lambda.Runtime.PROVIDED_AL2023,
        architecture: lambda.Architecture.ARM_64,
        handler: "bootstrap",
        code: lambda.Code.fromAsset(code),
        memorySize: 1024,
        timeout: Duration.seconds(30),
        ...extra,
      });
    };

    const ingestFn = rustFunction("IngestFn", "ingest", {
      memorySize: 2048,
      timeout: Duration.minutes(5),
      environment: {
        TABLE_NAME: this.table.tableName,
        MEDIA_BUCKET: this.bucket.bucketName,
        EVENT_BUS: this.bus.eventBusName,
        RUST_LOG: "info",
      },
    });
    this.table.grantReadWriteData(ingestFn);
    this.bucket.grantReadWrite(ingestFn);
    this.bus.grantPutEventsTo(ingestFn);
    // Model and map backends behind the store traits (Vision, Faces, Maps). Embeddings
    // live in the table (DynamoVectors) until S3 Vectors is enabled per region (R7).
    ingestFn.addToRolePolicy(
      new iam.PolicyStatement({
        actions: [
          "rekognition:DetectLabels",
          "rekognition:DetectFaces",
          "rekognition:DetectText",
          "rekognition:IndexFaces",
          "rekognition:SearchFacesByImage",
          "bedrock:InvokeModel",
          "geo-maps:GetStaticMap",
        ],
        resources: ["*"],
      }),
    );

    // Poster frames for videos (research R5): a static ffmpeg from a layer built by
    // scripts/ffmpeg-layer.sh, well under the 250 MB unzipped limit. Without the binary
    // the layer is a placeholder and the poster step fails at runtime, never at synth.
    const ffmpegLayer = new lambda.LayerVersion(this, "Ffmpeg", {
      code: lambda.Code.fromAsset(existsSync(path.join(ffmpegLayerDir, "bin", "ffmpeg")) ? ffmpegLayerDir : placeholderDir),
      compatibleArchitectures: [lambda.Architecture.ARM_64],
      description: "Static ffmpeg under /opt/bin for poster frames",
    });
    const posterFn = rustFunction("PosterFn", "poster", {
      memorySize: 2048,
      timeout: Duration.minutes(2),
      ephemeralStorageSize: Size.gibibytes(2),
      layers: [ffmpegLayer],
      environment: {
        TABLE_NAME: this.table.tableName,
        MEDIA_BUCKET: this.bucket.bucketName,
        FFMPEG: "/opt/bin/ffmpeg",
        RUST_LOG: "info",
      },
    });
    this.table.grantReadWriteData(posterFn);
    this.bucket.grantReadWrite(posterFn);

    // Signal: one execution per original object, started by the S3 event. Videos go on
    // to the poster Lambda; photos are done.
    const signalTask = new sfnTasks.LambdaInvoke(this, "Signal", {
      lambdaFunction: ingestFn,
      outputPath: "$.Payload",
    });
    const posterTask = new sfnTasks.LambdaInvoke(this, "Poster", {
      lambdaFunction: posterFn,
      payload: sfn.TaskInput.fromObject({
        household: sfn.JsonPath.stringAt("$.household"),
        media_id: sfn.JsonPath.stringAt("$.media_id"),
      }),
      outputPath: "$.Payload",
    });
    const signalChain = signalTask.next(
      new sfn.Choice(this, "NeedsPoster")
        .when(sfn.Condition.booleanEquals("$.poster", true), posterTask)
        .otherwise(new sfn.Pass(this, "PhotoDone")),
    );
    const signalMachine = new sfn.StateMachine(this, "SignalMachine", {
      definitionBody: sfn.DefinitionBody.fromChainable(signalChain),
      timeout: Duration.minutes(10),
    });
    new events.Rule(this, "OriginalCreated", {
      eventPattern: {
        source: ["aws.s3"],
        detailType: ["Object Created"],
        detail: {
          bucket: { name: [this.bucket.bucketName] },
          object: { key: [{ prefix: "originals/" }] },
        },
      },
      targets: [
        new targets.SfnStateMachine(signalMachine, {
          input: events.RuleTargetInput.fromObject({
            task: "signal",
            key: events.EventField.fromPath("$.detail.object.key"),
          }),
        }),
      ],
    });

    // Organize: per household, started by the API when an upload batch completes.
    // Waits briefly so trailing signal executions land, organises, enriches keepers,
    // organises again so scores and highlights include the model results.
    const organize = (id: string, resultPath: string) =>
      new sfnTasks.LambdaInvoke(this, id, {
        lambdaFunction: ingestFn,
        payload: sfn.TaskInput.fromObject({
          task: "organize",
          household: sfn.JsonPath.stringAt("$.household"),
        }),
        resultSelector: {
          "keepers.$": "$.Payload.keepers",
          "keepers_manifest.$": "$.Payload.keepers_manifest",
          "trips.$": "$.Payload.trips",
        },
        resultPath,
      });
    // F-108: a Distributed Map over the keepers manifest organize wrote to S3, so a
    // 5,000-photo batch never hits the inline Map's payload limit. One execution per
    // household caps that household's model calls at four in flight.
    const enrichMap = new sfn.DistributedMap(this, "EnrichKeepers", {
      itemReader: new sfn.S3JsonItemReader({
        bucket: this.bucket,
        key: sfn.JsonPath.stringAt("$.organize.keepers_manifest"),
      }),
      maxConcurrency: 4,
      itemSelector: {
        task: "enrich",
        household: sfn.JsonPath.stringAt("$.household"),
        media_id: sfn.JsonPath.stringAt("$$.Map.Item.Value.media_id"),
      },
      resultPath: sfn.JsonPath.DISCARD,
    });
    enrichMap.itemProcessor(
      new sfnTasks.LambdaInvoke(this, "Enrich", { lambdaFunction: ingestFn, outputPath: "$.Payload" }),
    );
    const organizeChain = new sfn.Wait(this, "SettleUploads", {
      time: sfn.WaitTime.duration(Duration.seconds(30)),
    })
      .next(organize("OrganizeFirst", "$.organize"))
      .next(enrichMap)
      .next(organize("OrganizeFinal", "$.final"));
    const organizeMachine = new sfn.StateMachine(this, "OrganizeMachine", {
      definitionBody: sfn.DefinitionBody.fromChainable(organizeChain),
      timeout: Duration.hours(1),
    });
    this.bucket.grantRead(organizeMachine, "manifests/*");

    const apiFn = rustFunction("ApiFn", "api", {
      environment: {
        TABLE_NAME: this.table.tableName,
        MEDIA_BUCKET: this.bucket.bucketName,
        ORGANIZE_STATE_MACHINE_ARN: organizeMachine.stateMachineArn,
        TOKEN_SECRET: tokenSecret.secretValue.unsafeUnwrap(),
        LINK_BASE: props.linkBase,
        UPLOAD_BASE: props.uploadBase,
        POLL_HOLD_SECONDS: "25",
        RUST_LOG: "info",
      },
    });
    this.table.grantReadWriteData(apiFn);
    this.bucket.grantReadWrite(apiFn);
    organizeMachine.grantStartExecution(apiFn);
    // People opt-in (Faces), describe_photo fallback (Vision), narration (Speech).
    apiFn.addToRolePolicy(
      new iam.PolicyStatement({
        actions: [
          "rekognition:CreateCollection",
          "rekognition:DeleteCollection",
          "rekognition:IndexFaces",
          "rekognition:SearchFacesByImage",
          "bedrock:InvokeModel",
          "polly:SynthesizeSpeech",
          "geo-maps:GetStaticMap",
        ],
        resources: ["*"],
      }),
    );

    // HTTP API. Everything routes to the API Lambda; two routes require a Cognito JWT.
    this.api = new apigw.HttpApi(this, "Api", {
      corsPreflight: {
        allowOrigins: [props.uploadBase, props.linkBase],
        allowMethods: [apigw.CorsHttpMethod.GET, apigw.CorsHttpMethod.POST, apigw.CorsHttpMethod.DELETE],
        allowHeaders: ["authorization", "content-type", "x-device-id"],
        maxAge: Duration.hours(1),
      },
    });
    const integration = new HttpLambdaIntegration("ApiIntegration", apiFn);
    const jwt = new HttpJwtAuthorizer("CognitoJwt", this.userPool.userPoolProviderUrl, {
      jwtAudience: [client.userPoolClientId],
    });
    this.api.addRoutes({ path: "/device/approve", methods: [apigw.HttpMethod.POST], integration, authorizer: jwt });
    this.api.addRoutes({ path: "/join/amazon", methods: [apigw.HttpMethod.POST], integration, authorizer: jwt });
    this.api.addRoutes({ path: "/{proxy+}", methods: [apigw.HttpMethod.ANY], integration });

    // AgentCore Gateway: the one tool surface for the Concierge and Alexa+ (F-505, W2).
    // Cedar policy from policy/tools.cedar requires a confirmation token on every
    // irreversible tool (constitution III). Property values follow the CloudFormation
    // schema for AWS::BedrockAgentCore; not yet deployed.
    const policyEngine = new agentcore.CfnPolicyEngine(this, "ToolPolicyEngine", {
      name: "memorylane_tools",
      description: "Confirmation tokens for irreversible MemoryLane tools",
    });
    new agentcore.CfnPolicy(this, "ToolPolicy", {
      name: "irreversible_requires_confirmation",
      policyEngineId: policyEngine.attrPolicyEngineId,
      description: "Cedar rules from infra/policy/tools.cedar",
      definition: { cedar: { statement: readFileSync(cedarPolicyPath, "utf8") } },
    });
    const gatewayRole = new iam.Role(this, "GatewayRole", {
      assumedBy: new iam.ServicePrincipal("bedrock-agentcore.amazonaws.com"),
      description: "Lets the AgentCore Gateway invoke the API Lambda as its tool target",
    });
    apiFn.grantInvoke(gatewayRole);
    this.gateway = new agentcore.CfnGateway(this, "ToolGateway", {
      name: "memorylane_tools",
      description: "MemoryLane tools over MCP for the Concierge and Alexa+",
      roleArn: gatewayRole.roleArn,
      protocolType: "MCP",
      protocolConfiguration: { mcp: { supportedVersions: ["2025-11-25"], searchType: "SEMANTIC" } },
      authorizerType: "CUSTOM_JWT",
      authorizerConfiguration: {
        customJwtAuthorizer: {
          discoveryUrl: `${this.userPool.userPoolProviderUrl}/.well-known/openid-configuration`,
          allowedClients: [client.userPoolClientId],
        },
      },
      policyEngineConfiguration: { arn: policyEngine.attrPolicyEngineArn, mode: "ENFORCE" },
    });
    new agentcore.CfnGatewayTarget(this, "ApiTools", {
      gatewayIdentifier: this.gateway.attrGatewayIdentifier,
      name: "api",
      description: "Every tool in contracts/tools.md, served by the API Lambda",
      credentialProviderConfigurations: [{ credentialProviderType: "GATEWAY_IAM_ROLE" }],
      targetConfiguration: {
        mcp: {
          lambda: {
            lambdaArn: apiFn.functionArn,
            toolSchema: { inlinePayload: toolDefinitions },
          },
        },
      },
    });

    // Nightly work: one `nightly.tick` on the bus at 03:00 UTC. The Concierge is its only
    // consumer (W3 Household Patterns, memories_nightly, purge_expired) and fans out per
    // household in its own local time. No Lambda runs on a timer.
    new scheduler.Schedule(this, "NightlyTick", {
      description: "nightly.tick for the Concierge",
      schedule: scheduler.ScheduleExpression.cron({ minute: "0", hour: "3" }),
      target: new schedulerTargets.EventBridgePutEvents({
        eventBus: this.bus,
        source: "memorylane.schedule",
        detailType: "nightly.tick",
        detail: scheduler.ScheduleTargetInput.fromObject({ reason: "nightly" }),
      }),
    });

    new CfnOutput(this, "ApiUrl", { value: this.api.apiEndpoint });
    new CfnOutput(this, "GatewayUrl", { value: this.gateway.attrGatewayUrl });
    new CfnOutput(this, "PolicyEngineArn", { value: policyEngine.attrPolicyEngineArn });
    new CfnOutput(this, "UserPoolId", { value: this.userPool.userPoolId });
    new CfnOutput(this, "UserPoolClientId", { value: client.userPoolClientId });
    new CfnOutput(this, "MediaBucket", { value: this.bucket.bucketName });
    new CfnOutput(this, "TableName", { value: this.table.tableName });
    new CfnOutput(this, "EventBusName", { value: this.bus.eventBusName });
  }
}
