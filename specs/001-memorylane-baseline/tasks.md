---

description: "Task list for the MemoryLane baseline"
---

# Tasks: MemoryLane Baseline

**Input**: Design documents from `/specs/001-memorylane-baseline/`

**Prerequisites**: plan.md, spec.md, research.md, data-model.md, contracts/, quickstart.md

**Tests**: Not requested as TDD. The constitution requires `make test` to pass offline, so
every task that adds a backend or trait names the in-memory test file it extends. Existing
tests: `services/lambdas/api/tests/api.rs`, `services/lambdas/ingest/tests/pipeline.rs`,
`agents/tests/test_workflows.py`, `apps/tv/test/*.test.tsx`, `apps/web/test/*.test.ts`.

**Organization**: Tasks are grouped by user story (US1 to US12, priority order from
spec.md). About half the baseline already exists; tasks below cover the gap the plan
identified, plus the structural TV split.

## Already built (no new task; verified by existing tests, see T029)

| FR | Where | Test |
|----|-------|------|
| FR-001, FR-003 (upload types, resumable, exact duplicates) | `apps/web/src/upload/*`, `services/lambdas/api/src/handlers/uploads.rs`, `services/crates/ingest/src/checksum.rs` | `apps/web/test/upload.test.ts`, `services/lambdas/api/tests/api.rs` |
| FR-002, FR-018 (progress and activity on the TV) | `services/crates/core/src/events.rs`, `packages/tv-ui/src/hooks/useEvents.ts`, `components/Ticker.tsx` | `packages/tv-ui/test/hooks.test.tsx` |
| FR-004, FR-005, FR-007 (EXIF, renditions, HEIC, pre-model signals) | `services/crates/ingest/src/{exif,renditions,decode,phash,quality}.rs`, `services/lambdas/ingest/src/signal.rs` | `services/lambdas/ingest/tests/pipeline.rs` |
| FR-010, FR-011, FR-012 (codes, join, rotate) | `services/crates/core/src/trip_code.rs`, `services/lambdas/api/src/handlers/codes.rs` | `services/lambdas/api/tests/api.rs` |
| FR-015 (cross-contributor bursts) | `services/crates/ingest/src/burst.rs`, `services/lambdas/ingest/src/organize.rs` | `services/lambdas/ingest/tests/pipeline.rs` |
| FR-030, FR-033, FR-034, FR-036 (keepers, score, clustering, naming, highlights) | `services/crates/ingest/src/quality.rs`, `services/crates/trips/src/{cluster,naming,highlights}.rs` | unit tests in those files |
| FR-050, FR-051, FR-054, FR-055 (device code, screens, long-poll, Review Changes) | `services/lambdas/api/src/handlers/{device,events,confirmations}.rs`, `packages/tv-ui/src/screens/*` | `services/lambdas/api/tests/api.rs`, `packages/tv-ui/test/screens.test.tsx` |
| FR-056 (runs on Vega) | `apps/tv-vega/` (Phase 1 done: bundle builds in CI) | T035 on the Virtual Device |
| FR-060 (governed tools, memory, event workflows) | `agents/src/memorylane_agents/{gateway,workflows,router}.py` | `agents/tests/test_workflows.py` |
| FR-071 (per-household isolation) | DynamoDB `pk = household`, `services/lambdas/api/src/app.rs::caller` | `services/lambdas/api/tests/api.rs` |

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependencies)
- **[Story]**: Which user story this task belongs to (US1 to US12)
- Include exact file paths in descriptions

## Path Conventions

Monorepo per plan.md: `services/` (Rust), `agents/` (Python), `infra/` (CDK),
`packages/tv-ui/` and `apps/tv-vega/` (TV, new), `apps/web/` (PWA).

---

## Phase 1: Setup (TV split and tooling)

**Purpose**: make the TV app buildable for Vega (research R1, R2) and give every later
task a place to land.

- [X] T001 Create `packages/tv-ui/` with `package.json` (`@memorylane/tv-ui`, React Native core peer dep, jest 30, `test`/`lint`/`typecheck` scripts) and move `apps/tv/src/**` and `apps/tv/test/**` into it unchanged; update import paths in `packages/tv-ui/test/*.test.tsx`
- [X] T002 Create `apps/tv-vega/` shell: `package.json` with the Vega SDK React Native pin (0.83 if reachable, else 0.72), `kepler` block, `build:vega` and `build:vega:debug` scripts; `index.js` registering `App` from `@memorylane/tv-ui`; move `apps/tv/manifest.toml`, `apps/tv/app.json`, `apps/tv/metro.config.js`, `apps/tv/react-native.config.js` there
- [X] T003 Add `[os.version] target = "1.2" min = "1.2"` to `apps/tv-vega/manifest.toml` (research R1)
- [X] T004 [P] Add `packages/*` to `pnpm-workspace.yaml` and remove `apps/tv` from the workspace once T001 and T002 build; delete `apps/tv/`
- [X] T005 [P] Add Makefile targets `tv-vvd-start`, `tv-vvd-install`, `tv-vvd-logs`, `tv-vvd-shot` in `Makefile` wrapping the `vega` CLI commands from quickstart.md
- [X] T006 [P] Add `scripts/tv-walkthrough.sh` that enables Developer Mode, drives `inputd-cli button_press` through Onboarding, Home, Trip and Invite, and saves screenshots to `docs/screens/`
- [X] T007 Update `README.md` Fire TV section and `.github/workflows/ci.yml` for the new workspace layout. Found during implementation: `@amazon-devices/react-native-kepler` 4.0.1, `kepler-cli-platform` 0.22.14 and the module resolver preset are on public npm, so no `.npmrc`, registry token or `VEGA_NPM_TOKEN` secret is needed; CI runs `pnpm --filter @memorylane/tv-vega bundle:vega` after `pnpm install --frozen-lockfile`, which satisfies constitution VI before T030 lands

