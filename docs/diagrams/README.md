# Diagrams

SVG source, built by hand on explicit coordinates so diffs are meaningful and the visual
encoding holds across the set. Grey is deterministic code, blue is a model call, amber is a
human decision, red is a security service, dashed is a store or queue, dotted is an external
system.

## Workflow diagrams

Drawn from the code as built on 2026-09-08; 01 redrawn from the code as built on 2026-10-21.
Dense by design; for checking a claim against source.

| # | Draws | Source |
|---|-------|--------|
| 01 | Topology: TV, phone pages on CloudFront, API, S3, live video through IVS and saved broadcasts, both ingest state machines (signal and poster; settle, organize, enrich keepers, organize again, field notes, sketch maps) with the models and open map and encyclopedia data they call, day films on one-time Scheduler schedules, DynamoDB, EventBridge and the one event its rule forwards, the nightly tick, AgentCore, Alexa+, and the confirmation-token gate | `infra/lib/mantelpiece-stack.ts`, `services/lambdas/{api,ingest,poster,recording,film}`, `services/crates/store/src/{vision,geography,encyclopedia}.rs`, `agents/src/mantelpiece_agents` |
| 02 | Workflow #1 Trip Curator: one model call, deterministic fact check, retry once, withhold, playlist and events either way | `agents/.../workflows.py::trip_curator`, `narrative.py` |
| 03 | Workflow #2 Ask Your Archive: model turns words into a query, retrieval and playlist are deterministic | `workflows.py::ask_archive`, `runtime.py::invoke` |
| 04 | Workflow #3 Household Patterns: nightly tag weights in Memory, favourite tag drives the ambient set, no model | `workflows.py::household_patterns` |
| 05 | Workflow #4 Dedup Steward: propose, Review Changes on the remote, Cedar policy, apply only with the token | `workflows.py::dedup_steward`, `core::confirmation`, `api handlers/confirmations.rs` |
| 06 | Workflow #5 Share: Workflow #2 path plus the same token gate as deletion | `workflows.py::share` |
| 07 | Workflow #6 Trip Merge: clock alignment and regrouping in Rust ingest, agent re-tells the story with credits | `lambdas/ingest/src/organize.rs`, `workflows.py::trip_merge` |
| 08 | Baseline delta 2026-09-09: member TV, phone reactions and voice, shared album page, nightly path, Alexa+ OAuth context path, the wider token gate and the audit log. Drawn from the spec and code, not from device runs | `specs/001-mantelpiece-baseline/`, `lambdas/api/src/app.rs`, `infra/lib/mantelpiece-stack.ts` |

Shared grid for 02 to 07 lives in `src/_grid.mjs`.

## Regenerate

```sh
node docs/diagrams/src/build.mjs        # all
node docs/diagrams/src/build.mjs 01     # by filename fragment
```

SVG needs nothing. PNG needs playwright (`npm i -g playwright && npx playwright install chromium`).
