import * as path from "node:path";
import { existsSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { CfnOutput, Duration, RemovalPolicy, Stack, type StackProps } from "aws-cdk-lib";
import * as apigw from "aws-cdk-lib/aws-apigatewayv2";
import { HttpJwtAuthorizer } from "aws-cdk-lib/aws-apigatewayv2-authorizers";
import { HttpLambdaIntegration } from "aws-cdk-lib/aws-apigatewayv2-integrations";
import * as cognito from "aws-cdk-lib/aws-cognito";
import * as dynamodb from "aws-cdk-lib/aws-dynamodb";
import * as events from "aws-cdk-lib/aws-events";
import * as targets from "aws-cdk-lib/aws-events-targets";
import * as iam from "aws-cdk-lib/aws-iam";
import * as lambda from "aws-cdk-lib/aws-lambda";
import * as s3 from "aws-cdk-lib/aws-s3";
import * as secretsmanager from "aws-cdk-lib/aws-secretsmanager";
import * as sfn from "aws-cdk-lib/aws-stepfunctions";
import * as sfnTasks from "aws-cdk-lib/aws-stepfunctions-tasks";
import type { Construct } from "constructs";

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

export class MemoryLaneStack extends Stack {
  readonly table: dynamodb.Table;
  readonly bucket: s3.Bucket;
  readonly bus: events.EventBus;
  readonly userPool: cognito.UserPool;
  readonly api: apigw.HttpApi;

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
    ingestFn.addToRolePolicy(
      new iam.PolicyStatement({
        actions: ["rekognition:DetectLabels", "rekognition:DetectFaces"],
        resources: ["*"],
      }),
    );

    // Signal: one execution per original object, started by the S3 event.
    const signalTask = new sfnTasks.LambdaInvoke(this, "Signal", {
      lambdaFunction: ingestFn,
      outputPath: "$.Payload",
    });
    const signalMachine = new sfn.StateMachine(this, "SignalMachine", {
      definitionBody: sfn.DefinitionBody.fromChainable(signalTask),
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
        resultSelector: { "keepers.$": "$.Payload.keepers", "trips.$": "$.Payload.trips" },
        resultPath,
      });
    const enrichMap = new sfn.Map(this, "EnrichKeepers", {
      itemsPath: "$.organize.keepers",
      maxConcurrency: 4,
      itemSelector: {
        task: "enrich",
        household: sfn.JsonPath.stringAt("$.household"),
        media_id: sfn.JsonPath.stringAt("$$.Map.Item.Value"),
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

    // HTTP API. Everything routes to the API Lambda; two routes require a Cognito JWT.
    this.api = new apigw.HttpApi(this, "Api", {
      corsPreflight: {
        allowOrigins: [props.uploadBase, props.linkBase],
        allowMethods: [apigw.CorsHttpMethod.GET, apigw.CorsHttpMethod.POST, apigw.CorsHttpMethod.DELETE],
        allowHeaders: ["authorization", "content-type"],
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

    new CfnOutput(this, "ApiUrl", { value: this.api.apiEndpoint });
    new CfnOutput(this, "UserPoolId", { value: this.userPool.userPoolId });
    new CfnOutput(this, "UserPoolClientId", { value: client.userPoolClientId });
    new CfnOutput(this, "MediaBucket", { value: this.bucket.bucketName });
    new CfnOutput(this, "TableName", { value: this.table.tableName });
    new CfnOutput(this, "EventBusName", { value: this.bus.eventBusName });
  }
}
