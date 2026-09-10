# Implementation Plan: MemoryLane Baseline

**Branch**: `001-memorylane-baseline` | **Date**: 2026-09-09 | **Spec**: [spec.md](spec.md)

**Input**: Feature specification from `/specs/001-memorylane-baseline/spec.md`

## Summary

Deliver the whole baseline (F-101 to F-710) on the stack already scaffolded: a Rust API and
ingest on Lambda behind API Gateway and Step Functions, DynamoDB single table, S3 originals,
EventBridge, AgentCore Gateway and Runtime with Strands, a React Native for Vega TV app, and
a PWA for phones. Roughly half the baseline exists as code with in-memory tests (device
code, codes and joins, resumable uploads, signal/organize/enrich, trips, pin and hide,
confirmations, workflows W1 to W6). The plan adds what the spec requires on top: the
`packages/tv-ui` split so the TV app builds for Vega, persisted TV sessions, search and
playlists over the API, memberships for a second TV, roles, reactions, People, quick
cleanup, resurfacing memories, now-showing plus `describe_photo` for Ask by voice, the
Alexa+ MCP surface with OAuth, and the deployed AWS integrations (Rekognition, embeddings,
Location, Polly) behind the existing traits. Research resolves the ten open questions in
`research.md`; each answer is tagged with how it was verified.

## Technical Context

**Language/Version**: Rust 1.93 (edition 2021) for API and ingest; TypeScript 5.9 on Node 22 with pnpm 10.20 for TV, web and CDK; Python 3.12 with uv for agents.

**Primary Dependencies**: lambda_http and cargo-lambda (arm64), aws-sdk-rust (S3, DynamoDB, Rekognition, Bedrock Runtime, Location, Polly), image and libheif for renditions; React Native for Vega (0.83 early access on SDK 0.24, else 0.72) in `apps/tv-vega`, React Native 0.87 core in `packages/tv-ui`; Vite PWA in `apps/web`; strands-agents 1.x and bedrock-agentcore SDK; aws-cdk-lib 2.268.

**Storage**: DynamoDB single table keyed by household (media, trips, codes, feed, memberships, reactions, people, presence, proposals, playlists, share links); S3 originals and renditions under `originals/{household}/{contributor}/` and `renditions/`; embeddings in DynamoDB per trip for the hackathon with S3 Vectors as the cross-trip store when available in region (research R7); Rekognition face collection per household (opt-in); AgentCore Memory per household.

**Testing**: cargo test (unit and in-memory integration in `services/lambdas/api/tests/api.rs`, `services/lambdas/ingest/tests/pipeline.rs`), jest for `packages/tv-ui`, vitest for `apps/web` and `infra`, pytest for agents with `InMemoryTools`. All under `make test`, no AWS.

**Target Platform**: Fire TV on Vega OS 1.2 (Vega Virtual Device and Fire TV Stick 4K Select); phone browsers (iOS Safari 17+, Android Chrome); AWS us-east-1 or us-west-2 (research R4); Alexa+ via MCP add-on.

**Project Type**: TV app + web app + serverless backend + agents, one pnpm/cargo/uv monorepo.

**Performance Goals**: within-trip Ask under 300 ms warm (F-303); feed events on the TV within 30 s (25 s long-poll hold); a 5,000-photo batch without orchestration limits (F-108, Distributed Map); Alexa+ tool round trip under 500 ms (research R6); trip organised within 15 minutes of the last upload for 400 photos (SC-003).

**Constraints**: no native module bundled in the TV app, system-deployed `@amazon-devices/*` ports only (constitution II); no `@amazon-devices/*` in `package.json` on `main` until CI can install it; tests without AWS (VI); irreversible tools only with a TV-minted token and Cedar (III); stories checked against metadata (IV); Lambda layer under 250 MB unzipped (libheif, ffmpeg); TV never captures audio.

**Scale/Scope**: households in the tens for the hackathon, archives to 10k photos each; 12 user stories, 11 TV screens and 6 phone pages (mockups: MemoryLane Screens artifact); 6 workflows plus 3 new tools.

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

| Principle | Gate | Pre-research | Post-design |
|-----------|------|--------------|-------------|
| I. Zero infrastructure for the user | No feature needs a hub, sideload, native phone app, or the user's own cloud account | PASS: every new feature is TV app, phone browser, or hosted backend | PASS |
| II. Vega-first TV app | No bundled native module; RN version set by the Vega SDK; platform code in `.kepler.ts` | PASS with one structural change: `apps/tv` at RN 0.87 splits into `packages/tv-ui` and `apps/tv-vega` (R1); AsyncStorage and qrcode-svg as system ports (R2) | PASS |
| III. Confirm before irreversible | Every new destructive tool has a `ConfirmedAction` variant, Review Changes UI, and a Cedar rule | PASS: new variants `ApplyCleanup`, `RemoveMedia` for co-editors, plus role checks (data-model, contracts/tools.md) | PASS |
| IV. Narrative from structured data only | Any model text shown to users is checked against metadata and stamped | PASS: `describe_photo` answers pass the same place and date check against the photo's metadata before display (R8) | PASS |
| V. Deterministic before expensive | New model calls happen only on keepers and after deterministic filters; Ask stays single-model-call | PASS: People, text detection and descriptions run on keepers at enrich; On this day, Then and now, best-of are pure queries | PASS |
| VI. Tests run without AWS | Every new backend is a trait with an in-memory implementation; agents tested with `InMemoryTools` | PASS: `Faces`, `Vision`, `Maps`, `Speech`, `Vectors` traits with memory fakes | PASS |