---

## Phase 2: Foundational (blocking prerequisites)

**Purpose**: domain types, storage traits with in-memory fakes, roles, and the Gateway
policy that every story builds on.

**⚠️ CRITICAL**: No user story work can begin until this phase is complete

- [X] T008 Add `Role { Contributor, CoEditor }` and `role` field to `Contributor` in `services/crates/core/src/contributor.rs`; validation: a guest never has `role = CoEditor`
- [X] T009 [P] Add `Membership { member_household, owner_household, trip, joined_at, left_at }` in `services/crates/core/src/membership.rs`; validation: member and owner households differ
- [X] T010 [P] Add `Presence { device_id, media, trip, day, paused, at }` with a 10-minute staleness rule in `services/crates/core/src/presence.rs`
- [X] T011 [P] Add `Proposal { id, kind, action, before, after, samples, state, requested_by, confirmation }` with transitions open → cancelled | confirmed → applied and open → expired after 24 h in `services/crates/core/src/proposal.rs`
- [X] T012 [P] Add `Reaction { trip, target: Day | Media, id, actor, kind: Like | Comment, text (comment, max 200 chars) }` in `services/crates/core/src/reaction.rs`; one like per actor per target
- [X] T013 [P] Add `Person { id, name: Option, face_ids, media_count, sample_media, merged_into }` in `services/crates/core/src/person.rs`
- [X] T014 [P] Add `ShareLink { id, target: Album | Playlist, url, expires_at (7 days default), confirmation, revoked_at }` in `services/crates/core/src/share_link.rs`
- [X] T015 Add `ConfirmedAction::ApplyCleanup { media_ids }` and `ConfirmedAction::RemoveMedia { media_ids, by }` with `tool_name()` values `apply_cleanup` and `delete_media`, plus `subject` on `ConfirmationToken`, in `services/crates/core/src/confirmation.rs`
- [X] T016 Add Media fields `pinned_by: BTreeSet<HouseholdId>`, `hidden_by`, `text`, `description { text, landmark, model_id, generated_at }`, `cleanup_reason`, `restore_until` and state `Removed` in `services/crates/core/src/media.rs`; add `owner_household`, `members`, `narrative.check: Passed | Withheld`, `TripDay.story` in `services/crates/core/src/trip.rs`
- [X] T017 Add feed kinds `cleanup.proposal`, `reaction.received`, `membership.joined`, `answer.card`, `narration.ready`, `memories.ready` to `EventKind` in `services/crates/core/src/events.rs` per contracts/events.md
- [X] T018 Extend the `Storage` trait in `services/crates/store/src/storage.rs` with membership, presence, proposal, reaction, person, share link, playlist and per-household pin/hide operations; implement in `services/crates/store/src/memory.rs` and `services/crates/store/src/dynamo.rs` (sort-key layouts from data-model.md)
- [X] T019 [P] Add `Vectors` trait (upsert, query within trip, query across household) with `MemoryVectors` in `services/crates/store/src/vectors.rs`; DynamoDB per-trip cache plus S3 Vectors implementation behind a feature flag (research R7)
- [X] T020 [P] Add `Faces` trait (create/delete collection, index, search) with `MemoryFaces` and a Rekognition implementation in `services/crates/store/src/faces.rs` (research R9)
- [X] T021 [P] Add `Vision` trait (labels, text, describe) with `MemoryVision`, moving the Rekognition client out of `services/lambdas/ingest/src/vision.rs` and adding a Bedrock multimodal `describe` in `services/crates/store/src/vision.rs` (research R8)
- [X] T022 [P] Add `Maps` trait (static route map, pins map) with `MemoryMaps` and an Amazon Location implementation in `services/crates/store/src/maps.rs`
- [X] T023 [P] Add `Speech` trait (synthesize to S3 key) with `MemorySpeech` and a Polly implementation in `services/crates/store/src/speech.rs` (research R10)
- [X] T024 Extend `caller()` in `services/lambdas/api/src/app.rs` to resolve `Caller::Member { household, trip }` from memberships and `Caller::CoEditor` from contributor role; add `owner_or_co_editor(trip)` and `member_or_owner(trip)` guards
- [X] T025 [P] Write the Cedar policy set in `infra/policy/tools.cedar` requiring a confirmation token for `apply_dedup`, `apply_cleanup`, `delete_media`, `share_album`, `remove_contributor`, and role `co_editor` or owner for `RemoveMedia`; load it in `infra/lib/memorylane-stack.ts`
- [X] T026 [P] Add `pytest` fakes for the new tools (`get_now_showing`, `describe_photo`, `list_cleanup_candidates`, `memories_for`, `apply_cleanup`) to `InMemoryTools` and the `Tools` protocol in `agents/src/memorylane_agents/tools.py`
- [X] T027 Wire new Gateway tool targets and the nightly `events.Rule` (`nightly.tick`) in `infra/lib/memorylane-stack.ts`; extend `infra/test/stack.test.ts`
- [X] T028 Extend `services/lambdas/api/tests/api.rs` with role and membership fixtures (owner, co-editor, guest, member household) used by every later story
- [X] T029 Run `make test` and tick each row of the "Already built" table above against its named test, recording any row that fails as a new task in this file (2026-09-09: every named test file exists and passes; the FR-056 row stays open until T035 runs on a device)

