import { diagram, node } from "./svg.mjs";
// Topology as built — services/lambdas/api, services/lambdas/ingest, infra/lib/mantelpiece-stack.ts,
// agents/src/mantelpiece_agents. Drawn 2026-09-08.
const W = 200, GAP = 36, H = 64;
const X = (i) => 40 + i * (W + GAP);
const R0 = 96, R1 = 250, R2 = 404, R3 = 610;
const mid = (i) => X(i) + W / 2;

export default () => diagram({
  width: 1460, height: 900,
  title: "Mantelpiece · Topology — one QR, one hosted backend, one governed tool surface",
  subtitle: "Phones upload straight to S3; ingest is deterministic Rust until keepers reach a model; agents reach data only through the Gateway, and irreversible tools only with a token the remote minted.",
  groups: [
    { x: X(2) - 18, y: R2 - 44, w: 3 * W + 2 * GAP + 36, h: H + 66, label: "Step Functions · Ingest" },
    { x: X(3) - 18, y: R3 - 44, w: 2 * W + GAP + 36, h: H + 66, label: "AgentCore" },
  ],
  nodes: [
    node("tv",       X(0), R0, W, H, "Fire TV app",        { sub: "RN for Vega · fetch only\nlong-poll /events" }),
    node("phone",    X(1), R0, W, H, "Phone uploader",     { sub: "PWA · joins by trip code\nresumable multipart" }),
    node("s3",       X(2), R0, W, H, "S3 originals",       { kind: "store", sub: "originals/{household}/{contributor}/…\nrenditions/…" }),
    node("api",      X(0), R1, 2 * W + GAP, H, "Backend API", { sub: "API Gateway + Lambda (Rust)\ndevice code · trip codes · uploads · events · confirmations" }),
    node("ddb",      X(0), R2, W, H, "DynamoDB",           { kind: "store", sub: "single table, pk = household\nmedia · trips · codes · feed" }),
    node("signal",   X(2), R2, W, H, "signal",             { sub: "sha256 · EXIF · pHash\nsharpness · renditions" }),
    node("organize", X(3), R2, W, H, "organize",           { sub: "bursts · keepers · clocks\ntrips · routes · highlights" }),
    node("enrich",   X(4), R2, W, H, "enrich",             { sub: "keepers only\nrest of burst inherits" }),
    node("rek",      X(5), R2, W, H, "Rekognition",        { kind: "llm", sub: "labels · faces · smiles" }),
    node("review",   X(0), R3, W, H, "Review Changes",     { kind: "human", sub: "TV modal, confirm on the remote\nmints a single-use token" }),
    node("bus",      X(1), R3, W, H, "EventBridge bus",    { kind: "store", sub: "trip.detected\ncontribution.received" }),
    node("gateway",  X(3), R3, W, H, "Gateway + Policy",   { kind: "security", sub: "Identity (Cognito) · Cedar\nLambda targets = the API" }),
    node("agent",    X(4), R3, W, H, "Concierge",          { kind: "llm", sub: "Runtime · Strands · Memory\nClaude on Bedrock · Workflows #1–#6" }),
    node("alexa",    X(5), R3, W, H, "Alexa+",             { kind: "ext", sub: "MCP add-on\nsame Gateway endpoint" }),
  ],
  edges: [
    { from: "tv", to: "api", label: "Ask, Share → Workflows #2, #5", at: [mid(0) + 40, R0 + H + 16] },
    { from: "phone", to: "api" },
    { from: "phone", to: "s3", label: "PUT parts", at: [X(1) + W + GAP / 2, R0 + H + 14] },
    { from: "api", to: "ddb" },
    { from: "s3", to: "signal", label: "Object Created → Signal machine", at: [mid(2) + 110, 200],
      points: [[mid(2), R0 + H], [mid(2), R2]] },
    { from: "api", to: "organize", muted: true, label: "batch complete → StartExecution", at: [mid(2) + 130, R1 + H - 30],
      points: [[X(1) + W, R1 + H - 16], [X(3) + 60, R1 + H - 16], [X(3) + 60, R2]] },
    { from: "signal", to: "organize", muted: true, label: "signals", at: [X(2) + W + GAP / 2, R2 - 8] },
    { from: "organize", to: "enrich" },
    { from: "enrich", to: "rek" },
    { from: "signal", to: "ddb", muted: true, label: "media · groups · trips (signal, organize)", at: [mid(1), R2 + H / 2 - 12] },
    { from: "organize", to: "bus", label: "trip.detected · contribution.received", at: [X(2) - 20, 504],
      points: [[X(3) + 30, R2 + H], [X(3) + 30, 516], [X(1) + W - 30, 516], [X(1) + W - 30, R3]] },
    { from: "bus", to: "agent", label: "EventBridge rule invokes the agent: Workflows #1 Curator, #4 Dedup, #6 Merge", at: [mid(3) - 20, 534],
      points: [[X(1) + W - 10, R3], [X(1) + W - 10, 548], [mid(4), 548], [mid(4), R3]] },
    { from: "agent", to: "gateway", label: "MCP tools", at: [X(4) - GAP / 2, R3 + H + 14] },
    { from: "alexa", to: "gateway", muted: true, label: "Ask, Share → Workflows #2, #5", at: [X(5) - 10, R3 - 38],
      points: [[X(5), R3 + 12], [X(4) + W + 8, R3 + 12], [X(4) + W + 8, R3 - 24], [X(3) + W - 40, R3 - 24], [X(3) + W - 40, R3]] },
    { from: "gateway", to: "api", muted: true, label: "Lambda targets: the same API Lambda, no OpenAPI, no tunnel", at: [mid(1) + 60, R3 + H + 84],
      points: [[X(3) + W - 20, R3 + H], [X(3) + W - 20, R3 + H + 70], [20, R3 + H + 70], [20, R1 + H / 2], [X(0), R1 + H / 2]] },
    { from: "review", to: "gateway", color: "red", label: "apply_dedup · delete_media · share_album · remove_contributor: without the token, Policy denies", at: [mid(2) + 20, R3 + H + 54],
      points: [[mid(0), R3 + H], [mid(0), R3 + H + 40], [mid(3), R3 + H + 40], [mid(3), R3 + H]] },
  ],
  legend: ["det", "llm", "human", "security", "store", "ext"],
  notes: [
    "Ingest runs per object (signal) and per household after a batch (organize, enrich keepers, organize again); non-keepers never reach a model.",
    "Workflow #3 Household Patterns runs on a nightly schedule; the schedule rule is not in the stack yet.",
    "The Gateway's Cedar policy requires a confirmation token minted by the TV on apply_dedup, delete_media, share_album, remove_contributor; agents cannot mint one.",
  ],
});
