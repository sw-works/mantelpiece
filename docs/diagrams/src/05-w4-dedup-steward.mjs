import { diagram, node } from "./svg.mjs";
import { W, GAP, H, X, R, mid, WIDTH } from "./_grid.mjs";
// Workflow #4 Dedup Steward — workflows.py::dedup_steward, core::confirmation, api handlers/confirmations.rs. Drawn 2026-09-08.
export default () => diagram({
  width: WIDTH, height: 500,
  title: "Workflow #4 · Dedup Steward — the agent proposes, the remote decides, the policy enforces",
  subtitle: "Nothing is deleted without a single-use token that only the TV's Review Changes modal can mint.",
  nodes: [
    node("trig",   X(0), R(0), W, H, "dedup.proposal",      { kind: "ext", sub: "from organize, after a batch" }),
    node("list",   X(1), R(0), W, H, "list_duplicates",     { sub: "burst groups with keepers\nremovable = members − 1" }),
    node("noop",   X(1), R(1), W, H, "nothing to do",       { kind: "terminal", sub: "removable = 0" }),
    node("push",   X(2), R(0), W, H, "dedup.proposal → feed", { kind: "store", sub: "groups, removable count" }),
    node("review", X(3), R(0), W, H, "Review Changes",      { kind: "human", sub: "before/after counts, samples\nconfirm on the remote" }),
    node("kept",   X(3), R(1), W, H, "proposed, not applied", { kind: "blocked", sub: "no token; groups stay" }),
    node("policy", X(4), R(0), W, H, "Gateway Policy",      { kind: "security", sub: "Cedar: apply_dedup needs\na valid confirmation token" }),
    node("apply",  X(5), R(0), W, H, "apply_dedup",         { sub: "token redeemed once\nnon-keepers removed" }),
    node("done",   X(5), R(1), W, H, "removed N, keepers stay", { kind: "terminal" }),
  ],
  edges: [
    { from: "trig", to: "list" },
    { from: "list", to: "noop", muted: true, label: "no groups", at: [mid(1) + 44, R(0) + H + 30] },
    { from: "list", to: "push", label: "groups", at: [X(1) + W + 14, R(0) - 10] },
    { from: "push", to: "review", label: "TV opens the modal", at: [X(2) + W + 14, R(0) + H + 12] },
    { from: "review", to: "kept", color: "red", label: "cancel or timeout", at: [mid(3) + 66, R(0) + H + 30] },
    { from: "review", to: "policy", label: "token", at: [X(3) + W + 14, R(0) - 10] },
    { from: "policy", to: "apply", label: "allowed", at: [X(4) + W + 14, R(0) - 10] },
    { from: "apply", to: "done" },
  ],
  legend: ["det", "human", "security", "store", "ext", "blocked", "terminal"],
  notes: [
    "Why a token and not a flag: an agent can set confirm=true; it cannot produce a value the TV signed five minutes ago for exactly these group ids (F-504).",
  ],
});
