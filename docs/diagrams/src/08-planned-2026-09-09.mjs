import { diagram, node } from "./svg.mjs";
// Planned delta over 01-topology — specs/001-memorylane-baseline (plan, contracts). Drawn 2026-09-09
// from the spec, not from device runs: what the baseline adds around the built topology.
const W = 200, GAP = 36, H = 64;
const X = (i) => 40 + i * (W + GAP);
const R0 = 96, R1 = 250, R2 = 404, R3 = 610;
const mid = (i) => X(i) + W / 2;

export default () => diagram({
  width: 1460, height: 900,
  title: "MemoryLane · Baseline delta 2026-09-09 — a second TV, reactions and voice from the phone, Alexa+ context, a wider token gate",
  subtitle: "New clients on the left, new paths in red and blue. The Gateway is still the only way an agent or Alexa+ touches data; the token gate now covers co-editors and quick cleanup.",
  groups: [
    { x: X(0) - 18, y: R0 - 44, w: 3 * W + 2 * GAP + 36, h: H + 66, label: "Clients · new in the baseline" },
    { x: X(3) - 18, y: R3 - 44, w: 3 * W + 2 * GAP + 36, h: H + 66, label: "AgentCore · Alexa+" },
  ],
  nodes: [
    node("tv2",      X(0), R0, W, H, "Member TV",          { sub: "joins by trip code (F-162)\nread-only trip, own pins and hides" }),
    node("phone",    X(1), R0, W, H, "Phone: react + voice", { sub: "likes, comments (F-710)\nhold-to-talk question (F-405)" }),
    node("share",    X(2), R0, W, H, "Shared album page",  { kind: "ext", sub: "s.html?s= · seven days\nno session (F-601)" }),
    node("api",      X(0), R1, 2 * W + GAP, H, "Backend API", { sub: "memberships · reactions · presence · ask\nsearch · playlists · shares · audit · .well-known" }),
    node("feed",     X(2), R1, W, H, "Household feed",     { kind: "store", sub: "fan-out to member TVs\nreaction.received · answer.card" }),
    node("ddb",      X(0), R2, W, H, "DynamoDB",           { kind: "store", sub: "+ MB# M# R# PE# PL# SL# P# AU#\nvectors per household" }),
    node("enrich",   X(2), R2, W, H, "enrich (keepers)",   { sub: "labels · text · description\nfaces (opt-in) · embedding · cleanup_reason" }),
    node("nightly",  X(3), R2, W, H, "nightly.tick",       { kind: "store", sub: "Scheduler 03:00 → bus\nno Lambda on a timer" }),
    node("memories", X(4), R2, W, H, "memories",           { sub: "on this day · then and now\nbest of · purge_expired" }),
    node("polly",    X(5), R2, W, H, "Polly · Location",   { kind: "llm", sub: "narration behind a flag (R10)\nstatic route maps (F-707)" }),
    node("review",   X(0), R3, W, H, "Review Changes",     { kind: "human", sub: "proposals live on the TV\nco-editor may confirm RemoveMedia" }),
    node("cedar",    X(1), R3, W, H, "Cedar policy",       { kind: "security", sub: "+ apply_cleanup\n+ RemoveMedia by role" }),
    node("gateway",  X(3), R3, W, H, "Gateway (MCP)",      { kind: "security", sub: "22 tools · CUSTOM_JWT\nallowed clients: web, AlexaPlus" }),
    node("agent",    X(4), R3, W, H, "Concierge",          { kind: "llm", sub: "+ describe_photo · quick_cleanup\n+ memories_nightly · spans per run and tool" }),
    node("alexa",    X(5), R3, W, H, "Alexa+",             { kind: "ext", sub: "OAuth 2.1 PKCE · memorylane/tools\nget_now_showing → describe_photo" }),
  ],
  edges: [
    { from: "tv2", to: "api", label: "join · leave · reactions", at: [mid(0) + 30, R0 + H + 16] },
    { from: "phone", to: "api" },
    { from: "share", to: "api", muted: true, label: "GET /s/{share}", at: [mid(2) + 20, R0 + H + 16] },
    { from: "api", to: "feed", label: "putPresence · ask · reactions", at: [X(2) - GAP / 2, R1 - 8] },
    { from: "feed", to: "tv2", muted: true, label: "member feed: contribution · memory.ready · reaction · media.removed", at: [mid(1) + 60, R1 - 30],
      points: [[mid(2), R1], [mid(2), R1 - 20], [mid(0) + 40, R1 - 20], [mid(0) + 40, R0 + H]] },
    { from: "api", to: "ddb" },
    { from: "enrich", to: "ddb", muted: true, label: "descriptions, embeddings, people", at: [mid(1), R2 + H / 2 - 12] },
    { from: "nightly", to: "memories" },
    { from: "memories", to: "polly", muted: true },
    { from: "nightly", to: "agent", color: "blue", label: "W3 patterns · memories_nightly · purge_expired", at: [mid(3) + 40, R2 + H + 40],
      points: [[mid(3), R2 + H], [mid(3), R2 + H + 26], [mid(4) + 20, R2 + H + 26], [mid(4) + 20, R3]] },
    { from: "alexa", to: "gateway", color: "blue", label: "PKCE token from Cognito; /.well-known on the API names the endpoint", at: [X(5) - 10, R3 - 38],
      points: [[X(5), R3 + 12], [X(4) + W + 8, R3 + 12], [X(4) + W + 8, R3 - 24], [X(3) + W - 40, R3 - 24], [X(3) + W - 40, R3]] },
    { from: "agent", to: "gateway", label: "MCP tools", at: [X(4) - GAP / 2, R3 + H + 14] },
    { from: "gateway", to: "api", muted: true, label: "same API Lambda; presence lets Alexa+ answer \"that statue\"", at: [mid(1) + 60, R3 + H + 84],
      points: [[X(3) + W - 20, R3 + H], [X(3) + W - 20, R3 + H + 70], [20, R3 + H + 70], [20, R1 + H / 2], [X(0), R1 + H / 2]] },
    { from: "review", to: "cedar", color: "red", label: "token per action", at: [X(1) - GAP / 2, R3 - 8] },
    { from: "cedar", to: "gateway", color: "red", label: "apply_dedup · apply_cleanup · delete_media · share_album · remove_contributor", at: [mid(2), R3 + H + 30],
      points: [[X(1) + W, R3 + H / 2], [X(3), R3 + H / 2]] },
  ],
  legend: ["det", "llm", "human", "security", "store", "ext"],
  notes: [
    "Everything drawn here is code-complete with in-memory tests on 2026-09-09; device behaviour, the deployed stack and the Alexa+ registration are not verified.",
    "The audit log (AU#) records sign-in, shares, deletions, memberships and every gated tool route with the confirmation it carried (F-605, SC-006).",
    "Narration audio depends on a system-deployed media port on Vega; without it the slideshow shows the story as captions and the flag stays off.",
  ],
});
