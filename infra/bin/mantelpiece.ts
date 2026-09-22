import { App } from "aws-cdk-lib";
import { MantelpieceStack } from "../lib/mantelpiece-stack.js";

const app = new App();
new MantelpieceStack(app, "Mantelpiece", {
  env: { account: process.env.CDK_DEFAULT_ACCOUNT, region: process.env.CDK_DEFAULT_REGION ?? "us-east-1" },
  // Custom domains for the phone pages; without them the stack's CloudFront URLs are used.
  linkBase: app.node.tryGetContext("link-base") as string | undefined,
  uploadBase: app.node.tryGetContext("upload-base") as string | undefined,
  amazonClientId: app.node.tryGetContext("amazon-client-id") as string | undefined,
  amazonClientSecret: app.node.tryGetContext("amazon-client-secret") as string | undefined,
  lambdaAssetDir: app.node.tryGetContext("lambda-asset-dir") as string | undefined,
  alexaSkillId: app.node.tryGetContext("alexa-skill-id") as string | undefined,
  agentModelId: app.node.tryGetContext("agent-model-id") as string | undefined,
  retainData: app.node.tryGetContext("retain-data") !== "false",
  alexaSkillCallbackUrls: (app.node.tryGetContext("alexa-skill-callback-urls") as string | undefined)?.split(","),
});
