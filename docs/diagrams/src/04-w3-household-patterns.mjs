import { diagram, node } from "./svg.mjs";
import { W, GAP, H, X, R, mid, WIDTH } from "./_grid.mjs";
// W3 Household Patterns — workflows.py::household_patterns. Drawn 2026-09-08.
export default () => diagram({
  width: WIDTH, height: 520,
  title: "W3 · Household Patterns — the ambient set learns from pin and hide, with no model in the loop",
  subtitle: "Nightly. Feedback becomes tag weights in AgentCore Memory; the favourite tag drives search; the rationale is stored next to the playlist.",
  nodes: [
    node("trig",  X(0), R(0), W, H, "nightly + feedback", { kind: "ext", sub: "EventBridge schedule\npin / hide events since last run" }),
    node("read",  X(1), R(0), W, H, "read_memory",        { kind: "store", sub: "AgentCore Memory\nliked_tags" }),
    node("fold",  X(2), R(0), W, H, "fold feedback",      { sub: "pin +1 · hide −1\nper scene tag" }),
    node("write", X(3), R(0), W, H, "write liked_tags",   { kind: "store", sub: "long-term, per household" }),
    node("fav",   X(4), R(0), W, H, "favourite tag?",     { sub: "argmax over weights" }),
    node("srch",  X(5), R(0), W, H, "search_media",       { sub: "query = favourite tag" }),
    node("hl",    X(4), R(1), W, H, "recent highlights",  { sub: "list_trips → highlights[:4]\nper trip" }),
    node("pl",    X(5), R(1), W, H, "create_playlist",    { sub: "\"Ambient\"\nsource = rationale" }),
    node("why",   X(5), R(2), W, H, "write ambient_rationale", { kind: "store", sub: "so Ask can explain the choice" }),
    node("tv",    X(4), R(2), W, H, "Ambient screen",     { kind: "terminal", sub: "idle slideshow" }),
  ],
  edges: [
    { from: "trig", to: "read" },
    { from: "read", to: "fold" },
    { from: "fold", to: "write" },
    { from: "write", to: "fav" },
    { from: "fav", to: "srch", label: "yes", at: [X(4) + W + 14, R(0) - 10] },
    { from: "fav", to: "hl", label: "no weights yet", at: [mid(4) - 70, R(0) + H + 30] },
    { from: "srch", to: "pl", label: "hits", at: [X(5) + W + 2, R(0) + H + 30] },
    { from: "srch", to: "hl", muted: true, label: "no hits: fall back", at: [X(5) - 8, R(0) + H + 14],
      points: [[X(5) + 14, R(0) + H], [X(5) + 14, R(1) - 30], [X(4) + W - 30, R(1) - 30], [X(4) + W - 30, R(1)]] },
    { from: "hl", to: "pl" },
    { from: "pl", to: "why" },
    { from: "why", to: "tv" },
  ],
  legend: ["det", "store", "ext", "terminal"],
  notes: ["Why no model: the judgement here is a running tally; a model would add cost and a reason to distrust the result. Memory carries the state between nights."],
});
