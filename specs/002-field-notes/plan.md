# Implementation Plan: Field Notes

**Branch**: `002-field-notes` | **Date**: 2026-10-09 | **Spec**: [spec.md](spec.md)

**Input**: Feature specification from `/specs/002-field-notes/spec.md`

## Summary

Add F-711: for each trip highlight, a quiet map of the real geography where it was taken,
drawn as a separate transparent layer into the calmest part of the photo that is clear of
its subjects. The map is drawn in Rust from OpenStreetMap vector tiles and open terrain
tiles (Amazon Location's terms bar storing map content, research R1), as a resolution-free
display list rasterised per audience: owner form with a dot and rounded coordinates for real
GPS, public form with a ring only. A new `field_note` ingest task runs after organize over a
manifest of highlights whose note is missing or stale. Subjects come from Rekognition boxes
already returned at enrich plus one Nova Lite call per highlight for scenery. The TV gains a
Field notes mode in the slideshow (whole photo, no zoom, layer aligned) and a mark in Trip
view; the phone gains view and full-resolution share.

## Technical Context

**Language/Version**: Rust 1.93 (ingest, API, new `crates/fieldnotes`); TypeScript 5.9 (TV in `packages/tv-ui`, phone in `apps/web`, CDK).

**Primary Dependencies**: new pure-Rust crates `mvt-reader` (tile decode), `tiny-skia` (raster), `ttf-parser` (glyph outlines), `contour` (marching squares); existing `image`, aws-sdk (S3, Rekognition, Bedrock Runtime), `ureq` for tile HTTP (already a store dependency). Bundled font: Figtree (OFL).

**Storage**: `MediaItem.field_note` and `Enrichment.subjects` in the existing DynamoDB item; `Household.home`; display list and rasters in S3 under `renditions/{h}/{m}/` (data-model.md).

**Testing**: cargo test with in-memory `Geography` and `Vision` fakes and golden placement fixtures (synthetic images with known calm regions and boxes); jest for the TV mode and toggle; vitest for the phone viewer. No network in `make test` (constitution VI).

**Target Platform**: Lambda arm64 (ingest, API); Vega Virtual Device and Fire TV; phone browsers.

**Project Type**: serverless backend + TV app + web app (existing monorepo).

**Performance Goals**: one field note made in under 10 s (tile fetches dominate); a 24-highlight trip ready within 10 minutes of organize (SC-005); TV toggle under 1 s (SC-006, the layer is one cached PNG per slide).

**Constraints**: never modify originals or existing renditions (FR-002); no image-generation model (FR-008); at most one model call per highlight; tiles fetched from a third party at make time only, never at view time; ingest Lambda 2048 MB / 5 min.

**Scale/Scope**: about 24 highlights per trip, 4 to 9 vector tiles and up to 4 terrain tiles per note; two new TV states (mode on/off), one phone viewer.

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

| Principle | Gate | Pre-research | Post-design |
|-----------|------|--------------|-------------|
| I. Zero infrastructure for the user | Nothing new for the household to run or install | PASS: made in the hosted pipeline, shown in the existing apps | PASS |
| II. Vega-first TV app | No bundled native module | PASS: two stacked `Image`s and a toggle, React Native core only | PASS |
| III. Confirm before irreversible | No new irreversible action | PASS: field notes are derived and regenerable; the only delete path is the photo's own (already confirmed) | PASS |
| IV. Narrative from structured data only | Text shown comes from metadata and is checked | PASS: caption is the place name from map data or trip landmarks, the day's date and GPS-derived coordinates; no model text is shown. The subject call's output is used only as boxes, never displayed | PASS |
| V. Deterministic before expensive | Model calls only on keepers, after deterministic filters | PASS with note: one Nova Lite call per highlight (highlights are keepers), skipped when place is unknown or the note is up to date; placement, mode and drawing are deterministic | PASS |
| VI. Tests run without AWS | New backends behind traits with in-memory fakes | PASS: `Geography` (tiles and terrain) and `Vision::subjects` get memory implementations; HTTP and Bedrock compile-checked | PASS |

No violations.

## Project Structure

### Documentation (this feature)

```text
specs/002-field-notes/
├── plan.md              # This file
├── research.md          # Phase 0
├── data-model.md        # Phase 1
├── quickstart.md        # Phase 1
├── contracts/api.md     # Phase 1
├── checklists/requirements.md
└── tasks.md             # Phase 2 (/speckit-tasks)
```

### Source Code (repository root)

```text
services/
  crates/core/src/media.rs          # Subject, NormBox, FieldNote, Certainty; Enrichment.subjects
  crates/core/src/household.rs      # Household.home
  crates/fieldnotes/                # NEW pure library, no AWS
    src/lib.rs                      #   make(): place → geography → placement → drawing
    src/certainty.rs                #   GPS / estimated route / landmark / home → Certainty
    src/placement.rs                #   detail grid, subject mask, candidate search, ink
    src/geo.rs                      #   Web Mercator, tile cover for a view, projection into the box
    src/mode.rs                     #   coast / mountains / town / trail from features
    src/drawing.rs                  #   display list: strokes, labels, marker, caption; public form
    src/raster.rs                   #   tiny-skia rasteriser: fade, grain, halo text, PNG
    src/contours.rs                 #   terrarium decode + marching squares
    assets/Figtree-Medium.ttf       #   OFL font
    tests/                          #   golden placement and drawing tests
  crates/store/src/geography.rs     # NEW trait Geography { vector_tile, terrain_tile } + Memory, OpenFreeMap/TerrainTiles impls
  crates/store/src/vision.rs        # analyze/detect_text keep boxes; new subjects() via Nova
  lambdas/ingest/src/organize.rs    # fingerprints, manifest, drop stale notes, Household.home
  lambdas/ingest/src/fieldnote.rs   # NEW task: subjects call, make, store list + rasters
  lambdas/api/src/handlers/media.rs # fieldnote rendition, POST /media/{id}/fieldnote/full, purge list
  lambdas/api/src/handlers/trips.rs # field_note on highlights (public form for members/code view)
infra/lib/mantelpiece-stack.ts      # FieldNotes Distributed Map after OrganizeFinal
packages/tv-ui/src/
  screens/Slideshow.tsx             # Field notes mode: contain, no Ken Burns, layer, toggle, fading chrome
  screens/TripView.tsx              # mark on highlights, full-screen field note
  components/FieldNoteImage.tsx     # NEW photo + layer in one aligned box
  session.ts                        # StoredSession.fieldNotes
apps/web/src/pages/trips.ts         # highlight viewer with toggle and share
```

**Structure Decision**: the drawing logic lives in a new pure crate, `crates/fieldnotes`, so it is tested without AWS and reused by the ingest task (make) and the API (full-resolution share), the way `crates/trips` serves organize. Network access stays in `crates/store` behind the new `Geography` trait.

## Delivery order

1. **Spike (tasks first):** fetch one OpenFreeMap tile and one terrain tile for a known
   place, decode, draw, and composite onto a real photo locally through `ingest-local`, to
   judge the look and the placement threshold before wiring AWS. This is the "evening
   prototype" checkpoint the user asked for.
2. US3 placement and US2 certainty (pure, tested), then US1 pipeline and TV mode, then US4
   phone share, then US5 keeping up.

## Complexity Tracking

None.
