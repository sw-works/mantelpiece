# Tasks: Field Notes

**Input**: Design documents from `/specs/002-field-notes/`

**Prerequisites**: plan.md, spec.md, research.md, data-model.md, contracts/api.md, quickstart.md

**Tests**: every task that adds logic names its test, as in spec 001; `make test` stays offline (constitution VI).

## Format: `[ID] [P?] [Story] Description`

- **[P]**: can run in parallel (different files, no dependency on an unfinished task)
- **[Story]**: US1 to US5 from spec.md

---

## Phase 1: Setup

- [ ] T001 Create the pure crate `services/crates/fieldnotes` (lib, `[lints] workspace = true`) with dependencies `mvt-reader`, `tiny-skia` (`png-format`), `ttf-parser`, `contour`, `image`, `serde`, `mantelpiece-core`; add it to `services/Cargo.toml` workspace dependencies
- [ ] T002 [P] Bundle the OFL font `services/crates/fieldnotes/assets/Figtree-Medium.ttf` with its `OFL.txt` beside it, loaded with `include_bytes!` in `services/crates/fieldnotes/src/raster.rs`
- [ ] T003 Confirm `cargo lambda build --release --arm64 --bin ingest` still builds with the new crate linked (zig toolchain, README "Build and test"); record the binary size in `specs/decisions-2026-09-09.md`

---

## Phase 2: Foundational

- [ ] T004 Add `NormBox { left, top, width, height }` (fractions 0 to 1, clamped to the frame), `Subject { kind: person|face|text|object|landscape, label, bbox: NormBox, confidence }` and `Enrichment.subjects: Vec<Subject>` (`#[serde(default)]`) in `services/crates/core/src/media.rs`; drop boxes "under 0.5 percent of the frame" except `text`; unit tests in the same file
- [ ] T005 Add `FieldNote { placement: NormBox, mode: coast|mountains|town|trail, certainty: Certainty, ink: light|dark, caption: Caption { place, date, coords: Option<String> }, source, credit, drawing_key, fingerprint, made_at }`, `Certainty::{Precise { at }, Approximate { at, radius_m }}` and `MediaItem.field_note: Option<FieldNote>` (`#[serde(default)]`) in `services/crates/core/src/media.rs`; old rows deserialise (test)
- [ ] T006 [P] Add `Household.home: Option<GeoPoint>` (`#[serde(default, skip_serializing_if = "Option::is_none")]`) in `services/crates/core/src/household.rs`
- [ ] T007 Add trait `Geography { vector_tile(z, x, y) -> Option<Vec<u8>>, terrain_tile(z, x, y) -> Option<Vec<u8>> }` with `MemoryGeography` (fixtures by key) and `OpenGeography` (OpenFreeMap tiles via its TileJSON URL template, Terrain Tiles `https://s3.amazonaws.com/elevation-tiles-prod/terrarium/{z}/{x}/{y}.png`, `ureq`, 5 s timeout, a `User-Agent` naming Mantelpiece) in `services/crates/store/src/geography.rs`; `None` on 404 and on network failure is an error
- [ ] T008 Keep the boxes Rekognition already returns: label `Instances[].BoundingBox` as `person`/`object`, `FaceDetails[].BoundingBox` as `face`, `TextDetections` of type `LINE` as `text`, into `Enrichment.subjects` in `services/crates/store/src/vision.rs` (`AwsVision::analyze`, `detect_text` gains boxes through a new `detect_text_boxes` or a richer return); `MemoryVision` returns configured subjects
- [ ] T009 Add `Vision::subjects(jpeg) -> Vec<Subject>`: Nova Lite through Converse, temperature 0, prompt asking for the photo's main subjects as JSON `[{"label","box":[x1,y1,x2,y2]}]` on the 0 to 1000 grid, parsed leniently (malformed → empty) into `landscape` subjects; `MemoryVision.subjects` configurable, in `services/crates/store/src/vision.rs`; parser tests
- [ ] T010 Write enrich's new `subjects` through to keepers and their burst in `services/lambdas/ingest/src/enrich.rs` and `services/lambdas/ingest/src/organize.rs` (inheritance copies `subjects`); pipeline test in `services/lambdas/ingest/tests/`

**Checkpoint**: types, Geography and subject boxes exist; nothing is drawn yet.

---

## Phase 3: User Story 3 - The map never covers what the photo is about (Priority: P1)

**Goal**: given a photo and its subjects, choose where a map may go, or decline.

**Independent Test**: golden tests over synthetic images with known calm regions and boxes; `ingest-local --field-notes` writes a mask image per photo.