No violations. Complexity Tracking left empty.

## Project Structure

### Documentation (this feature)

```text
specs/001-memorylane-baseline/
├── plan.md              # This file
├── research.md          # Phase 0: ten open questions resolved
├── data-model.md        # Phase 1: entities, keys, state transitions
├── quickstart.md        # Phase 1: how to prove each user story works
├── contracts/
│   ├── api.md           # HTTP routes, existing and new
│   ├── tools.md         # Gateway tools (MCP), existing and new, Cedar rules
│   └── events.md        # Household feed events and EventBridge details
└── tasks.md             # Phase 2 (/speckit-tasks), not created here
```

### Source Code (repository root)

```text
services/                       Rust workspace (exists)
  crates/core                   domain types; add membership, role, reaction, person, presence, proposal, share_link
  crates/ingest                 signals and renditions (exists); add text detection request shaping
  crates/trips                  clustering, clocks, routes, highlights (exists); add memories.rs (on this day, then and now, best of)
  crates/store                  Storage and Blobs traits, DynamoDB, S3, memory (exists); add Vectors, Faces, Vision, Maps, Speech traits with memory fakes
  lambdas/api                   routes (exists); add search, playlists, memberships, roles, reactions, people, presence, describe, share, cleanup proposals
  lambdas/ingest                signal, organize, enrich (exists); enrich gains faces, text, description, embedding; add cleanup candidates
  lambdas/poster                NEW: video poster frames, ffmpeg layer (research R5)
agents/                         Strands workflows W1 to W6 (exists); add get_now_showing, describe_photo, quick_cleanup, memories_nightly
infra/                          one CDK stack (exists); add Rekognition and Bedrock permissions, Location, Polly, S3 Vectors (if available), AgentCore Gateway targets, nightly schedule rule, Alexa+ OAuth resource server
  policy/tools.cedar            NEW: Cedar policy for irreversible tools and roles
packages/tv-ui/                 NEW: screens, hooks, api client, theme, tests moved from apps/tv/src and apps/tv/test (RN core only)
apps/tv-vega/                   NEW: Vega shell: manifest.toml, app.json, metro, kepler scripts, RN pinned by the SDK; imports @memorylane/tv-ui
apps/tv/                        REMOVED after the split (history keeps it)
apps/web/                       PWA (exists: link, upload); add trip (read-only + reactions), ask (speech), people, uploads-mine, share pages
  lambdas/api handlers          NEW handlers: search, playlists, presence, ask, proposals, memberships, reactions, people, memories, shares, narration, wellknown (OAuth metadata)
scripts/tv-walkthrough.sh       NEW: Virtual Device walkthrough and screenshots
docs/alexa-plus.md              NEW: add-on registration steps
docs/diagrams/                  as-built diagrams (exists); add 08-planned delta when the split lands
```

**Nightly work**: the schedule rule emits `nightly.tick` to EventBridge; the Concierge is its only consumer (W3 Household Patterns, `memories_nightly`, and the `purge_expired` tool). Lambdas never run on a timer themselves.

**Structure Decision**: keep the three-language monorepo as scaffolded. The one structural
change is the TV split forced by the Vega React Native pin (R1): shared UI in
`packages/tv-ui`, a thin Vega shell in `apps/tv-vega`, and the current `apps/tv` deleted
once the shell builds on the Virtual Device. Fire OS gets its own shell later if time
allows (F-408 stretch). Backend additions extend existing crates and handlers rather than
adding services; every AWS integration sits behind a trait in `crates/store` with an
in-memory fake so `make test` stays offline.

## Phase 0 and Phase 1 outputs

- [research.md](research.md): R1 to R10, each with Decision, Rationale, Alternatives, and
  how it was verified (documentation, code, device).
- [data-model.md](data-model.md): entities from the spec mapped to the single table, with
  validation rules and state transitions.
- [contracts/api.md](contracts/api.md), [contracts/tools.md](contracts/tools.md),
  [contracts/events.md](contracts/events.md).
- [quickstart.md](quickstart.md): commands and expected outcomes per user story.

## Complexity Tracking

No constitution violations to justify.
