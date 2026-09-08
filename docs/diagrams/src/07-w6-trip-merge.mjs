import { diagram, node } from "./svg.mjs";
import { W, GAP, H, X, R as R0, WIDTH } from "./_grid.mjs";
const R = (i) => R0(i) + 36;
// Workflow #6 Trip Merge — services/lambdas/ingest/src/organize.rs (clocks, dedup, announce) + workflows.py::trip_merge. Drawn 2026-09-08.
export default () => diagram({
  width: WIDTH, height: 500,
  title: "Workflow #6 · Trip Merge — a friend's phone joins an existing trip; the hard part is deterministic",
  subtitle: "Clock alignment, cross-phone dedup and re-clustering run in the Rust ingest task. The agent only re-tells the story with credits and tells the room.",
  groups: [
    { x: X(0) - 16, y: R(0) - 42, w: 3 * W + 2 * GAP + 32, h: H + 62, label: "Ingest · organize (Rust, per household)" },
    { x: X(3) - 16, y: R(0) - 42, w: 3 * W + 2 * GAP + 32, h: H + 62, label: "Concierge (AgentCore Runtime)" },
  ],
  nodes: [
    node("batch",  X(0), R(0), W, H, "new contributor's batch", { kind: "ext", sub: "Maya scanned the Invite QR\nuploads complete" }),
    node("align",  X(1), R(0), W, H, "align + regroup",   { sub: "clock offset from overlapping GPS\nre-dedup across phones · re-cluster days" }),
    node("emit",   X(2), R(0), W, H, "contribution.received", { kind: "store", sub: "feed + EventBridge\nonly for contributors new to the trip" }),
    node("tick",   X(3), R(0), W, H, "push to TV feed",   { sub: "\"Maya added 42 photos\"\n(F-160 ticker)" }),
    node("cur",    X(4), R(0), W, H, "trip_curator (Workflow #1)", { kind: "llm", sub: "narrative regenerated\nnames who took what" }),
    node("ready",  X(5), R(0), W, H, "memory.ready",      { kind: "terminal", sub: "trip view refreshes with credits" }),
    node("hint",   X(1), R(1), W, H, "clock hint",        { kind: "store", sub: "offset stored on the trip\nshown as a correction" }),
  ],
  edges: [
    { from: "batch", to: "align" },
    { from: "align", to: "emit" },
    { from: "align", to: "hint", muted: true },
    { from: "emit", to: "tick", label: "rule invokes the agent", at: [X(2) + W + 14, R(0) + H + 12] },
    { from: "tick", to: "cur" },
    { from: "cur", to: "ready" },
  ],
  legend: ["det", "llm", "store", "ext", "terminal"],
  notes: ["Why the split: timestamps from five phones disagree by minutes; fixing that is arithmetic over GPS pairs, not judgement, so it never waits on a model."],
});