**Checkpoint**: `make test` green with the new types, traits and fakes (reached 2026-09-09)

---

## Phase 3: User Story 1 - Sign in the TV from the phone (Priority: P1) 🎯 MVP

**Goal**: install, scan, approve, signed in, and still signed in after a restart.

**Independent Test**: on the Vega Virtual Device, scan the QR with a phone, approve, land on Memories home; restart the app and stay signed in.

- [X] T030 [US1] Implement `AsyncStorageSessionStore` in `apps/tv-vega/src/sessionStore.ts` using the `@react-native-async-storage/async-storage` alias to `@amazon-devices/react-native-async-storage__async-storage` (package.json `npm:` alias plus `pnpm.overrides` in `apps/tv-vega/package.json` only); inject it into `App` from `apps/tv-vega/index.js`
- [X] T031 [P] [US1] Add code expiry and "Get a new code" handling to `packages/tv-ui/src/screens/Onboarding.tsx` and `packages/tv-ui/src/hooks/useDeviceSignIn.ts` (expired state after `expires_at`, one press issues a new code); extend `packages/tv-ui/test/hooks.test.tsx`
- [X] T032 [P] [US1] Make the second approval of an already-approved code return 409 "already signed in" in `services/lambdas/api/src/handlers/device.rs`; test in `services/lambdas/api/tests/api.rs`
- [X] T033 [P] [US1] After approval, redirect the phone from `apps/web/src/pages/link.ts` to the upload page with the household session (no second sign-in); test in `apps/web/test/auth.test.ts`
- [X] T034 [US1] Record TV name on `POST /device/poll` approval and expose it on `GET /me` in `services/lambdas/api/src/handlers/session.rs`
- [ ] T035 [US1] (blocked 2026-09-09: no Vega SDK on the build machine) Verify on the Vega Virtual Device via `make tv-vvd-install`: code and QR shown, approval lands on Home under 5 s, session survives restart; record results in `specs/decisions-2026-09-09.md` section 6 (research R1, R2 verified: device)

**Checkpoint**: US1 demonstrable on the Virtual Device (code complete 2026-09-09; device run pending T035)

---

## Phase 4: User Story 2 - Add photos from the phone and watch them arrive (Priority: P1)

**Goal**: resumable uploads with progress on both screens and thumbnails on the TV.

**Independent Test**: upload 50 mixed JPG, HEIC, MOV; progress on phone and TV; thumbnails within a minute; re-upload returns duplicate.

- [X] T036 [P] [US2] Add `GET /uploads/mine` listing the caller's uploads with state in `services/lambdas/api/src/handlers/uploads.rs`; test in `services/lambdas/api/tests/api.rs`
- [X] T037 [P] [US2] Show "duplicate, skipped" for 409 from `/uploads/init` and a retry state for failed parts in `apps/web/src/pages/upload.ts` and `apps/web/src/upload/queue.ts`; test in `apps/web/test/upload.test.ts`
- [X] T038 [P] [US2] Emit `upload.progress` per completed file in `services/lambdas/api/src/handlers/uploads.rs` (`done`, `total` per batch) and render it in `packages/tv-ui/src/components/Ticker.tsx`
- [X] T039 [US2] Add the poster-frame Lambda `services/lambdas/poster/` (ffmpeg layer, h264/hevc/aac only) invoked from the signal task for videos in `services/lambdas/ingest/src/signal.rs`; layer definition in `infra/lib/memorylane-stack.ts` under 250 MB unzipped (research R5)
- [X] T040 [US2] Replace the inline `sfn.Map` with a Distributed Map over an S3 manifest with a per-household concurrency cap in `infra/lib/memorylane-stack.ts` (F-108); extend `infra/test/stack.test.ts`
- [X] T041 [US2] Prove idempotent re-run: re-executing a completed batch produces no new records in `services/lambdas/ingest/tests/pipeline.rs` (F-106)

**Checkpoint**: uploads visible end to end on phone and TV (code complete 2026-09-09; the ffmpeg layer and Distributed Map are compile- and synth-checked, not deployed)

---

