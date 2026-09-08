import { diagram, node } from "./svg.mjs";
import { W, GAP, H, X, R, mid, WIDTH } from "./_grid.mjs";
// W5 Share / Export — workflows.py::share (reuses ask_archive). Drawn 2026-09-08.
export default () => diagram({
  width: WIDTH, height: 500,
  title: "W5 · Share — \"send Kyoto highlights to grandma\" is W2 plus a confirmation",
  subtitle: "The request becomes a playlist by the Ask path; sharing it outside the household is irreversible, so it takes the same token gate as deletion.",
  nodes: [
    node("req",    X(0), R(0), W, H, "share request",     { kind: "ext", sub: "TV Ask or Alexa+" }),
    node("intent", X(1), R(0), W, H, "model.complete",    { kind: "llm", sub: "INTENT_SYSTEM → keywords" }),
    node("search", X(2), R(0), W, H, "search_media",      { sub: "W2 path: playlist created" }),
    node("none",   X(2), R(1), W, H, "nothing to share",  { kind: "blocked", sub: "no playlist" }),
    node("review", X(3), R(0), W, H, "Review Changes",    { kind: "human", sub: "share_album <playlist>\nconfirm on the remote" }),
    node("kept",   X(3), R(1), W, H, "not shared",        { kind: "blocked", sub: "no token" }),
    node("policy", X(4), R(0), W, H, "Gateway Policy",    { kind: "security", sub: "Cedar: share_album\nneeds the token" }),
    node("share",  X(5), R(0), W, H, "share_album",       { sub: "presigned, expiring link\nemail via SES (later)" }),
    node("link",   X(5), R(1), W, H, "link returned",     { kind: "terminal", sub: "read by voice or shown on TV" }),
  ],
  edges: [
    { from: "req", to: "intent" },
    { from: "intent", to: "search" },
    { from: "search", to: "none", color: "red", label: "no hits", at: [mid(2) + 40, R(0) + H + 30] },
    { from: "search", to: "review", label: "playlist id", at: [X(2) + W + 14, R(0) - 10] },
    { from: "review", to: "kept", color: "red", label: "cancel", at: [mid(3) + 36, R(0) + H + 30] },
    { from: "review", to: "policy", label: "token", at: [X(3) + W + 14, R(0) - 10] },
    { from: "policy", to: "share" },
    { from: "share", to: "link" },
  ],
  legend: ["det", "llm", "human", "security", "ext", "blocked", "terminal"],
  notes: ["Why the same gate as delete: a link that leaves the household cannot be recalled once opened; the token proves a person on the couch said yes to this album."],
});
