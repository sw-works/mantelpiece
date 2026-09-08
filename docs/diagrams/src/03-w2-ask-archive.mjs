import { diagram, node } from "./svg.mjs";
import { W, GAP, H, X, R, WIDTH } from "./_grid.mjs";
// W2 Ask Your Archive — workflows.py::ask_archive, runtime.py::invoke (prompt payload). Drawn 2026-09-08.
export default () => diagram({
  width: WIDTH, height: 400,
  title: "W2 · Ask Your Archive — the model only turns words into a query",
  subtitle: "Retrieval and the playlist are deterministic; the same path serves the TV Ask screen and Alexa+.",
  nodes: [
    node("prompt", X(0), R(0), W, H, "prompt",          { kind: "ext", sub: "TV Ask screen or Alexa+\noptional trip scope" }),
    node("intent", X(1), R(0), W, H, "model.complete",  { kind: "llm", sub: "INTENT_SYSTEM: one line of\nlowercase keywords" }),
    node("search", X(2), R(0), W, H, "search_media",    { sub: "scene · trip · date · people\nper-trip cache first" }),
    node("none",   X(2), R(1), W, H, "nothing found",   { kind: "blocked", sub: "count 0, no playlist" }),
    node("pl",     X(3), R(0), W, H, "create_playlist", { sub: "name = prompt[:60]\nsource = ask:<query>" }),
    node("push",   X(4), R(0), W, H, "playlist.created", { kind: "store", sub: "household feed" }),
    node("tv",     X(5), R(0), W, H, "TV plays it",     { kind: "terminal", sub: "Alexa+: voice answer + Echo Show" }),
  ],
  edges: [
    { from: "prompt", to: "intent" },
    { from: "intent", to: "search", label: "query", at: [X(1) + W + 14, R(0) - 10] },
    { from: "search", to: "none", color: "red", label: "no hits", at: [X(2) + W / 2 + 40, R(0) + H + 30] },
    { from: "search", to: "pl", label: "media ids", at: [X(2) + W + 14, R(0) - 10] },
    { from: "pl", to: "push" },
    { from: "push", to: "tv" },
  ],
  legend: ["det", "llm", "store", "ext", "blocked", "terminal"],
  notes: ["Why not let the model pick photos: the within-trip answer budget is 300 ms warm (F-303); vector search meets it, a second model call does not."],
});