## Phase 5: User Story 3 - Everyone on the trip adds photos by one QR (Priority: P1)

**Goal**: trip-scoped QR, guests by name, credit everywhere, live ticker.

**Independent Test**: two phones scan the same trip QR (one Amazon, one guest "Maya"); both contributors' names show on their photos; ticker announces arrivals; expired code refuses uploads.

- [X] T042 [P] [US3] Return live `contributors` and `member_households` counts on `POST /trips/{trip}/code` and `GET /codes` in `services/lambdas/api/src/handlers/codes.rs`; show them on `packages/tv-ui/src/screens/Invite.tsx` with a Rotate code button
- [X] T043 [P] [US3] Bind guest sessions to `{code, device}` and reject a guest token used from another device or after code expiry in `services/lambdas/api/src/tokens.rs` and `handlers/codes.rs`; test in `services/lambdas/api/tests/api.rs`
- [X] T044 [P] [US3] Show contributor name on every thumbnail and in the day summary in `packages/tv-ui/src/screens/TripView.tsx` and `packages/tv-ui/src/screens/Highlights.tsx` (F-154)
- [X] T045 [US3] Expose `clock_offsets` per contributor on `GET /trips/{trip}` and render "corrected by N min" hints in `packages/tv-ui/src/screens/TripView.tsx` (F-155)
- [X] T046 [US3] Cascade `DELETE /media/{media}` by a contributor to renditions, embeddings, albums, playlists, highlights and hero in `services/lambdas/api/src/handlers/media.rs`; test in `services/lambdas/api/tests/api.rs` (F-157, edge case: hero recomputed)
- [X] T047 [US3] Implement contributor removal grace period and export link in `services/lambdas/api/src/handlers/contributors.rs` (`grace_until`, purge after) (F-158)
- [X] T048 [US3] Add the read-only trip page `apps/web/src/pages/trip.ts` and `apps/web/trip.html` gated by the trip code (narrative, days, highlights, route) (F-159); test in `apps/web/test/trip.test.ts`

**Checkpoint**: multi-phone trip with credits and ticker (code complete 2026-09-09)

---

## Phase 6: User Story 4 - The trip organises itself (Priority: P1)

**Goal**: bursts, keepers, tags, landmarks, scores, days, route, highlights, checked story.

**Independent Test**: 400-photo, 7-day archive becomes one trip with seven days, route map, 24 highlights and a story that names only metadata places and dates.

