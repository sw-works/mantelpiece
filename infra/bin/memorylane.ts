import { App } from "aws-cdk-lib";
import { MemoryLaneStack } from "../lib/memorylane-stack.js";

const app = new App();
const ctx = (key: string, fallback?: string): string => {
  const value = (app.node.tryGetContext(key) as string | undefined) ?? process.env[key.toUpperCase().replace(/-/g, "_")] ?? fallback;
  if (value === undefined) throw new Error(`missing context ${key} (pass -c ${key}=... or set ${key.toUpperCase()})`);
  return value;
};

new MemoryLaneStack(app, "MemoryLane", {
  env: { account: process.env.CDK_DEFAULT_ACCOUNT, region: process.env.CDK_DEFAULT_REGION ?? "us-east-1" },
  linkBase: ctx("link-base", "https://link.memorylane.test"),
  uploadBase: ctx("upload-base", "https://upload.memorylane.test"),
  amazonClientId: app.node.tryGetContext("amazon-client-id") as string | undefined,
  amazonClientSecret: app.node.tryGetContext("amazon-client-secret") as string | undefined,
  lambdaAssetDir: app.node.tryGetContext("lambda-asset-dir") as string | undefined,
  alexaSkillId: app.node.tryGetContext("alexa-skill-id") as string | undefined,
  alexaSkillCallbackUrls: (app.node.tryGetContext("alexa-skill-callback-urls") as string | undefined)?.split(","),
});