- [ ] T011 [P] [US3] Detail grid in `services/crates/fieldnotes/src/placement.rs`: scale the upright TV rendition to 96 cells wide, per cell mean Sobel magnitude, mean luminance, sky-likeness (upper half, low detail, bright or blue-dominant) and water-likeness (low detail, gradients mostly horizontal)
- [ ] T012 [US3] Subject mask and search in `services/crates/fieldnotes/src/placement.rs`: pad every subject by "4 percent of the shorter side", keep a "3 percent frame margin", slide rectangles of 15, 20 and 25 percent of the frame at aspects 4:3, 3:2, 1:1 (and 3:4 for portrait) over an integral image; score = mean detail − sky/water bonus + centre-third penalty; return `Placement { box, ink }` or `NoCalmRegion` when the best mean detail is over the threshold; ink light when mean luminance < 0.45
- [ ] T013 [P] [US3] Golden tests in `services/crates/fieldnotes/tests/placement.rs`: open sky above people (map in sky, no overlap with any box), sea behind a person, busy texture (declined), portrait photo (portrait box), 15 to 25 percent area always, aspect never changed
- [ ] T014 [US3] Debug mask writer (subjects outlined, detail heat, chosen box) in `services/crates/fieldnotes/src/raster.rs` behind a `debug_mask()` function used only by tools

**Checkpoint**: placement is decided and tested without network or AWS.

---

## Phase 4: User Story 2 - The map tells the truth about where the photo was taken (Priority: P1)

**Goal**: a certainty for every highlight, or none, and the forms each audience sees.

**Independent Test**: unit tests over a trip fixture mixing GPS, estimated and unplaced photos and a home point.

