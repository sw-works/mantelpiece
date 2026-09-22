import { diagram, node } from "./svg.mjs";
import { W, GAP, H, X, R, mid, WIDTH } from "./_grid.mjs";
// Workflow #1 Trip Curator — agents/src/mantelpiece_agents/workflows.py::trip_curator, narrative.py. Drawn 2026-09-08.
export default () => diagram({
  width: WIDTH, height: 500,
  title: "Workflow #1 · Trip Curator — the story is checked against the metadata before it is saved",
  subtitle: "One model call writes the narrative; deterministic code decides whether it may be shown. The playlist and the TV event happen either way.",
  nodes: [
    node("trig",  X(0), R(0), W, H, "trip.detected",    { kind: "ext", sub: "EventBridge rule → Runtime\nrouter.handle_event" }),
    node("get",   X(1), R(0), W, H, "get_trip",         { sub: "Gateway tool" }),
    node("facts", X(2), R(0), W, H, "prompt_for · facts_for", { sub: "places, dates, people\nfrom metadata only" }),
    node("model", X(3), R(0), W, H, "model.complete",   { kind: "llm", sub: "SYSTEM: use only what is listed\ntwo or three sentences per day" }),
    node("check", X(4), R(0), W, H, "check_narrative",  { sub: "years · months · capitalised names\nagainst the facts" }),
    node("save",  X(5), R(0), W, H, "save_narrative",   { sub: "text + model_id + generated_at" }),
    node("held",  X(4), R(1), W, H, "story withheld",   { kind: "blocked", sub: "trip keeps no narrative" }),
    node("pl",    X(5), R(1), W, H, "create_playlist",  { sub: "highlights[:12]\nsource = trip_curator:<trip>" }),
    node("push",  X(5), R(2), W, H, "memory.ready · playlist.created", { kind: "store", sub: "household feed" }),
    node("tv",    X(4), R(2), W, H, "TV: New memory",   { kind: "terminal" }),
  ],
  edges: [
    { from: "trig", to: "get" },
    { from: "get", to: "facts" },
    { from: "facts", to: "model" },
    { from: "model", to: "check" },
    { from: "check", to: "save", label: "no problems", at: [X(4) + W + GAP / 2, R(0) - 10] },
    { from: "check", to: "model", color: "red", label: "problems → retry once, problems appended to the prompt", at: [X(3) + W - 10, R(0) + H + 32],
      points: [[X(4) + 30, R(0) + H], [X(4) + 30, R(0) + H + 22], [X(3) + 60, R(0) + H + 22], [X(3) + 60, R(0) + H]] },
    { from: "check", to: "held", color: "red", label: "still problems, 2 attempts", at: [X(4) + W - 30 + 66, R(1) - 12],
      points: [[X(4) + W - 30, R(0) + H], [X(4) + W - 30, R(1)]] },
    { from: "save", to: "pl" },
    { from: "held", to: "pl", muted: true, label: "playlist still built", at: [X(4) + W + GAP / 2, R(1) + H + 14] },
    { from: "pl", to: "push" },
    { from: "push", to: "tv" },
  ],
  legend: ["det", "llm", "store", "ext", "blocked", "terminal"],
  notes: [
    "Why withhold rather than edit: a story that names a place the family never went is worse than no story; the TV shows 'being written' until the next run.",
  ],
});
