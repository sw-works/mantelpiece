# Implementation Plan: Trip Sketch Maps

**Branch**: `003-trip-sketch-maps` | **Date**: 2026-10-10 | **Spec**: [spec.md](spec.md)

**Input**: Feature specification from `/specs/003-trip-sketch-maps/spec.md`

## Summary

Add F-712: a hand-drawn map of each trip and of each trip day, drawn in Rust from the same
OpenStreetMap vector tiles and open terrain tiles field notes use (decision 36), and stored
as two JPEGs per map: the owner form and a public form with the home end of the route cut.
They replace the F-207 route maps, which came from Amazon Location `GetStaticMap` (terms
bar keeping them, decision 36) and had no public form. A new `sketch_map` ingest task runs
per map in a Distributed Map after FieldNotes, over a manifest organize writes; each map
keeps its fingerprint in a marker object beside its picture, so parallel items never write the
trip record. The API serves the right form by viewer; the TV shows the trip map in Trip view,
each day's map on its day, and a four-second day map at each day change in the slideshow; the
phone shows the trip map with Share.

## Technical Context

**Language/Version**: Rust 1.93 (ingest, API, `crates/fieldnotes`); TypeScript 5.9 (TV in `packages/tv-ui`, phone in `apps/web`, CDK).

**Primary Dependencies**: existing `mvt-reader`, `tiny-skia`, `ttf-parser`, `contour`, `image`, `sha2`; no new crates. Bundled font: Patrick Hand (OFL), beside Instrument Serif.

**Storage**: JPEGs and fingerprint markers in S3 under `sketch/{h}/{trip}/` (data-model.md). No DynamoDB change.

**Testing**: cargo test with the existing fixture tiles (Florence, zoom 13, and terrain zoom 12) plus a zoom-6 Iceland fixture; determinism by byte-comparing two draws; home cut asserted on the public display list. jest for the TV day intro and Trip view; vitest for the phone page. No network in `make test`.

**Target Platform**: Lambda arm64 (ingest, API); Vega Virtual Device and Fire TV; phone browsers.

**Project Type**: serverless backend + TV app + web app (existing monorepo).

**Performance Goals**: one map in under 30 s with cold tiles (up to 12 vector and 6 terrain tiles), well inside the 5-minute ingest timeout; TV shows a stored JPEG of about 400 KB (SC-005).

**Constraints**: nothing invented (FR-002); no model calls (FR-019); tiles fetched at make time only; deterministic output (FR-006): all noise seeded from the trip id and day, no clock or random source.

**Scale/Scope**: about 9 maps per trip (one trip map plus one per day); 1600 × 900 px each.

## Constitution Check

| Principle | Gate | Pre-research | Post-design |
|-----------|------|--------------|-------------|
| I. Zero infrastructure for the user | Nothing new to run | PASS: hosted pipeline, existing apps | PASS |
| II. Vega-first TV app | No bundled native module | PASS: `Image` and a timer | PASS |
| III. Confirm before irreversible | No new irreversible action | PASS: derived, regenerable pictures | PASS |
| IV. Narrative from structured data only | Text shown from metadata | PASS: names from map data and trip landmarks, trip name and dates from the trip | PASS |
| V. Deterministic before expensive | No model calls | PASS: no model calls at all | PASS |
| VI. Tests run without AWS | Fakes behind traits | PASS: `Geography` memory fake already exists | PASS |

No violations.

## Project Structure

### Documentation (this feature)

```text
specs/003-trip-sketch-maps/
├── spec.md
├── plan.md
├── research.md
├── data-model.md
├── quickstart.md
├── contracts/api.md
├── checklists/requirements.md
└── tasks.md
```

### Source Code (repository root)

```text
services/crates/fieldnotes/
├── assets/PatrickHand-Regular.ttf, OFL-PatrickHand.txt
├── src/sketch.rs          # frame, legs, home cut, display list for trip and day maps
├── src/hand.rs            # seeded wobble, double pen pass, watercolour wash, hatching, paper
├── src/pictogram.rs       # landmark kind from name/kind, small sketched icons
└── tests/sketch.rs        # determinism, home cut, long hop, nothing invented
services/crates/core/src/sketch.rs        # SketchMap keys, which form a viewer gets
services/lambdas/ingest/src/sketch.rs     # sketch_map task: fingerprint, draw, store, delete
services/lambdas/ingest/src/organize.rs   # manifest of maps; no more GetStaticMap calls
services/lambdas/api/src/handlers/trips.rs   # GET /trips/{id}/map picks owner or public form
packages/tv-ui/src/screens/TripView.tsx, Slideshow.tsx, components/MapImage.tsx
apps/web/src/pages/trips.ts, trip.ts
infra/lib/mantelpiece-stack.ts           # SketchMaps Distributed Map after FieldNotes
```

**Structure Decision**: extend the field-notes crate (it already decodes tiles, draws with
tiny-skia and sets type) instead of a new crate; the hand-drawn look is a separate module so
field notes keep their clean editorial line.

## Complexity Tracking

None.