- [ ] T015 [P] [US2] `certainty()` in `services/crates/fieldnotes/src/certainty.rs`: `Precise` from the photo's own GPS; `Approximate` from an estimated `RoutePoint` (radius "half the distance to the nearer geotagged neighbour, clamped to 300 m to 5 km") or from a `Landmark` whose `media` lists the photo (radius `Landmark::reach_m()`); downgrade `Precise` within 2 km of `Household.home` to `Approximate { radius_m: 2000 }`; `None` otherwise; tests
- [ ] T016 [P] [US2] Caption in `services/crates/fieldnotes/src/certainty.rs`: place name (nearest trip landmark within reach, else the map's nearest place label), the day's date, and coordinates "rounded to two decimal places" only for `Precise` (`35.01° N, 135.77° E`); tests
- [ ] T017 [US2] Public form in `services/crates/fieldnotes/src/drawing.rs`: `Drawing::public()` forces `marker.kind = ring` with radius "at least 500 m" and drops coordinates; test that no public drawing ever contains a dot or a coordinate string
- [ ] T018 [US2] Home area in `services/lambdas/ingest/src/organize.rs`: busiest 0.02° cell of geotagged photos with `trip == None`, "at least 20 photos", stored in `Household.home`; test in `services/lambdas/ingest/tests/`

**Checkpoint**: dot, ring or nothing is decided correctly for every case in the spec.

---

## Phase 5: User Story 1 - Watch a trip as field notes on the TV (Priority: P1) 🎯 MVP

**Goal**: notes are made after organize and shown in the TV slideshow with a one-button toggle.

**Independent Test**: on the Virtual Device, organise a GPS trip, open its slideshow, press Field notes; maps appear on highlights with quiet space; off shows the unchanged photos.

- [ ] T019 [P] [US1] Web Mercator helpers in `services/crates/fieldnotes/src/geo.rs`: view extent from certainty and mode (town about 1.5 km across, coast and trail about 8 km, mountains about 12 km, at least 3× an approximate ring), the tiles covering it at a zoom giving roughly 256 source px across the box, projection of tile coordinates into box fractions; tests
- [ ] T020 [P] [US1] Contours in `services/crates/fieldnotes/src/contours.rs`: decode Terrarium PNG (`elevation = R*256 + G + B/256 - 32768`), stitch tiles, marching squares via `contour`, interval 10/20/50/100 m from relief, every fifth line `contour_index`; test on a synthetic cone
- [ ] T021 [US1] Features in `services/crates/fieldnotes/src/drawing.rs`: decode OpenMapTiles layers `water` (ocean polygon edges as `coast`, lakes as `water`), `waterway`, `transportation` (`class` motorway/trunk/primary → `road_major`, minor/service → `road_minor`, path/track → `path`), `place` and `mountain_peak` (English name from `name:en`, else `name_en`, else omit), clip to the view, simplify to the box size
- [ ] T022 [US1] Mode in `services/crates/fieldnotes/src/mode.rs`: `coast` when coastline length in view is significant, else `mountains` when relief > 300 m, else `town` when road density is high, else `trail`; the day's route drawn as `route` in every mode when the trip has one; tests with feature fixtures
- [ ] T023 [US1] Labels and marker in `services/crates/fieldnotes/src/drawing.rs`: "at most four English place names" by rank and distance without overlap; `dot` for `Precise`, `ring` for `Approximate`; edge `fade`; `grain_seed` from the media id; serialise the display list as JSON (`version`, `strokes`, `labels`, `marker`, `fade`, `grain_seed`)
- [ ] T024 [US1] Rasteriser in `services/crates/fieldnotes/src/raster.rs`: `tiny-skia` transparent canvas at the target frame size, strokes thin and low-saturation per class in light or dark ink, labels as `ttf-parser` outlines with a halo, red dot `#C8402F`, ring, feathered edge mask, paper grain applied to the layer's alpha only, PNG out; `compose(photo, layer)` for sharing; tests: output same size as input, pixels outside `placement` fully transparent, `compose` with an empty layer equals the photo (SC-001)
- [ ] T025 [US1] `make()` in `services/crates/fieldnotes/src/lib.rs`: certainty → placement → geography → drawing; returns `Made { drawing, field_note }` or a `Skip` reason (`no place`, `no calm region`, `map data unavailable`); fingerprint = hash of photo sha256, certainty, the day's route and the style version; pipeline test with `MemoryGeography` fixtures in `services/crates/fieldnotes/tests/make.rs`
- [ ] T026 [US1] Ingest task `field_note` in `services/lambdas/ingest/src/fieldnote.rs` and the `Task` enum in `services/lambdas/ingest/src/lib.rs`: load media, trip and household; skip non-highlights and up-to-date notes; one `Vision::subjects` call; `make()`; write `fieldnote.json`, `fieldnote.png` and `fieldnote-public.png` under `renditions/{h}/{m}/`; set `MediaItem.field_note`; output `{media_id, made, skipped}`; test with memory stores
- [ ] T027 [US1] Organize manifest in `services/lambdas/ingest/src/organize.rs`: write `manifests/{h}/field-notes-{run}.json` of highlights with a missing or stale fingerprint, add `field_notes_manifest` to `Output`; emit `field_notes.ready {trip, count}` from the field-note task when a trip's last pending note completes (or from organize's next run); event type in `services/crates/core/src/events.rs`
- [ ] T028 [US1] Wire `OpenGeography` into `services/lambdas/ingest/src/main.rs` (`FIELD_NOTES=off` turns making off) and add the `FieldNotes` Distributed Map over `field_notes_manifest` after `OrganizeFinal`, "at most four in flight per household", in `infra/lib/mantelpiece-stack.ts`; stack test in `infra/test/stack.test.ts`
- [ ] T029 [US1] API: `rendition=fieldnote` in `GET /media/{id}/url` choosing `fieldnote.png` for the owner household and `fieldnote-public.png` for members and code-gated views; `field_note` on highlights in `GET /trips/{id}` with `certainty` forced to `approximate` for members, in `services/lambdas/api/src/handlers/media.rs` and `services/lambdas/api/src/handlers/trips.rs`; tests in `services/lambdas/api/tests/api.rs`
- [ ] T030 [P] [US1] TV client and types: `mediaUrl(..., "fieldnote")`, `field_note` on trip media, `field_notes.ready` in the feed reducer, in `packages/tv-ui/src/api/client.ts` and `packages/tv-ui/src/hooks/useEvents.ts`
- [ ] T031 [P] [US1] `FieldNoteImage` in `packages/tv-ui/src/components/FieldNoteImage.tsx`: photo and layer as two `resizeMode="contain"` images in one box so they align; falls back to the plain photo when the layer fails
- [ ] T032 [US1] Slideshow Field notes mode in `packages/tv-ui/src/screens/Slideshow.tsx`: a "Field notes" button toggles; in the mode, highlights with a note use `FieldNoteImage` with no Ken Burns and no bottom shade, others show plainly with `contain`; controls fade after three seconds without a key; "No field notes for this trip yet" in one line when none exist; jest tests in `packages/tv-ui/test/screens.test.tsx`
- [ ] T033 [US1] Remember the toggle per TV: `StoredSession.fieldNotes` in `packages/tv-ui/src/session.ts`, read and written by `packages/tv-ui/src/App.tsx`; test in `packages/tv-ui/test/session.test.ts`
- [ ] T034 [US1] Trip view mark and full-screen field note on focused highlights in `packages/tv-ui/src/screens/TripView.tsx`; test in `packages/tv-ui/test/screens.test.tsx`

- [ ] T046 [US1] Reference look (spec FR-005, FR-005a, FR-007): translucent water and park fills, peak elevation labels, generic pictograms by kind (peak, hut, church, castle, bridge, station), and a title block (place and country in spaced capitals, "no larger than 4 percent of the frame's height") with scale bar and north mark in a calm corner, in `services/crates/fieldnotes/src/drawing.rs` and `services/crates/fieldnotes/src/raster.rs`
- [ ] T047 [US1] Shaded relief under contours in mountain mode from the same terrain tiles (`services/crates/fieldnotes/src/contours.rs`), faint enough that the photo shows through
- [ ] T048 [US1] View cone (FR-005b): parse EXIF `GPSImgDirection` into `MediaItem.heading: Option<f32>` in `services/crates/ingest/src/exif.rs` and `services/lambdas/ingest/src/signal.rs`; draw a narrow cone from the dot when present, never for approximate places

