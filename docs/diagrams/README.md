# Diagrams

SVG source, built by hand on explicit coordinates so diffs are meaningful and the visual
encoding holds across the set. Grey is deterministic code, blue is a model call, amber is a
human decision, red is a security service, dashed is a store or queue, dotted is an external
system.

## Workflow diagrams

Drawn from the code as built on 2026-09-08. Dense by design; for checking a claim against source.

| # | Draws | Source |
|---|-------|--------|
| 01 | Topology: clients, API, S3, ingest state machines, DynamoDB, EventBridge, AgentCore, Alexa+, and the confirmation-token gate | `services/lambdas/api`, `services/lambdas/ingest`, `infra/lib/memorylane-stack.ts`, `agents/src/memorylane_agents` |
| 02 | W1 Trip Curator: one model call, deterministic fact check, retry once, withhold, playlist and events either way | `agents/.../workflows.py::trip_curator`, `narrative.py` |
| 03 | W2 Ask Your Archive: model turns words into a query, retrieval and playlist are deterministic | `workflows.py::ask_archive`, `runtime.py::invoke` |
| 04 | W3 Household Patterns: nightly tag weights in Memory, favourite tag drives the ambient set, no model | `workflows.py::household_patterns` |
| 05 | W4 Dedup Steward: propose, Review Changes on the remote, Cedar policy, apply only with the token | `workflows.py::dedup_steward`, `core::confirmation`, `api handlers/confirmations.rs` |
| 06 | W5 Share: W2 path plus the same token gate as deletion | `workflows.py::share` |
| 07 | W6 Trip Merge: clock alignment and regrouping in Rust ingest, agent re-tells the story with credits | `lambdas/ingest/src/organize.rs`, `workflows.py::trip_merge` |

Shared grid for 02 to 07 lives in `src/_grid.mjs`.

## Regenerate

```sh
node docs/diagrams/src/build.mjs        # all
node docs/diagrams/src/build.mjs 01     # by filename fragment
```

SVG needs nothing. PNG needs playwright (`npm i -g playwright && npx playwright install chromium`).
