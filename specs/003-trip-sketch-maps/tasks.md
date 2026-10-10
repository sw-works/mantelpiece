# Tasks: Trip Sketch Maps

**Input**: Design documents from `/specs/003-trip-sketch-maps/`
**Prerequisites**: plan.md, spec.md, research.md, data-model.md, contracts/api.md

## Phase 1: Setup

- [ ] T001 Bundle `services/crates/fieldnotes/assets/PatrickHand-Regular.ttf` with `OFL-PatrickHand.txt`, loaded with `include_bytes!` in `services/crates/fieldnotes/src/hand.rs`
- [ ] T002 [P] Add a zoom-6 Iceland vector fixture and a zoom-7 terrain fixture under `services/crates/fieldnotes/tests/fixtures/` with their source URLs in the fixtures README

## Phase 2: Foundational

- [ ] T003 `SketchKeys` in `services/crates/core/src/sketch.rs`: `trip_png`, `trip_public_png`, `trip_marker`, `day_png(n)`, `day_public_png(n)`, `day_marker(n)` under `sketch/{h}/{trip}/`, and `all(trip, days)` for deletion; tests
- [ ] T004 Hand-drawn primitives in `services/crates/fieldnotes/src/hand.rs`: seeded 1D value noise, `wobble(polyline, seed, amplitude)` resampled at 4 px, `ink(pixmap, path, width, colour, seed)` with a second 0.5 px pass at 35 percent, hand dashes, `wash(pixmap, polygon, colour, seed)` (three low-alpha fills plus a 15 percent edge), `paper(pixmap, seed)`, `text(pixmap, str, x, y, size, colour)` in Patrick Hand; unit tests for determinism

## Phase 3: User Story 1 — Trip map (P1)

**Goal**: one hand-drawn map of the whole trip. **Independent test**: `tests/sketch.rs` draws the Iceland fixture trip twice, byte-identical, with legs, day numbers, pictograms and title.

- [ ] T005 [US1] Frame and legs in `services/crates/fieldnotes/src/sketch.rs`: legs per day from `day.route.points`; gaps over "300 km" become `hop` legs that do not set the frame unless alone; frame = extent of the rest plus 12 percent margin fitted to 16:9; zoom from `geo::zoom_for` at 1600 px
- [ ] T006 [US1] Geography layers in `sketch.rs`: decode `water` (ocean edge as coast, lakes), `landcover` ice (glaciers), `waterway` river, `transportation` motorway, trunk and primary, `mountain_peak`, `place` city and town, via the field-notes decoder; drawn as washes and wobbly ink in that order
- [ ] T007 [US1] Pictograms in `services/crates/fieldnotes/src/pictogram.rs`: kind from the landmark's kind and name (foss, falls, waterfall; viti, lighthouse; jökull, glacier; kirkja, church, cathedral; peak, fjall, mount, kind mountain; hver, geysir, hot spring, laug; castle; beach; vatn, lake, kind waterbody; kind city; else star); small sketched paths for each
- [ ] T008 [US1] Labels and marks in `sketch.rs`: "at most twelve" landmarks by prominence and spread, label beside the pictogram in the first of four free positions; circled day numbers at each leg's end; title cartouche (trip name, dates) in the emptiest corner; compass; hand scale bar; credit line
- [ ] T009 [US1] Hatching in `sketch.rs`: terrain at vector zoom plus one, contours at the field-notes interval, hachures across contour lines pointing downhill where relief exceeds 200 m
- [ ] T010 [US1] `sketch::draw(trip, which, home, tiles) -> Result<Sketch{owner_png, public_png, fingerprint, credit}, Skip>` and `fingerprint(...)` per data-model.md, in `sketch.rs`; tests in `services/crates/fieldnotes/tests/sketch.rs`
- [ ] T011 [US1] Ingest task `sketch_map` in `services/lambdas/ingest/src/sketch.rs` and `Task::SketchMap` in `lib.rs`: load trip and home, compare marker, draw on a blocking thread, put both PNGs and the marker; delete a day's objects when the day no longer exists; test in `services/lambdas/ingest/tests/pipeline.rs`
- [ ] T012 [US1] Organize in `services/lambdas/ingest/src/organize.rs`: write `manifests/{h}/sketch-maps-{run}.json` (one item per trip and per day) and return `sketch_maps_manifest`; remove the `Maps::route_map` calls (`render_maps`)
- [ ] T013 [US1] Infra in `infra/lib/mantelpiece-stack.ts`: `SketchMaps` Distributed Map (concurrency 4, retry 2) after `FieldNotes`, and `sketch_maps_manifest` in the organize result selector
- [ ] T014 [US1] API `GET /trips/{id}/map` in `services/lambdas/api/src/handlers/trips.rs`: owner form for the owner household, public form for members and code views, 404 without the marker, `credit` in the response; tests in `services/lambdas/api/tests/api.rs`
- [ ] T015 [US1] TV Trip view in `packages/tv-ui/src/screens/TripView.tsx` and `components/MapImage.tsx`: trip map at the top of the trip, `contain`, credit under it

## Phase 4: User Story 2 — Day maps (P1)

- [ ] T016 [US2] Day map in `sketch.rs`: frame of the day's route and photo spots ("at least 2 km across"), dots for GPS photo spots, the day's landmarks named, no title box (the day number and date in a small corner note)
- [ ] T017 [US2] TV: day section shows its day map in `TripView.tsx`; slideshow opens each new day with the day map for "about four seconds" (held while paused) in `packages/tv-ui/src/screens/Slideshow.tsx`; tests in `packages/tv-ui/test/`

## Phase 5: User Story 3 — Honesty (P1)

- [ ] T018 [US3] In `sketch.rs`: estimated legs fainter (45 percent ink) with sparser dashes, no dots for estimated points; home cut per research R7 for both forms; tests: public form has no point within 2 km of home

## Phase 6: User Story 4 — Phone (P2)

- [ ] T019 [US4] Phone trips page in `apps/web/src/pages/trips.ts` and read-only `apps/web/src/pages/trip.ts`: trip map with credit at the top; Share (owner, co-editors) through the existing `shareFile`; tests in `apps/web/test/`

## Phase 7: User Story 5 — Follows the trip (P2)

- [ ] T020 [US5] Purge: trip removal and account deletion delete `SketchKeys::all` in `services/lambdas/api/src/handlers/session.rs` and the trip removal path; test

## Phase 8: Polish

- [ ] T021 [P] Example `services/crates/fieldnotes/examples/sketch.rs` per quickstart; run it on the Iceland album and tune
- [ ] T022 [P] F-712 in `specs/architecture.md`, F-207 row updated; API contract rows in `specs/001-mantelpiece-baseline/contracts/api.md`; README credits for Patrick Hand
- [ ] T023 `make lint`, `make test`, deployed run per quickstart; record in `specs/decisions-2026-09-09.md`

## Dependencies

Setup, then Foundational (T003, T004), then US1 (T005 to T015) which US2 and US3 extend in
`sketch.rs`; US4 needs T014; US5 needs T003. MVP: US1 plus US3 (honesty is not optional).
