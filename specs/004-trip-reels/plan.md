# Implementation Plan: Trip Reels and Day Films

**Branch**: `004-trip-reels` | **Date**: 2026-10-16 | **Spec**: [spec.md](spec.md)

**Input**: Feature specification from `/specs/004-trip-reels/spec.md`

## Summary

A new crate, `crates/reels`, plans a reel deterministically from data ingest already has:
capture order, quality scores, pins and hides, subject boxes, landmark tags. It picks a
library track by trip and day, puts every cut on a bar line of the track's beat grid, gives
each slide two crop windows, and adds a title, landmark labels and the music credit. The API
serves the plan (F-713) and the TV plays it over the music, the music's clock driving the
slides. A new `film` Lambda draws the same plan into frames with tiny-skia (the crop formula
the TV uses), and pipes them with the music into the ffmpeg layer, which only encodes. Ingest
upserts a one-time EventBridge Scheduler schedule per recent day at 21:00 local (F-714).

## Technical Context

**Language/Version**: Rust 1.93 (reels crate, API, ingest, film Lambda); TypeScript 5.9 (TV,
CDK); Python 3.12 via uv (offline beat analysis only).

**Primary Dependencies**: existing `tiny-skia`, `ttf-parser`, `image`, `sha2`; new
`aws-sdk-lambda` (API, async invoke) and `aws-sdk-scheduler` (ingest). Offline: librosa.

**Storage**: S3 `music/{track}.m4a` (deployed by the stack), `films/{h}/{trip}/day-{n}.mp4`
and `.fingerprint`. No DynamoDB change.

**Testing**: cargo tests for the planner (bar-line cuts, windows inside the picture, labels,
selection, determinism), the renderer (placement, fades), the end-of-day times, the film
Lambda with a counting encoder, and the API routes; jest for the TV reel clock, crop maths,
overlays, the film screen and the buttons; vitest for the stack. A local example renders a
real film with ffmpeg (`cargo run --release -p mantelpiece-film --example local`).

**Target Platform**: Lambda arm64; Vega Virtual Device and Fire TV.

**Performance Goals**: a 31 s film with 7 photos renders in 8.3 s on an M2 (measured);
under 3 minutes on a 3008 MB Lambda (SC-002, to be measured on the stack).

**Constraints**: no model calls; the ffmpeg layer has no `drawtext` (text drawn in Rust); the
TV gets single presigned objects (MP4, not HLS); the TV never seeks the music.

## Constitution Check

| Principle | Gate | Result |
|-----------|------|--------|
| I. Zero infrastructure for the user | Nothing new to run | PASS: hosted pipeline, existing TV app |
| II. Vega-first TV app | No bundled native module | PASS: system-deployed w3cmedia players reached by runtime `require` in `.kepler` files |
| III. Confirm before irreversible | No new irreversible action | PASS: films are derived and regenerable |
| IV. Narrative from structured data only | Text from metadata | PASS: trip name, day number, grounded landmarks, track credit |
| V. Deterministic before expensive | No model calls | PASS: the planner is pure; films skip on an unchanged fingerprint |
| VI. Tests run without AWS | Fakes behind traits | PASS: `Encoder`, `Films`, `DayFilms` have memory fakes |

Amendment: F-708 "no video encoding" (decision 39). Originals are still never transcoded.

## Project Structure

```text
specs/004-trip-reels/ spec.md, plan.md, research.md, data-model.md, quickstart.md,
                      contracts/api.md, tasks.md
scripts/music-library.sh, scripts/music/beats.py     # fetch, transcode, beat grids
services/crates/reels/                               # planner, renderer, text, schedule
  assets/music.json                                  # committed beat grids
services/crates/core/src/film.rs, events.rs           # film keys, film.ready
services/lambdas/film/                                # the film Lambda (+ examples/local.rs)
services/lambdas/api/src/films.rs, handlers/reels.rs  # queue, reel, film routes
services/lambdas/ingest/src/films.rs, organize.rs     # end-of-day schedules
packages/tv-ui/src/screens/Reel.tsx, Film.tsx, components/CropMotion.tsx, music.ts, film.ts
apps/tv-vega/src/musicPlayer.kepler.ts, filmPlayer.kepler.tsx
infra/lib/mantelpiece-stack.ts                        # film Lambda, scheduler role, music
```

## Complexity Tracking

None.