**Checkpoint**: the MVP. A trip's highlights become field notes on the TV.

---

## Phase 6: User Story 4 - Share a field note from the phone (Priority: P2)

**Goal**: owner views a note on the phone and shares it at full resolution.

**Independent Test**: on the phone trips page, open a highlight with a note, share it, receive one JPEG at the original's resolution.

- [ ] T035 [US4] `POST /media/{id}/fieldnote/full` in `services/lambdas/api/src/handlers/media.rs`: owner or co-editor only; decode the original, rasterise the stored drawing at its size, `compose`, write `fieldnote-full.jpg` (reused while the fingerprint is unchanged), return `{url, expires_in}`; 404 without a note, 403 for others; route in `services/lambdas/api/src/app.rs`; tests in `services/lambdas/api/tests/api.rs`
- [ ] T036 [US4] Purge and account deletion remove `fieldnote.json`, `fieldnote.png`, `fieldnote-public.png`, `fieldnote-full.jpg` in `services/lambdas/api/src/handlers/media.rs` and `services/lambdas/api/src/handlers/session.rs`; tests
- [ ] T037 [P] [US4] Phone viewer in `apps/web/src/pages/trips.ts`: highlights with a note open a full-width view with a Plain / Field note switch and Share (Web Share API with the file, else a download link), credit line under it; vitest for the pure parts in `apps/web/test/trips.test.ts`
- [ ] T038 [P] [US4] Read-only view and shared links show the public form with the credit, in `apps/web/src/pages/trip.ts` and `apps/web/src/pages/shared.ts`

---

## Phase 7: User Story 5 - Field notes keep up with the trip (Priority: P3)

**Goal**: notes follow highlight changes, removals and corrected places.

**Independent Test**: re-organise after adding photos; new highlights get notes, dropped ones lose them.

- [ ] T039 [US5] In `services/lambdas/ingest/src/organize.rs`: clear `field_note` and delete its objects for photos that stopped being highlights; stale fingerprints (place or route changed) go back on the manifest; tests in `services/lambdas/ingest/tests/`
- [ ] T040 [US5] Soft removal hides and restore brings back the note without remaking it (`GET /media/{id}/url?rendition=fieldnote` 404 while removed) in `services/lambdas/api/src/handlers/media.rs`; test

---

## Phase 8: Polish & Cross-Cutting

- [ ] T041 [P] `ingest-local --field-notes` in `services/tools/ingest-local/src/main.rs`: run placement and make over a folder with `OpenGeography` (network, no AWS), write `field-notes/<media>.png` (composited), `<media>-mask.png` and a report line per photo; README section
- [ ] T042 Spike on real photos with T041 (quickstart "Without AWS"): tune the calm threshold and line weights; record the threshold, examples and timings in `specs/decisions-2026-09-09.md`
- [ ] T043 [P] Add F-711 to `specs/architecture.md` section 4.8 and log the Amazon Location storage finding for the existing F-207 route maps (research R1) as an open question in `specs/decisions-2026-09-09.md`
- [ ] T044 [P] Credit "© OpenStreetMap contributors" and the terrain sources on every surface that shows a field note, and in the README
- [ ] T045 Run `make lint`, `make test`, and the quickstart scenarios on the deployed stack and the Virtual Device; record outcomes in `specs/decisions-2026-09-09.md`

---

## Dependencies & Execution Order

- Setup (T001 to T003) → Foundational (T004 to T010) → stories.
- US3 (placement) and US2 (certainty) are pure and independent of each other; both feed US1's `make()` (T025).
- US1 needs US2 and US3. US4 needs US1's stored drawing. US5 needs US1's pipeline.
- T041 and T042 (the spike) can start as soon as T025 exists and should run before the TV work (T030 to T034) to settle the look.

### Parallel opportunities

- T002, T006 alongside T001/T004.
- T011 and T015/T016 (different files, both pure).
- T019 and T020 alongside T021.
- T030, T031 (TV) alongside T026 to T029 (backend).
- T037, T038 (phone) alongside T035.

## Implementation Strategy

1. Foundational, then US3 and US2 (pure, fully tested offline).
2. `make()` and the spike (T019 to T025, T041, T042): look at real photos before building surfaces.
3. Pipeline and TV (T026 to T034): the MVP.
4. Phone share (US4), then keeping up (US5), then polish and the deployed run.
