import * as path from "node:path";
import { existsSync, readdirSync, readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { CfnOutput, Duration, RemovalPolicy, Size, Stack, type StackProps } from "aws-cdk-lib";
import * as apigw from "aws-cdk-lib/aws-apigatewayv2";
import * as agentcore from "aws-cdk-lib/aws-bedrockagentcore";
import * as cloudfront from "aws-cdk-lib/aws-cloudfront";
import * as origins from "aws-cdk-lib/aws-cloudfront-origins";
import * as s3deploy from "aws-cdk-lib/aws-s3-deployment";
import { HttpJwtAuthorizer } from "aws-cdk-lib/aws-apigatewayv2-authorizers";
import { HttpLambdaIntegration } from "aws-cdk-lib/aws-apigatewayv2-integrations";
import * as cognito from "aws-cdk-lib/aws-cognito";
import * as dynamodb from "aws-cdk-lib/aws-dynamodb";
import * as events from "aws-cdk-lib/aws-events";
import * as targets from "aws-cdk-lib/aws-events-targets";
import * as iam from "aws-cdk-lib/aws-iam";
import * as ivs from "aws-cdk-lib/aws-ivs";
import * as lambda from "aws-cdk-lib/aws-lambda";
import * as s3 from "aws-cdk-lib/aws-s3";
import * as s3assets from "aws-cdk-lib/aws-s3-assets";
import * as scheduler from "aws-cdk-lib/aws-scheduler";
import * as ses from "aws-cdk-lib/aws-ses";
import * as schedulerTargets from "aws-cdk-lib/aws-scheduler-targets";
import * as secretsmanager from "aws-cdk-lib/aws-secretsmanager";
import * as sfn from "aws-cdk-lib/aws-stepfunctions";
import * as sfnTasks from "aws-cdk-lib/aws-stepfunctions-tasks";
import type { Construct } from "constructs";
import { toolDefinitions } from "./tools.js";

export interface MantelpieceStackProps extends StackProps {
  /**
   * Base URL of the sign-in approval page, e.g. https://link.example.com. Omit to serve the
   * page from the stack's own CloudFront distribution (no domain needed).
   */
  linkBase?: string;
  /** Base URL of the uploader page, e.g. https://upload.example.com; same default. */
  uploadBase?: string;
  /**
   * Built phone pages (`apps/web/dist`) to publish to the web bucket. Missing on a first
   * deploy, since the build needs the stack's outputs; a placeholder page goes up instead.
   */
  webDistDir?: string;
  /** Login with Amazon credentials. Without them the user pool has no federated sign-in. */
  amazonClientId?: string;
  amazonClientSecret?: string;
  /**
   * Root of the cargo-lambda output (`services/target/lambda`). Each function is read
   * from `<root>/<name>/bootstrap`. Missing directories fall back to a placeholder so
   * `cdk synth` and the tests work without a Rust build.
   */
  lambdaAssetDir?: string;
  /**
   * Verified sender for emailed share links (F-601), e.g. hello@example.com. SES sends a
   * verification mail on first deploy; without a sender the API returns links only.
   */
  shareSender?: string;
  /**
   * Redirect URIs Alexa+ uses after the user authorises the add-on (research R6). The
   * default is a placeholder until the preview registration says which; unverified.
   */
  alexaCallbackUrls?: string[];
  /** Directory built by `scripts/build-agent.sh`; the Runtime is left out when it is absent. */
  agentPackageDir?: string;
  /**
   * Bedrock model (or inference profile) id the Concierge writes with. Defaults to Amazon
   * Nova Pro, which needs no sign-up; an Anthropic model first needs the account's use-case
   * form (`-c agent-model-id=us.anthropic.claude-sonnet-4-20250514-v1:0`).
   */
  agentModelId?: string;
  /** Where route landmarks come from: "aws" (Amazon Location Places), "wikipedia", "none". */
  placesProvider?: "aws" | "wikipedia" | "none";
  /**
   * The custom Alexa skill (Echo Show slideshow, `agents/skill/`). The skill id locks the
   * Lambda's invoke permission to that skill; the callback URLs are the account-linking
   * redirects the developer console shows for the skill. Both are known only after the
   * skill is created in the console, so defaults are placeholders.
   */
  alexaSkillId?: string;
  alexaSkillCallbackUrls?: string[];
  /**
   * Keep the table, media bucket and user pool when the stack is destroyed. On by default
   * (they cost next to nothing idle); `false` for a throwaway run. A fresh deploy does not
   * reattach retained stores, they are a backup, not continuity.
   */
  retainData?: boolean;
}

const here = path.dirname(fileURLToPath(import.meta.url));
const placeholderDir = path.resolve(here, "..", "assets", "placeholder");
const cedarPolicyDir = path.resolve(here, "..", "policy");
const ffmpegLayerDir = path.resolve(here, "..", "assets", "ffmpeg-layer");
const skillAssetDir = path.resolve(here, "..", "..", "agents", "src");
/** `scripts/build-agent.sh` output: the Concierge with its arm64 dependencies. */
const agentPackageDefault = path.resolve(here, "..", "..", "agents", "build", "runtime");
const webDistDefault = path.resolve(here, "..", "..", "apps", "web", "dist");

export class MantelpieceStack extends Stack {
  readonly table: dynamodb.Table;
  readonly bucket: s3.Bucket;
  readonly bus: events.EventBus;
  readonly userPool: cognito.UserPool;
  readonly api: apigw.HttpApi;
  readonly gateway: agentcore.CfnGateway;

  constructor(scope: Construct, id: string, props: MantelpieceStackProps) {
    super(scope, id, props);
    const retain = props.retainData ?? true;
    const dataRemoval = retain ? RemovalPolicy.RETAIN : RemovalPolicy.DESTROY;

    // Phone pages: one private bucket behind two CloudFront distributions, so the link page
    // and the uploader keep separate origins (the owner session crosses them in a URL
    // fragment) without anyone owning a domain. A custom domain overrides either base.
    const webBucket = new s3.Bucket(this, "Web", {
      blockPublicAccess: s3.BlockPublicAccess.BLOCK_ALL,
      encryption: s3.BucketEncryption.S3_MANAGED,
      enforceSSL: true,
      removalPolicy: RemovalPolicy.DESTROY,
      autoDeleteObjects: true,
    });
    const webOrigin = origins.S3BucketOrigin.withOriginAccessControl(webBucket);
    const site = (id: string, root: string) =>
      new cloudfront.Distribution(this, id, {
        defaultBehavior: {
          origin: webOrigin,
          viewerProtocolPolicy: cloudfront.ViewerProtocolPolicy.REDIRECT_TO_HTTPS,
          allowedMethods: cloudfront.AllowedMethods.ALLOW_GET_HEAD_OPTIONS,
          cachePolicy: cloudfront.CachePolicy.CACHING_OPTIMIZED,
        },
        defaultRootObject: root,
        // `/callback` and other client routes are the page itself.
        errorResponses: [403, 404].map((httpStatus) => ({ httpStatus, responseHttpStatus: 200, responsePagePath: `/${root}`, ttl: Duration.minutes(1) })),
        httpVersion: cloudfront.HttpVersion.HTTP2_AND_3,
        priceClass: cloudfront.PriceClass.PRICE_CLASS_100,
        comment: `Mantelpiece ${root}`,
      });
    const linkSite = site("LinkSite", "link.html");
    const uploadSite = site("UploadSite", "index.html");
    const linkBase = props.linkBase ?? `https://${linkSite.distributionDomainName}`;
    const uploadBase = props.uploadBase ?? `https://${uploadSite.distributionDomainName}`;
    const webDist = props.webDistDir ?? webDistDefault;
    new s3deploy.BucketDeployment(this, "WebDeploy", {
      destinationBucket: webBucket,
      sources: [s3deploy.Source.asset(existsSync(path.join(webDist, "index.html")) ? webDist : placeholderDir)],
      distribution: uploadSite,
      distributionPaths: ["/*"],
      prune: true,
    });

    // Storage: one table, one bucket, one event bus. All partitioned by household.
    this.table = new dynamodb.Table(this, "Table", {
      partitionKey: { name: "pk", type: dynamodb.AttributeType.STRING },
      sortKey: { name: "sk", type: dynamodb.AttributeType.STRING },
      billingMode: dynamodb.BillingMode.PAY_PER_REQUEST,
      timeToLiveAttribute: "ttl",
      pointInTimeRecoverySpecification: { pointInTimeRecoveryEnabled: true },
      removalPolicy: dataRemoval,
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
      removalPolicy: dataRemoval,
      autoDeleteObjects: !retain,
      cors: [
        {
          allowedOrigins: [uploadBase, linkBase],
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
      removalPolicy: dataRemoval,
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
        callbackUrls: [`${linkBase}/callback`, `${uploadBase}/callback`],
        logoutUrls: [linkBase, uploadBase],
      },
    });
    if (amazonProvider) client.node.addDependency(amazonProvider);
    const domain = this.userPool.addDomain("Domain", {
      cognitoDomain: { domainPrefix: `mantelpiece-${this.account}` },
    });

    // Alexa+ MCP add-on (research R6): OAuth 2.1 with PKCE against the same user pool. The
    // resource server declares the one scope the Gateway checks; the public client has no
    // secret, so Cognito requires PKCE on the authorization-code grant.
    const toolsScope = new cognito.ResourceServerScope({ scopeName: "tools", scopeDescription: "Call Mantelpiece tools on the household's behalf" });
    const resourceServer = this.userPool.addResourceServer("Tools", { identifier: "mantelpiece", scopes: [toolsScope] });
    const alexaClient = this.userPool.addClient("AlexaPlus", {
      supportedIdentityProviders: providers,
      generateSecret: false,
      oAuth: {
        flows: { authorizationCodeGrant: true },
        scopes: [cognito.OAuthScope.OPENID, cognito.OAuthScope.resourceServer(resourceServer, toolsScope)],
        callbackUrls: props.alexaCallbackUrls ?? ["https://layla.amazon.com/api/skill/link/placeholder"],
      },
      accessTokenValidity: Duration.hours(1),
      refreshTokenValidity: Duration.days(30),
    });
    alexaClient.node.addDependency(resourceServer);
    if (amazonProvider) alexaClient.node.addDependency(amazonProvider);

    // The Concierge agent's own identity at the Gateway: client credentials, the same
    // `mantelpiece/tools` scope, no user. It works for whichever household an event names.
    const agentClient = this.userPool.addClient("Concierge", {
      generateSecret: true,
      oAuth: {
        flows: { clientCredentials: true },
        scopes: [cognito.OAuthScope.resourceServer(resourceServer, toolsScope)],
      },
      accessTokenValidity: Duration.hours(1),
    });
    agentClient.node.addDependency(resourceServer);

    // Custom Alexa skill account linking: a confidential client (Alexa keeps the secret),
    // authorization-code grant, the same profile scopes as the web client.
    const skillClient = this.userPool.addClient("AlexaSkill", {
      supportedIdentityProviders: providers,
      generateSecret: true,
      oAuth: {
        flows: { authorizationCodeGrant: true },
        scopes: [cognito.OAuthScope.OPENID, cognito.OAuthScope.PROFILE, cognito.OAuthScope.EMAIL],
        callbackUrls: props.alexaSkillCallbackUrls ?? ["https://layla.amazon.com/api/skill/link/placeholder", "https://pitangui.amazon.com/api/skill/link/placeholder"],
      },
      accessTokenValidity: Duration.hours(1),
      refreshTokenValidity: Duration.days(90),
    });
    if (amazonProvider) skillClient.node.addDependency(amazonProvider);

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
        // Named places along routes (F-206): Amazon Location Places, or "wikipedia" and
        // "none" for runs without the service.
        PLACES_PROVIDER: props.placesProvider ?? "aws",
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
          "rekognition:SearchFaces",
          "bedrock:InvokeModel",
          "geo-maps:GetStaticMap",
          // Place names for days and trips (PLACES_PROVIDER "aws"); without it every
          // lookup is denied and trips stay "Trip, March 24 to 25".
          "geo-places:SearchNearby",
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

    // Saved broadcasts: a phone that asks to keep its broadcast gets a channel with this
    // recording configuration (handlers/live.rs). IVS writes HLS into its own bucket through
    // its service-linked role; when `recording-ended.json` lands, the recording Lambda
    // remuxes the highest rendition to MP4 into `originals/`, and signal, poster and
    // organize take it from there like any phone video. The raw HLS is a working copy, so
    // it expires after a week.
    const recordingsBucket = new s3.Bucket(this, "Recordings", {
      blockPublicAccess: s3.BlockPublicAccess.BLOCK_ALL,
      encryption: s3.BucketEncryption.S3_MANAGED,
      enforceSSL: true,
      eventBridgeEnabled: true,
      removalPolicy: RemovalPolicy.DESTROY,
      autoDeleteObjects: true,
      lifecycleRules: [{ expiration: Duration.days(7) }],
    });
    const recordingConfig = new ivs.CfnRecordingConfiguration(this, "LiveRecording", {
      name: "mantelpiece-saved-broadcasts",
      destinationConfiguration: { s3: { bucketName: recordingsBucket.bucketName } },
      // The poster frame comes from the MP4; IVS thumbnails would only add objects.
      thumbnailConfiguration: { recordingMode: "DISABLED" },
    });
    const recordingFn = rustFunction("RecordingFn", "recording", {
      memorySize: 3008,
      timeout: Duration.minutes(15),
      ephemeralStorageSize: Size.gibibytes(4),
      layers: [ffmpegLayer],
      environment: {
        TABLE_NAME: this.table.tableName,
        MEDIA_BUCKET: this.bucket.bucketName,
        RECORDINGS_BUCKET: recordingsBucket.bucketName,
        ORGANIZE_STATE_MACHINE_ARN: organizeMachine.stateMachineArn,
        FFMPEG: "/opt/bin/ffmpeg",
        RUST_LOG: "info",
      },
    });
    this.table.grantReadWriteData(recordingFn);
    this.bucket.grantReadWrite(recordingFn);
    recordingsBucket.grantRead(recordingFn);
    organizeMachine.grantStartExecution(recordingFn);
    new events.Rule(this, "RecordingEnded", {
      eventPattern: {
        source: ["aws.s3"],
        detailType: ["Object Created"],
        detail: {
          bucket: { name: [recordingsBucket.bucketName] },
          object: { key: [{ suffix: "/events/recording-ended.json" }] },
        },
      },
      targets: [
        new targets.LambdaFunction(recordingFn, {
          event: events.RuleTargetInput.fromObject({ key: events.EventField.fromPath("$.detail.object.key") }),
          retryAttempts: 2,
        }),
      ],
    });

    const apiEnvironment = {
        TABLE_NAME: this.table.tableName,
        MEDIA_BUCKET: this.bucket.bucketName,
        SHARE_SENDER: props.shareSender ?? "",
        OAUTH_ISSUER: this.userPool.userPoolProviderUrl,
        OAUTH_DOMAIN: domain.baseUrl(),
        ORGANIZE_STATE_MACHINE_ARN: organizeMachine.stateMachineArn,
        EVENT_BUS: this.bus.eventBusName,
        TOKEN_SECRET: tokenSecret.secretValue.unsafeUnwrap(),
        LINK_BASE: linkBase,
        UPLOAD_BASE: uploadBase,
        POLL_HOLD_SECONDS: "25",
        RUST_LOG: "info",
        AGENT_CLIENT_ID: agentClient.userPoolClientId,
        IVS_RECORDING_ARN: recordingConfig.attrArn,
    };
    const apiFn = rustFunction("ApiFn", "api", { environment: apiEnvironment });
    // The same binary as the Gateway's REQUEST interceptor. A function of its own because
    // the API function needs the Gateway's URL and the Gateway needs its interceptor's
    // ARN. It holds the signing key and nothing else: no table, no bucket, no bus.
    const interceptFn = rustFunction("ToolInterceptFn", "api", { environment: apiEnvironment });
    this.table.grantReadWriteData(apiFn);
    this.bucket.grantReadWrite(apiFn);
    organizeMachine.grantStartExecution(apiFn);
    // Narrative regeneration re-triggers the Concierge through the bus (US4).
    this.bus.grantPutEventsTo(apiFn);
    // People opt-in (Faces), describe_photo fallback (Vision), narration (Speech).
    apiFn.addToRolePolicy(
      new iam.PolicyStatement({
        actions: [
          "rekognition:CreateCollection",
          "rekognition:DeleteCollection",
          "rekognition:IndexFaces",
          "rekognition:SearchFaces",
          "bedrock:InvokeModel",
          "polly:SynthesizeSpeech",
          "geo-maps:GetStaticMap",
        ],
        resources: ["*"],
      }),
    );

    // Emailed share links (F-601). One verified identity; the API may only send from it.
    if (props.shareSender) {
      const identity = new ses.EmailIdentity(this, "ShareSender", { identity: ses.Identity.email(props.shareSender) });
      apiFn.addToRolePolicy(
        new iam.PolicyStatement({
          actions: ["ses:SendEmail"],
          resources: [identity.emailIdentityArn],
          conditions: { StringEquals: { "ses:FromAddress": props.shareSender } },
        }),
      );
    }

    // Live from the event: one IVS channel per phone broadcast, made and deleted by the API
    // (handlers/live.rs). Channels are runtime resources outside this stack; `make down`
    // leaves none behind once every broadcast ended, and idle channels cost nothing. A
    // saved broadcast's channel names the recording configuration above.
    apiFn.addToRolePolicy(
      new iam.PolicyStatement({
        actions: ["ivs:CreateChannel", "ivs:DeleteChannel", "ivs:StopStream", "ivs:TagResource", "ivs:GetChannel"],
        resources: [
          `arn:aws:ivs:${this.region}:${this.account}:channel/*`,
          `arn:aws:ivs:${this.region}:${this.account}:stream-key/*`,
          recordingConfig.attrArn,
        ],
      }),
    );

    // HTTP API. Everything routes to the API Lambda; two routes require a Cognito JWT.
    this.api = new apigw.HttpApi(this, "Api", {
      corsPreflight: {
        allowOrigins: [uploadBase, linkBase],
        allowMethods: [apigw.CorsHttpMethod.GET, apigw.CorsHttpMethod.POST, apigw.CorsHttpMethod.PUT, apigw.CorsHttpMethod.PATCH, apigw.CorsHttpMethod.DELETE],
        allowHeaders: ["authorization", "content-type", "x-device-id"],
        maxAge: Duration.hours(1),
      },
    });
    const integration = new HttpLambdaIntegration("ApiIntegration", apiFn);
    const jwt = new HttpJwtAuthorizer("CognitoJwt", this.userPool.userPoolProviderUrl, {
      jwtAudience: [client.userPoolClientId, skillClient.userPoolClientId],
    });
    this.api.addRoutes({ path: "/device/approve", methods: [apigw.HttpMethod.POST], integration, authorizer: jwt });
    this.api.addRoutes({ path: "/join/amazon", methods: [apigw.HttpMethod.POST], integration, authorizer: jwt });
    this.api.addRoutes({ path: "/session/amazon", methods: [apigw.HttpMethod.POST], integration, authorizer: jwt });

    // The custom Alexa skill (Echo Show slideshow): stdlib-only Python, so the source tree
    // is the asset. Alexa invokes it directly; the skill id, once known, pins the caller.
    const skillFn = new lambda.Function(this, "SkillFn", {
      runtime: lambda.Runtime.PYTHON_3_13,
      architecture: lambda.Architecture.ARM_64,
      handler: "mantelpiece_skill.handler.lambda_handler",
      code: lambda.Code.fromAsset(skillAssetDir, { exclude: ["**/__pycache__", "**/*.pyc"] }),
      memorySize: 256,
      timeout: Duration.seconds(8),
      environment: { API_BASE: this.api.apiEndpoint, SKILL_ID: props.alexaSkillId ?? "" },
    });
    skillFn.addPermission("AlexaInvoke", {
      principal: new iam.ServicePrincipal("alexa-appkit.amazon.com"),
      action: "lambda:InvokeFunction",
      ...(props.alexaSkillId ? { eventSourceToken: props.alexaSkillId } : {}),
    });
    this.api.addRoutes({ path: "/{proxy+}", methods: [apigw.HttpMethod.ANY], integration });

    // AgentCore Gateway: the one tool surface for the Concierge and Alexa+ (F-505, W2).
    // Policy Engine first: the Gateway points at it. The rules themselves come after the
    // Gateway, since tool-specific Cedar must name the Gateway's ARN.
    const policyEngine = new agentcore.CfnPolicyEngine(this, "ToolPolicyEngine", {
      name: "mantelpiece_tools",
      description: "Confirmation tokens for irreversible Mantelpiece tools",
    });
    const gatewayRole = new iam.Role(this, "GatewayRole", {
      assumedBy: new iam.ServicePrincipal("bedrock-agentcore.amazonaws.com"),
      description: "Lets the AgentCore Gateway invoke the API Lambda as its tool target",
    });
    apiFn.grantInvoke(gatewayRole);
    interceptFn.grantInvoke(gatewayRole);
    // Policy evaluation at request time (policy-permissions guide): the three actions the
    // Gateway calls on its Policy Engine, on both engine and gateway ARNs. Wildcards because
    // the Gateway's own ARN is not known until it exists. A separate policy resource so the
    // Gateway can depend on it without a cycle through the API Lambda's GATEWAY_URL.
    const engineAccess = new iam.Policy(this, "GatewayPolicyEngineAccess", {
      statements: [
        new iam.PolicyStatement({
          actions: ["bedrock-agentcore:GetPolicyEngine", "bedrock-agentcore:AuthorizeAction", "bedrock-agentcore:PartiallyAuthorizeActions"],
          resources: [
            `arn:aws:bedrock-agentcore:${this.region}:${this.account}:policy-engine/*`,
            `arn:aws:bedrock-agentcore:${this.region}:${this.account}:gateway/*`,
          ],
        }),
      ],
    });
    engineAccess.attachToRole(gatewayRole);
    // CreateGateway checks GetPolicyEngine with the role at once, so the role's policy
    // must exist first; without these the Gateway raced its own permissions.
    this.gateway = new agentcore.CfnGateway(this, "ToolGateway", {
      name: "mantelpiece-tools",
      description: "Mantelpiece tools over MCP for the Concierge and Alexa+",
      roleArn: gatewayRole.roleArn,
      protocolType: "MCP",
      protocolConfiguration: { mcp: { supportedVersions: ["2025-11-25"], searchType: "SEMANTIC" } },
      authorizerType: "CUSTOM_JWT",
      authorizerConfiguration: {
        customJwtAuthorizer: {
          discoveryUrl: `${this.userPool.userPoolProviderUrl}/.well-known/openid-configuration`,
          allowedClients: [client.userPoolClientId, alexaClient.userPoolClientId, agentClient.userPoolClientId],
        },
      },
      policyEngineConfiguration: { arn: policyEngine.attrPolicyEngineArn, mode: "ENFORCE" },
      // A Lambda target never sees who called. The API binary therefore also runs as the
      // REQUEST interceptor: it reads the bearer token the authorizer above accepted and
      // seals the caller into the tool arguments; as the target it refuses unsealed calls.
      interceptorConfigurations: [
        {
          interceptionPoints: ["REQUEST"],
          interceptor: { lambda: { arn: interceptFn.functionArn } },
          inputConfiguration: { passRequestHeaders: true },
        },
      ],
    });
    this.gateway.node.addDependency(engineAccess);
    // The API serves the OAuth metadata documents that point Alexa+ at this endpoint.
    apiFn.addEnvironment("GATEWAY_URL", this.gateway.attrGatewayUrl);
    const apiTarget = new agentcore.CfnGatewayTarget(this, "ApiTools", {
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

    // Cedar policies (constitution III). AgentCore takes one statement per Policy and
    // refuses wildcards, so: one file per forbid rule in infra/policy (tool-specific rules
    // name this Gateway through the `${GATEWAY_ARN}` placeholder), plus a generated permit
    // that lists every contract tool for OAuth callers. Forbid wins over permit. Each
    // policy is validated against the Gateway's registered actions, so it must wait for
    // the target that registers `api___<tool>`.
    const policyFor = (id: string, file: string, statement: string) => {
      const policy = new agentcore.CfnPolicy(this, `ToolPolicy_${id}`, {
        name: id,
        policyEngineId: policyEngine.attrPolicyEngineId,
        description: file,
        definition: { cedar: { statement } },
      });
      policy.node.addDependency(apiTarget);
      return policy;
    };
    // The permit goes first: a forbid is rejected as "Overly Restrictive" when nothing
    // permits its principal, so each forbid names OAuthUser and waits for the permit.
    const permitted = toolDefinitions.map((t) => `    AgentCore::Action::"api___${t.name}"`).join(",\n");
    const permit = policyFor(
      "permit_contract_tools",
      "Generated from infra/lib/tools.ts: every contract tool for authenticated callers",
      `// Every contract tool is open to an authenticated caller; the forbid rules gate the\n// irreversible ones, and the API scopes each call to the caller's household.\npermit(\n  principal is AgentCore::OAuthUser,\n  action in [\n${permitted}\n  ],\n  resource == AgentCore::Gateway::"${this.gateway.attrGatewayArn}"\n);\n`,
    );
    for (const file of readdirSync(cedarPolicyDir).filter((f) => f.endsWith(".cedar")).sort()) {
      const name = file.replace(/^\d+-/, "").replace(/\.cedar$/, "").replace(/-/g, "_");
      policyFor(name, `Cedar rule from infra/policy/${file}`, readFileSync(path.join(cedarPolicyDir, file), "utf8").split("${GATEWAY_ARN}").join(this.gateway.attrGatewayArn)).node.addDependency(permit);
    }

    // The Concierge on AgentCore Runtime (F-503): the Strands agent in agents/, deployed as
    // code (no container), calling its tools through the Gateway above as its own Cognito
    // client. Skipped until `scripts/build-agent.sh` has produced the package, so tests and
    // a first `cdk synth` need no Python toolchain.
    const agentPackage = props.agentPackageDir ?? agentPackageDefault;
    if (existsSync(path.join(agentPackage, "main.py"))) {
      const agentCode = new s3assets.Asset(this, "ConciergeCode", { path: agentPackage });
      const runtimeRole = new iam.Role(this, "ConciergeRole", {
        assumedBy: new iam.ServicePrincipal("bedrock-agentcore.amazonaws.com"),
        description: "The Concierge agent: Bedrock models, its own logs and traces",
      });
      agentCode.grantRead(runtimeRole);
      runtimeRole.addToPolicy(
        new iam.PolicyStatement({
          actions: ["bedrock:InvokeModel", "bedrock:InvokeModelWithResponseStream"],
          resources: ["*"],
        }),
      );
      runtimeRole.addToPolicy(
        new iam.PolicyStatement({
          actions: ["logs:CreateLogGroup", "logs:CreateLogStream", "logs:PutLogEvents", "logs:DescribeLogGroups", "logs:DescribeLogStreams", "xray:PutTraceSegments", "xray:PutTelemetryRecords", "cloudwatch:PutMetricData"],
          resources: ["*"],
        }),
      );
      const runtime = new agentcore.CfnRuntime(this, "Concierge", {
        agentRuntimeName: "mantelpiece_concierge",
        description: "Trip Curator and the other Mantelpiece workflows",
        roleArn: runtimeRole.roleArn,
        networkConfiguration: { networkMode: "PUBLIC" },
        agentRuntimeArtifact: {
          codeConfiguration: {
            code: { s3: { bucket: agentCode.s3BucketName, prefix: agentCode.s3ObjectKey } },
            entryPoint: ["main.py"],
            runtime: "PYTHON_3_13",
          },
        },
        environmentVariables: {
          GATEWAY_URL: this.gateway.attrGatewayUrl,
          GATEWAY_TOKEN_URL: `${domain.baseUrl()}/oauth2/token`,
          GATEWAY_CLIENT_ID: agentClient.userPoolClientId,
          // Hackathon shortcut, like TOKEN_SECRET: production would read Secrets Manager.
          GATEWAY_CLIENT_SECRET: agentClient.userPoolClientSecret.unsafeUnwrap(),
          MODEL_ID: props.agentModelId ?? "us.amazon.nova-pro-v1:0",
        },
      });
      runtime.node.addDependency(runtimeRole);

      // EventBridge has no AgentCore Runtime target, so one small function hands each
      // event over. A run is synchronous; the story takes well under its timeout.
      const forwardFn = new lambda.Function(this, "ConciergeForwardFn", {
        runtime: lambda.Runtime.PYTHON_3_13,
        architecture: lambda.Architecture.ARM_64,
        handler: "index.handler",
        timeout: Duration.minutes(5),
        memorySize: 256,
        environment: { AGENT_RUNTIME_ARN: runtime.attrAgentRuntimeArn },
        code: lambda.Code.fromInline(
          [
            "import json, os, uuid, boto3",
            "from botocore.config import Config",
            "client = boto3.client('bedrock-agentcore', config=Config(read_timeout=280, retries={'max_attempts': 1}))",
            "def handler(event, _context):",
            "    payload = {'detail-type': event['detail-type'], 'detail': event['detail']}",
            "    out = client.invoke_agent_runtime(agentRuntimeArn=os.environ['AGENT_RUNTIME_ARN'], runtimeSessionId=str(uuid.uuid4()) + str(uuid.uuid4()), payload=json.dumps(payload).encode(), contentType='application/json', accept='application/json')",
            "    body = out['response'].read().decode()",
            "    print(json.dumps({'event': event['detail-type'], 'household': event['detail'].get('household'), 'result': body[:2000]}))",
            "    return {'ok': True}",
          ].join("\n"),
        ),
      });
      forwardFn.addToRolePolicy(
        new iam.PolicyStatement({
          actions: ["bedrock-agentcore:InvokeAgentRuntime"],
          resources: [runtime.attrAgentRuntimeArn, `${runtime.attrAgentRuntimeArn}/*`],
        }),
      );
      // The events the Concierge can act on today: a new or grown trip gets its story.
      new events.Rule(this, "ConciergeEvents", {
        eventBus: this.bus,
        eventPattern: { source: ["mantelpiece"], detailType: ["trip.detected"] },
        targets: [new targets.LambdaFunction(forwardFn, { retryAttempts: 1 })],
      });
      new CfnOutput(this, "ConciergeRuntimeArn", { value: runtime.attrAgentRuntimeArn });
    }

    // Nightly work: one `nightly.tick` on the bus at 03:00 UTC. The Concierge is its only
    // consumer (W3 Household Patterns, memories_nightly, purge_expired) and fans out per
    // household in its own local time. No Lambda runs on a timer.
    new scheduler.Schedule(this, "NightlyTick", {
      description: "nightly.tick for the Concierge",
      schedule: scheduler.ScheduleExpression.cron({ minute: "0", hour: "3" }),
      target: new schedulerTargets.EventBridgePutEvents({
        eventBus: this.bus,
        source: "mantelpiece.schedule",
        detailType: "nightly.tick",
        detail: scheduler.ScheduleTargetInput.fromObject({ reason: "nightly" }),
      }),
    });

    new CfnOutput(this, "ApiUrl", { value: this.api.apiEndpoint });
    new CfnOutput(this, "LinkBase", { value: linkBase });
    // BucketDeployment invalidates one distribution (the upload site); deploy-web.sh
    // invalidates this one, or phones keep getting yesterday's pairing page.
    new CfnOutput(this, "LinkDistributionId", { value: linkSite.distributionId });
    new CfnOutput(this, "UploadBase", { value: uploadBase });
    new CfnOutput(this, "CognitoDomain", { value: domain.baseUrl() });
    new CfnOutput(this, "WebBucket", { value: webBucket.bucketName });
    new CfnOutput(this, "GatewayUrl", { value: this.gateway.attrGatewayUrl });
    new CfnOutput(this, "PolicyEngineArn", { value: policyEngine.attrPolicyEngineArn });
    new CfnOutput(this, "UserPoolId", { value: this.userPool.userPoolId });
    new CfnOutput(this, "UserPoolClientId", { value: client.userPoolClientId });
    new CfnOutput(this, "ConciergeClientId", { value: agentClient.userPoolClientId });
    new CfnOutput(this, "AlexaSkillClientId", { value: skillClient.userPoolClientId });
    new CfnOutput(this, "AlexaSkillFunctionArn", { value: skillFn.functionArn });
    new CfnOutput(this, "MediaBucket", { value: this.bucket.bucketName });
    new CfnOutput(this, "TableName", { value: this.table.tableName });
    new CfnOutput(this, "EventBusName", { value: this.bus.eventBusName });
  }
}