- [X] T049 [US4] Wire `enrich` to the `Vision` trait for labels and text and to `Vectors` for embeddings, keepers only, non-keepers inheriting, in `services/lambdas/ingest/src/enrich.rs`; test in `services/lambdas/ingest/tests/pipeline.rs` (F-202, F-706)
- [X] T050 [US4] Escalate low-confidence landmarks to `Vision::describe` with GPS hint and store `description` on the keeper in `services/lambdas/ingest/src/enrich.rs` (F-203, research R8)
- [X] T051 [P] [US4] Render per-trip and per-day static route maps through `Maps` in `services/lambdas/ingest/src/organize.rs`, storing `map_key`; inferred routes flagged (F-206, F-207)
- [X] T052 [P] [US4] Serve map images via `GET /trips/{trip}/map?day=` (presigned URL for the trip map or one day's map) in `services/lambdas/api/src/handlers/trips.rs`; show them in `packages/tv-ui/src/screens/TripView.tsx`
- [X] T053 [P] [US4] Build `packages/tv-ui/src/screens/Slideshow.tsx` (play, pause, next and previous day, current media id and paused state exposed for presence) and route it from Trip view in `packages/tv-ui/src/App.tsx`; test in `packages/tv-ui/test/screens.test.tsx` (F-403)
- [X] T054 [P] [US4] Add `POST /media/{media}/keeper` (owner or co-editor; moves the burst group's keeper, non-keepers re-inherit tags and embedding) in `services/lambdas/api/src/handlers/media.rs` and a Make keeper action in `packages/tv-ui/src/screens/Highlights.tsx`; test in `services/lambdas/api/tests/api.rs` (F-201, F-164)
- [X] T055 [US4] Enforce `narrative.check` in `save_narrative`: reject and set `Withheld` when `check_narrative` fails after one retry in `agents/src/memorylane_agents/workflows.py::trip_curator` and `services/lambdas/api/src/handlers/trips.rs`; TV shows "being written" (F-301); test in `agents/tests/test_workflows.py`
- [X] T056 [P] [US4] Add `POST /trips/{trip}/narrative/regenerate` in `services/lambdas/api/src/handlers/trips.rs` and a Regenerate action in `packages/tv-ui/src/screens/TripView.tsx` (F-302)
- [X] T057 [P] [US4] Split narrative into `TripDay.story` paragraphs in `agents/src/memorylane_agents/narrative.py` for narration sync (F-709 prerequisite)
- [X] T058 [US4] Handle the undated-photo edge case (no timestamp, no GPS: placed by upload time at the end of the nearest trip, flagged) in `services/crates/trips/src/cluster.rs`; unit test in the same file

**Checkpoint**: self-organising trip with a checked story (code complete 2026-09-09; descriptions and maps run through in-memory backends, the AWS ones are compile-checked)

---

## Phase 7: User Story 5 - Ask about the archive (Priority: P2)

**Goal**: text on the TV, voice from the phone or Alexa+, playlists and answer cards.

**Independent Test**: "the ramen place on day 1" typed on the TV plays a playlist; phone mic on a paused Nara photo returns a card naming the statue on the TV.

- [X] T059 [US5] Add `POST /search` (query, trip, filters; vector cache first, 300 ms warm budget) in `services/lambdas/api/src/handlers/search.rs`; register in `app.rs`; test in `services/lambdas/api/tests/api.rs` (F-303)
- [X] T060 [P] [US5] Add `POST /playlists`, `GET /playlists`, `GET /playlists/{id}` in `services/lambdas/api/src/handlers/playlists.rs`; the `create_playlist` Gateway target maps to it (F-304)
- [X] T061 [P] [US5] Add `PUT /presence` writing the TV's `now_showing` field (data-model Presence) in `services/lambdas/api/src/handlers/presence.rs`; post it from `packages/tv-ui/src/screens/Slideshow.tsx` on every slide change and pause (research R3; depends on T053)
- [X] T062 [US5] Add `POST /ask` in `services/lambdas/api/src/handlers/ask.rs`: resolves the target TV from the `tv` parameter, else the household's presence record with the newest `at` (edge case: two TVs); a question with no fresh now-showing goes to search, with now-showing to `describe_photo`; emits `answer.card` to that TV (F-405)
- [X] T063 [US5] Implement `get_now_showing` and `describe_photo` tools in `agents/src/memorylane_agents/tools.py`, `gateway.py`, and a `describe` workflow in `agents/src/memorylane_agents/workflows.py` that answers from the stored description and runs the place and date check on any live model answer (research R8, constitution IV); test in `agents/tests/test_workflows.py`
- [X] T064 [P] [US5] Build `packages/tv-ui/src/screens/Ask.tsx`: text input, results as playlist, `answer.card` overlay over the paused photo (mockup: Ask artboard); test in `packages/tv-ui/test/screens.test.tsx`
- [X] T065 [P] [US5] Build the phone Ask page `apps/web/src/pages/ask.ts` and `apps/web/ask.html` with Web Speech API hold-to-talk, now-showing preview, and `POST /ask`; feature-detect and fall back to text; test in `apps/web/test/ask.test.ts`
- [X] T066 [US5] Handle "nothing found" (no playlist created) and "nothing on screen" (offer search) paths in `services/lambdas/api/src/handlers/ask.rs`; tests in `services/lambdas/api/tests/api.rs`

**Checkpoint**: Ask works by text and by phone voice (code complete 2026-09-09; speech recognition runs only in a real browser, untested here)

---

## Phase 8: User Story 6 - Nothing irreversible without the remote (Priority: P2)

**Goal**: proposals with counts and samples, confirmation on the remote, cleanup batches, 30-day restore.

**Independent Test**: same 20 photos from two phones produce a dedup proposal; cancel removes nothing; confirm removes 20 and they are restorable for 30 days.

- [X] T067 [US6] Add `GET /proposals`, `POST /proposals/{id}/cancel` and proposal creation from `dedup.proposal` and `cleanup.proposal` events in `services/lambdas/api/src/handlers/proposals.rs`; bind `POST /confirmations` to an open proposal and the caller's subject; test in `services/lambdas/api/tests/api.rs`
- [X] T068 [P] [US6] Make `packages/tv-ui/src/screens/ReviewChanges.tsx` load open proposals, show before/after counts and up to four sample groups with reasons, Keep everything, Apply, Show all (mockup: Review Changes artboard); test in `packages/tv-ui/test/screens.test.tsx`
- [X] T069 [P] [US6] Flag cleanup candidates (`screenshot` by aspect and no EXIF camera, `document`/`receipt` by text density, `blurry` by sharpness threshold) in `services/lambdas/ingest/src/enrich.rs`, emitting `cleanup.candidates` on EventBridge; test in `services/lambdas/ingest/tests/pipeline.rs` (F-705)
- [X] T070 [US6] Implement `apply_cleanup` and `list_cleanup_candidates` in `services/lambdas/api/src/handlers/media.rs` and `agents/src/memorylane_agents/gateway.py`; add the `quick_cleanup` workflow in `agents/src/memorylane_agents/workflows.py` routed from `cleanup.candidates` in `router.py`; test in `agents/tests/test_workflows.py`
- [X] T071 [US6] Implement soft removal with `restore_until` (30 days), `POST /media/{media}/restore`, and a `purge_expired` Gateway tool in `services/lambdas/api/src/handlers/media.rs`; the `memories_nightly` workflow (T083) calls `purge_expired` on `nightly.tick` (contracts/events.md)
- [X] T072 [US6] Verify in `services/lambdas/api/tests/api.rs` that `apply_dedup`, `apply_cleanup`, `delete_media` without a valid token return 403 and that a token cannot be reused (F-504)

**Checkpoint**: every destructive path goes through Review Changes (code complete 2026-09-09)

---

## Phase 9: User Story 7 - Join a trip from another TV (Priority: P2)

**Goal**: a second household sees the trip live by code; per-household pins; co-editor role; guests never edit.

**Independent Test**: two Virtual Devices in two households; the second joins by code, sees the trip with a "shared by" badge, pins without affecting the first, and a reaction lands on the first's ticker.

- [X] T073 [US7] Add `POST /memberships` (refuse own trip), `DELETE /memberships/{trip}`, and `shared_by` on `GET /trips` in `services/lambdas/api/src/handlers/memberships.rs`; emit `membership.joined`; test in `services/lambdas/api/tests/api.rs` (F-162)
- [X] T074 [US7] Filter the member household's feed to `contribution.received`, `memory.ready`, `reaction.received`, `media.removed` for joined trips in `services/lambdas/api/src/handlers/events.rs` (contracts/events.md)
- [X] T075 [P] [US7] Build `packages/tv-ui/src/screens/JoinTrip.tsx` (code entry grid, trip preview, Join) and add "Join a trip" to `packages/tv-ui/src/screens/MemoriesHome.tsx` with the badge and a Leave action (mockups: Join a trip and Memories home artboards); route in `packages/tv-ui/src/App.tsx`; tests in `packages/tv-ui/test/screens.test.tsx` (F-411)
- [X] T076 [P] [US7] Make `POST /media/{media}/pin` and `/hide` write to `pinned_by`/`hidden_by` per household and filter highlights per caller in `services/lambdas/api/src/handlers/media.rs` and `services/crates/trips/src/highlights.rs` (F-163)
- [X] T077 [US7] Add `POST /trips/{trip}/roles` and `PATCH /trips/{trip}` (rename) in `services/lambdas/api/src/handlers/trips.rs`; co-editor `delete_media` via `ConfirmedAction::RemoveMedia` with subject role check, notification event to the affected contributor; test in `services/lambdas/api/tests/api.rs` (F-164)
- [X] T078 [US7] Add a Promote to co-editor action on the contributor list in `packages/tv-ui/src/screens/TripView.tsx`
- [X] T079 [US7] Reject every write from guest sessions and from the read-only web view (no edit routes accept `Caller::Contributor` guests) in `services/lambdas/api/src/app.rs`; test in `services/lambdas/api/tests/api.rs` (F-165)
- [X] T080 [US7] Scan-from-phone path: `apps/web/src/pages/link.ts` accepts `?join=<code>` while the phone is linked to a TV session and calls `POST /memberships` for that household

**Checkpoint**: second-TV join demonstrable with two Virtual Devices (code complete 2026-09-09; the two-device run waits on T035)

---

## Phase 10: User Story 8 - Memories that resurface (Priority: P3)

**Goal**: On this day, Then and now, best-of, map view, Ken Burns.

**Independent Test**: multi-year archive shows an On this day row on a date with history, a Then and now card for a repeated place, and every trip on the map.

- [X] T081 [US8] Add `services/crates/trips/src/memories.rs`: `on_this_day(date)`, `then_and_now(geohash cell, years apart)`, `best_of(period)` over media and trips; unit tests in the same file (F-701 to F-703)
- [X] T082 [P] [US8] Add `GET /memories/home` and `GET /map` in `services/lambdas/api/src/handlers/memories.rs`; `Maps::pins_map` for all trips; test in `services/lambdas/api/tests/api.rs` (F-707)
- [X] T083 [P] [US8] Add the `memories_nightly` workflow (creates On this day and best-of playlists, emits `memories.ready`) in `agents/src/memorylane_agents/workflows.py`, routed from `nightly.tick`; test in `agents/tests/test_workflows.py`
- [X] T084 [P] [US8] Render the On this day row and Then and now card on `packages/tv-ui/src/screens/MemoriesHome.tsx` and build `packages/tv-ui/src/screens/MapView.tsx` (mockups: Memories home and Map view artboards); tests in `packages/tv-ui/test/screens.test.tsx`
- [X] T085 [US8] Add Ken Burns motion (Animated scale and translate per slide, reduced when a slide is paused) to `packages/tv-ui/src/screens/Slideshow.tsx` (F-708)

**Checkpoint**: home row, card, map and motion visible (code complete 2026-09-09)

---

## Phase 11: User Story 9 - People, opt in (Priority: P3)

**Goal**: opt-in face grouping, naming from the phone, search filter, deletion with the account.

**Independent Test**: enable People, name two clusters from the phone, "Maya at the beach" returns only her photos; disable deletes the collection.

- [X] T086 [US9] Add `POST /people/enable`, `POST /people/disable` (deletes the collection and all `Person` records), `GET /people`, `PATCH /people/{id}` (name, merge_into) in `services/lambdas/api/src/handlers/people.rs`; test in `services/lambdas/api/tests/api.rs` (F-704)
- [X] T087 [US9] Index keeper faces through `Faces` at enrich when `people_enabled`, cluster by match, update `Person.media_count` and samples in `services/lambdas/ingest/src/enrich.rs`; test in `services/lambdas/ingest/tests/pipeline.rs`
- [X] T088 [P] [US9] Add a `people` filter to `POST /search` in `services/lambdas/api/src/handlers/search.rs`
- [X] T089 [P] [US9] Build the phone People page `apps/web/src/pages/people.ts` and `apps/web/people.html` (toggle with consent copy, name and merge clusters, My uploads list with remove) (mockup: People and my uploads artboard); test in `apps/web/test/people.test.ts`
- [X] T090 [P] [US9] Build `packages/tv-ui/src/screens/People.tsx` (clusters, best-of row, Turn off) (mockup: People artboard); test in `packages/tv-ui/test/screens.test.tsx`
- [X] T091 [US9] Delete the face collection and `Person` records in account deletion in `services/lambdas/api/src/handlers/session.rs` (F-604); mark the household `deleting` first so `POST /uploads/init` returns 410 while deletion runs (edge case); test in `services/lambdas/api/tests/api.rs`

**Checkpoint**: People end to end with consent and deletion (code complete 2026-09-09; Rekognition collections compile-checked only)

---

## Phase 12: User Story 10 - Reactions and comments from contributors (Priority: P3)

**Goal**: likes and comments from the read-only view reach the TV ticker and the trip view.

**Independent Test**: contributor likes day 3 and comments "That ramen!"; ticker shows "Grandma Ruth loved day 3"; comment appears beside the photo.

- [X] T092 [US10] Add `POST /trips/{trip}/reactions` (one like per actor per target, comment max 200 chars) and `GET /trips/{trip}/reactions` in `services/lambdas/api/src/handlers/reactions.rs`; emit `reaction.received`; test in `services/lambdas/api/tests/api.rs` (F-710)
- [X] T093 [P] [US10] Add like and comment controls to `apps/web/src/pages/trip.ts` (mockup: phone Trip view artboard); test in `apps/web/test/trip.test.ts`
- [X] T094 [P] [US10] Show reaction counts and the latest comment beside the day and photo in `packages/tv-ui/src/screens/TripView.tsx`; ticker line for `reaction.received` in `packages/tv-ui/src/hooks/useEvents.ts`; tests in `packages/tv-ui/test/hooks.test.tsx`

**Checkpoint**: reactions round-trip within 30 s (code complete 2026-09-09; the 30 s bound is the long-poll hold, untimed on a device)

---

## Phase 13: User Story 11 - Ambient and narrated slideshow (Priority: P3)

**Goal**: idle slideshow shaped by pins and hides; story read aloud behind a flag.

**Independent Test**: idle TV favours pinned and skips hidden; with the flag on, the day's story plays in sync or shows as captions.

- [ ] T095 [US11] Build `packages/tv-ui/src/screens/Ambient.tsx` with an idle timer in `packages/tv-ui/src/App.tsx`, drawing from the W3 ambient playlist and honouring `hidden_by` (mockup: Ambient artboard); test in `packages/tv-ui/test/screens.test.tsx` (F-406)
- [ ] T096 [P] [US11] Add a `synthesize_narration` Gateway tool in `services/lambdas/api/src/handlers/narration.rs` that runs `TripDay.story` through `Speech`, stores `audio_key` and emits `narration.ready`; W1 Trip Curator in `agents/src/memorylane_agents/workflows.py` calls it after `save_narrative` when `household.narration_enabled` (F-709); test in `agents/tests/test_workflows.py`
- [ ] T097 [US11] Add `apps/tv-vega/src/audio.kepler.ts` trying the system-deployed W3C media port for playback and a caption-only fallback in `packages/tv-ui/src/screens/Ambient.tsx`; record the Virtual Device result in `specs/decisions-2026-09-09.md` (research R10)

**Checkpoint**: ambient works; narration flag on only if the audio spike passed

---

## Phase 14: User Story 12 - Share outside the household (Priority: P3)

**Goal**: expiring links behind the confirmation gate; join code offered first when the recipient has a TV.

**Independent Test**: share "Kyoto highlights" to an email; link opens the album; after expiry it does not.

- [ ] T098 [US12] Add `POST /shares` (requires a `ShareAlbum` confirmation; 7-day expiry; optional email via SES) and public `GET /s/{share}` in `services/lambdas/api/src/handlers/shares.rs`; test in `services/lambdas/api/tests/api.rs` (F-601)
- [ ] T099 [P] [US12] Add the public album page `apps/web/src/pages/share.ts` and `apps/web/s.html` (read-only, expired state)
- [ ] T100 [US12] Update the `share` workflow in `agents/src/memorylane_agents/workflows.py` to offer a join code when the recipient household is known (W5); test in `agents/tests/test_workflows.py`
- [ ] T101 [US12] Add SES identity and permissions in `infra/lib/memorylane-stack.ts`; extend `infra/test/stack.test.ts`

**Checkpoint**: all twelve stories independently demonstrable

---

## Phase 15: Polish and cross-cutting

- [ ] T102 [P] Add the Cognito resource server (`memorylane/tools` scope), OAuth 2.1 PKCE client, and the protected resource metadata document at `/.well-known/oauth-authorization-server` in `infra/lib/memorylane-stack.ts` and `services/lambdas/api/src/handlers/wellknown.rs` (research R6)
- [ ] T103 [P] Register the Alexa+ MCP add-on with example phrases from contracts/tools.md; document steps in `docs/alexa-plus.md`
- [ ] T104 [P] Ensure every agent run and Gateway tool call is traced in AgentCore Observability with household and workflow tags in `agents/src/memorylane_agents/runtime.py` (F-506)
- [ ] T105 [P] Audit log entries for auth, share, delete and tool calls in `services/lambdas/api/src/app.rs` (F-605); test in `services/lambdas/api/tests/api.rs`
- [ ] T106 [P] Add `docs/diagrams/src/08-planned-2026-09-09.mjs` showing the member TV, phone reactions and voice, the Alexa+ context path and the widened token gate; update `docs/diagrams/README.md`
- [ ] T107 Run `make lint` and `make test`, then every scenario in `specs/001-memorylane-baseline/quickstart.md` on the Virtual Device and the deployed stack, timing the US4 scenario from last upload to `memory.ready` against the 15-minute target (SC-003); record outcomes in `specs/decisions-2026-09-09.md`
- [ ] T108 [P] Update `README.md`, `docs/devpost-overview.md` and the friction log with what shipped and what stayed unverified

---

## Dependencies & Execution Order

### Phase Dependencies

- **Setup (Phase 1)**: starts immediately; T001 → T002 → T004; T003, T005, T006 parallel; T007 after T004.
- **Foundational (Phase 2)**: after Setup. T008 to T014 parallel; T015 to T017 after T008 to T014; T018 after T016; T019 to T023 parallel; T024 after T009 and T018; T025 to T027 parallel after T015; T028 then T029 last. Blocks all stories.
- **User Stories (Phases 3 to 14)**: all depend on Phase 2. Priority order P1 (US1 to US4), P2 (US5 to US7), P3 (US8 to US12).
- **Polish (Phase 15)**: after the stories the team decides to ship.

### User Story Dependencies

- **US1**: none beyond Phase 2.
- **US2**: none; T039 needs the layer from research R5.
- **US3**: T046 touches `handlers/media.rs` shared with US6 and US7; sequence those three files by story order.
- **US4**: T049 and T050 need T019 to T022 (traits).
- **US5**: T061 depends on T053 (Slideshow); T062 depends on T059, T060, T061; T063 depends on T050 (descriptions) for full answers, works from labels before.
- **US6**: T067 before T068 and T070; T069 needs T049; T071 needs T083 for the nightly purge call.
- **US7**: T073 before T075 and T080; T076 before T084 (per-household highlights on Home).
- **US8**: T081 before T082 to T084; T083 needs T027 (nightly rule).
- **US9**: T086 before T087 to T091; T088 needs T059.
- **US10**: T092 before T093, T094; T093 needs T048.
- **US11**: T095 needs T083 (ambient playlist) and T053; T096 needs T057.
- **US12**: T098 before T099 to T101.

### Parallel Opportunities

- Phase 2 traits T019 to T023 are five independent files.
- Within every story the TV screen, the phone page and the API handler are separate files and run in parallel once the story's first API task exists.
- Stories US8, US9, US10 and US12 touch disjoint handler files and can run concurrently after Phase 2.

---

## Parallel Example: User Story 5

```bash
# After T059 (search) lands:
Task: "Add POST /playlists in services/lambdas/api/src/handlers/playlists.rs"
Task: "Add PUT /presence in services/lambdas/api/src/handlers/presence.rs"
Task: "Build packages/tv-ui/src/screens/Ask.tsx"
Task: "Build the phone Ask page apps/web/src/pages/ask.ts"
```

---

## Implementation Strategy

### MVP First (US1 to US4, the hackathon core)

1. Phase 1 Setup: the TV builds on the Virtual Device (week 1 spike).
2. Phase 2 Foundational: types, traits, fakes, roles, Cedar.
3. US1 then US2, US3, US4: sign in, upload, shared trip, self-organising trip. This is the demo's first 90 seconds.
4. Stop and validate with quickstart.md scenarios US1 to US4.

### Incremental Delivery

1. US5 Ask and US6 Review Changes: the AI-enhanced viewing and safety beats.
2. US7 Join from another TV: the second headline and the family beat.
3. US8 to US12 as time allows in priority order; US11 narration and US12 share are the first to drop.
4. Phase 15 Alexa+ OAuth and observability before submission; diagrams and docs last.

### Parallel Team Strategy

One developer: follow the order above. Two: one on Rust API and ingest, one on TV and phone UI from the mockups, meeting at each story's checkpoint. Agents work slots in beside the API on the same story.

---

## Notes

- [P] tasks = different files, no dependencies
- [Story] label maps task to specific user story for traceability
- Each user story should be independently completable and testable
- Commit after each task or logical group with conventional messages
- Stop at any checkpoint to validate story independently
- Constitution gates on every task: no bundled native module (II), token for anything irreversible (III), model text checked (IV), keepers only to models (V), `make test` offline (VI)
