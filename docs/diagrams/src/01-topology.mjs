import { diagram, node } from "./svg.mjs";
// Topology as built — services/lambdas/{api,ingest,poster,recording,film},
// infra/lib/mantelpiece-stack.ts, agents/src/mantelpiece_agents. Drawn 2026-09-08,
// redrawn 2026-10-21 (field notes, sketch maps, day films, live video, CloudFront).
const W = 200, GAP = 36, H = 64;
const X = (i) => 40 + i * (W + GAP);
const mid = (i) => X(i) + W / 2;
const YT = 100;                 // corridor above the clients row
const R0 = 124, R1 = 266;       // clients, backend
const GY = 346;                 // ingest group top
const RS = 384, YM = 486;       // signal lane, gap between the lanes
const RO = 524;                 // organize lane
const Y0 = 624, Y1 = 646, Y2 = 666;
const RV = 696;                 // models and external data
const YC = 800;                 // bus to Concierge
const R3 = 860;                 // events, review, AgentCore

export default () => diagram({
  width: 1720, height: 1150,
  title: "Mantelpiece · Topology — one QR, one hosted backend, one governed tool surface",
  subtitle: "Phones upload straight to S3; ingest is deterministic Rust except where keepers and highlights reach a model; agents reach data only through the Gateway, and irreversible tools only with a token a person minted.",
  groups: [
    { x: X(1) - 18, y: GY, w: X(6) + W + 18 - (X(1) - 18), h: RO + H + 20 - GY, label: "Step Functions · Ingest" },
    { x: X(3) - 18, y: R3 - 44, w: 2 * W + GAP + 36, h: H + 66, label: "AgentCore" },
  ],
  nodes: [
    node("tv",        X(0), R0, W, H, "Fire TV app",          { sub: "RN for Vega · fetch only · /events\nreels · day films · live HLS" }),
    node("ivs",       X(1), R0, W, H, "Amazon IVS",           { sub: "low-latency channel per broadcast\nmade and deleted by the API" }),
    node("phone",     X(2), R0, W, H, "Phone pages",          { sub: "PWA · pair · upload · go live\nresumable multipart" }),
    node("s3",        X(3), R0, W, H, "S3 media",             { kind: "store", sub: "originals/ · renditions/ · sketch/\nfilms/ · music/ · manifests/" }),
    node("recording", X(4), R0, W, H, "recording",            { sub: "saved broadcast → MP4 (ffmpeg)\nthen starts OrganizeMachine" }),

    node("api",       X(0), R1, 2 * W + GAP, H, "Backend API", { sub: "API Gateway + Lambda (Rust)\npairing · uploads · /events · /ask · reel plans · films · live · confirmations" }),
    node("cdn",       X(2), R1, W, H, "CloudFront",           { sub: "link + upload pages\nprivate S3 web bucket" }),
    node("sched",     X(5), R1, W, H, "Scheduler · day films", { kind: "store", sub: "one-time film-* schedules\n21:00 day-local · self-deleting" }),
    node("film",      X(6), R1, W, H, "film",                 { sub: "plans the day's reel → MP4 (ffmpeg)\nqueued by its schedule\nor by POST /trips/{trip}/film" }),

    node("signal",    X(3), RS, W, H, "Signal",               { sub: "SignalMachine, per original\nsha256 · EXIF · pHash · renditions" }),
    node("poster",    X(4), RS, W, H, "Poster",               { sub: "videos only (NeedsPoster)\nffmpeg layer · poster frame" }),

    node("ddb",       X(0), RO, W, H, "DynamoDB",             { kind: "store", sub: "single table, pk = household\nmedia · trips · codes · feed" }),
    node("settle",    X(1), RO, W, H, "SettleUploads",        { sub: "OrganizeMachine, per household\nWait 30 s" }),
    node("org1",      X(2), RO, W, H, "OrganizeFirst",        { sub: "bursts · keepers · clocks\ntrips · routes · highlights" }),
    node("enrich",    X(3), RO, W, H, "EnrichKeepers",        { sub: "Distributed Map · enrich\nkeepers only; bursts inherit" }),
    node("org2",      X(4), RO, W, H, "OrganizeFinal",        { sub: "organize again\nwith the model results" }),
    node("notes",     X(5), RO, W, H, "FieldNotes",           { sub: "Distributed Map · field_note\nhighlights (F-711)" }),
    node("sketch",    X(6), RO, W, H, "SketchMaps",           { sub: "Distributed Map · sketch_map\ntrip and each day (F-712)" }),

    node("wiki",      X(2), RV, W, H, "Wikipedia",            { kind: "ext", sub: "landmark facts, credited\ncached for every household" }),
    node("rek",       X(3), RV, W, H, "Rekognition",          { kind: "llm", sub: "labels · faces · text\nface search when People is on" }),
    node("nova",      X(4), RV, W, H, "Bedrock · Nova Lite",  { kind: "llm", sub: "keeper descriptions (enrich)\nsubject boxes (field notes)" }),
    node("tiles",     X(5), RV, 2 * W + GAP, H, "Open map tiles", { kind: "ext", sub: "OpenFreeMap vector tiles (OpenStreetMap)\nTerrarium terrain tiles · AWS Open Data" }),

    node("review",    X(0), R3, W, H, "Review Changes",       { kind: "human", sub: "TV modal or co-editor phone\nmints a single-use token" }),
    node("bus",       X(1), R3, W, H, "EventBridge bus",      { kind: "store", sub: "trip.detected · contribution.received\ncleanup.candidates · nightly.tick" }),
    node("nightly",   X(2), R3, W, H, "Scheduler · nightly",  { kind: "store", sub: "cron 03:00 UTC\nputs nightly.tick on the bus" }),
    node("gateway",   X(4), R3, W, H, "Gateway + Policy",     { kind: "security", sub: "Identity (Cognito) · Cedar\ninterceptor seals the caller" }),
    node("agent",     X(3), R3, W, H, "Concierge",            { kind: "llm", sub: "Strands on AgentCore Runtime\nAmazon Nova Pro on Bedrock\nWorkflows #1–#6" }),
    node("alexa",     X(5), R3, W, H, "Alexa+",               { kind: "ext", sub: "MCP add-on (OAuth)\nsame Gateway endpoint\nAsk · Share as tools" }),
  ],
  edges: [
    // Clients and the backend.
    { from: "tv", to: "api", points: [[mid(0), R0 + H], [mid(0), R1 - 2]] },
    { from: "phone", to: "api", points: [[X(2) + 60, R0 + H], [X(2) + 60, 226], [X(1) + W - 24, 226], [X(1) + W - 24, R1 - 2]] },
    { from: "cdn", to: "phone", label: "pages", at: [mid(2) + 66, 244], points: [[mid(2) + 40, R1], [mid(2) + 40, R0 + H + 2]] },
    { from: "api", to: "ivs", points: [[mid(1), R1], [mid(1), R0 + H + 2]] },
    { from: "phone", to: "ivs", label: "broadcast", at: [X(2) - 18, R0 + H + 16] },
    { from: "ivs", to: "tv", label: "live HLS", at: [X(1) - 18, R0 + H + 16] },
    { from: "phone", to: "s3", label: "PUT parts", at: [X(3) - 18, R0 + H + 16] },
    { from: "ivs", to: "recording", muted: true, label: "saved broadcast: IVS writes HLS to the Recordings bucket → recording-ended rule", at: [mid(2) + 120, YT],
      points: [[mid(1), R0], [mid(1), YT], [mid(4), YT], [mid(4), R0 - 2]] },
    { from: "recording", to: "s3", label: "MP4 into originals/", at: [X(4) - 18, R0 + H + 16] },
    { from: "film", to: "s3", label: "MP4 into films/", at: [mid(5) - 10, 230],
      points: [[mid(6), R1], [mid(6), 230], [mid(3) + 40, 230], [mid(3) + 40, R0 + H + 2]] },
    { from: "sched", to: "film" },

    // Ingest.
    { from: "s3", to: "signal", label: "Object Created → SignalMachine", at: [mid(3), 306],
      points: [[mid(3), R0 + H], [mid(3), RS - 2]] },
    { from: "signal", to: "poster", label: "videos", at: [X(4) - 18, RS + H + 16] },
    { from: "api", to: "settle", muted: true, label: "upload batch complete\n→ StartExecution", at: [X(1) + W - 20, RS + 34],
      points: [[X(1) + W - 20, R1 + H], [X(1) + W - 20, RO - 2]] },
    { from: "signal", to: "org1", muted: true, label: "signals", at: [X(3) - 14, YM],
      points: [[X(3) + 24, RS + H], [X(3) + 24, YM], [X(2) + W - 24, YM], [X(2) + W - 24, RO - 2]] },
    { from: "settle", to: "org1" },
    { from: "org1", to: "enrich" },
    { from: "enrich", to: "org2" },
    { from: "org2", to: "notes" },
    { from: "notes", to: "sketch" },
    { from: "org2", to: "sched", label: "organize upserts one-time\nfilm-* schedules", at: [mid(5), RS + 34],
      points: [[X(4) + W - 30, RO], [X(4) + W - 30, YM], [mid(5), YM], [mid(5), R1 + H + 2]] },

    // Stores and events out of ingest.
    { from: "api", to: "ddb", points: [[mid(0), R1 + H], [mid(0), RO - 2]] },
    { from: "org1", to: "ddb", muted: true, label: "media · groups · trips", at: [mid(1), Y0],
      points: [[X(2) + 20, RO + H], [X(2) + 20, Y0], [mid(0) + 40, Y0], [mid(0) + 40, RO + H + 2]] },
    { from: "org1", to: "bus", label: "trip.detected · contribution.received", at: [mid(1), RV + H + 22],
      points: [[X(2) + 50, RO + H], [X(2) + 50, Y2], [mid(1), Y2], [mid(1), R3 - 2]] },

    // Models and external data.
    { from: "enrich", to: "wiki", points: [[X(3) + 24, RO + H], [X(3) + 24, Y1], [X(2) + W - 40, Y1], [X(2) + W - 40, RV - 2]] },
    { from: "enrich", to: "rek", points: [[mid(3), RO + H], [mid(3), RV - 2]] },
    { from: "enrich", to: "nova", points: [[X(3) + W - 24, RO + H], [X(3) + W - 24, Y1], [X(4) + 30, Y1], [X(4) + 30, RV - 2]] },
    { from: "notes", to: "nova", points: [[X(5) + 24, RO + H], [X(5) + 24, Y1], [X(4) + W - 30, Y1], [X(4) + W - 30, RV - 2]] },
    { from: "notes", to: "tiles", points: [[mid(5), RO + H], [mid(5), RV - 2]] },
    { from: "sketch", to: "tiles", points: [[mid(6), RO + H], [mid(6), RV - 2]] },

    // Events, agents and the token gate.
    { from: "nightly", to: "bus" },
    { from: "bus", to: "agent", label: "rule forwards trip.detected only → Workflow #1 Trip Curator", at: [(X(1) + W - 10 + mid(3)) / 2, YC],
      points: [[X(1) + W - 10, R3], [X(1) + W - 10, YC], [mid(3), YC], [mid(3), R3 - 2]] },
    { from: "agent", to: "gateway", label: "MCP tools", at: [X(4) - 18, R3 - 14] },
    { from: "alexa", to: "gateway", muted: true },
    { from: "gateway", to: "api", muted: true, label: "Lambda target: the same API Lambda", at: [mid(1) + 60, R3 + H + 84],
      points: [[X(4) + W - 40, R3 + H], [X(4) + W - 40, R3 + H + 70], [20, R3 + H + 70], [20, R1 + H / 2], [X(0) - 2, R1 + H / 2]] },
    { from: "review", to: "gateway", color: "red", label: "apply_dedup · apply_cleanup · delete_media · share_album · remove_contributor: without the token, Policy denies", at: [mid(2) - 20, R3 + H + 54],
      points: [[mid(0), R3 + H], [mid(0), R3 + H + 40], [mid(4) - 20, R3 + H + 40], [mid(4) - 20, R3 + H + 2]] },
  ],
  legend: ["det", "llm", "human", "security", "store", "ext"],
  notes: [
    "SignalMachine runs per original; OrganizeMachine runs per household after an upload batch or a saved broadcast. Model calls go to keepers (enrich) and highlights (field notes); the rest of a burst inherits.",
    "The Concierge rule forwards trip.detected only. The agent's router also handles contribution.received, dedup.proposal, cleanup.candidates and nightly.tick; no rule forwards those yet.",
    "The Gateway's Cedar policy requires a confirmation token on apply_dedup, apply_cleanup, delete_media, share_album, remove_contributor; an owner's or co-editor's session mints it, never an agent.",
    "Place names come from Amazon Location Places. The Echo Show custom skill (a Python Lambda) calls the same API with an account-linked token.",
  ],
});
