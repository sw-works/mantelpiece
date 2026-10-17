# Tasks: Trip Reels and Day Films

`[x]` done and tested locally; `[~]` done, needs the Virtual Device or the stack; `[ ]` open.

## Setup

- [x] T001 Music library: `scripts/music-library.sh`, `scripts/music/beats.py`, `make music`;
  `services/crates/reels/assets/music.json` committed
- [x] T002 Contracts: `specs/004-trip-reels/contracts/api.md`

## Core (crates/core, crates/reels)

- [x] T010 `FilmKeys` and `film.ready` (`crates/core/src/film.rs`, `events.rs`)
- [x] T011 Planner: selection, bar-line timing, labels, moves, fingerprint (`plan.rs`, 8 tests)
- [x] T012 Text layers (`text.rs`) and frame renderer (`render.rs`)
- [x] T013 End-of-day times and schedule names (`schedule.rs`)

## Backend

- [x] T020 Film Lambda: plan, skip when current, draw, encode, store, announce
  (`lambdas/film`, counting-encoder tests)
- [x] T021 Local render example with real ffmpeg (`examples/local.rs`): 31 s film from 7
  photos in 8.3 s on an M2; -14.9 LUFS, -2.1 dBTP
- [x] T022 API: `GET /trips/{trip}/reel`, `GET` and `POST /trips/{trip}/film`; films deleted
  with the account (`handlers/reels.rs`, `films.rs`, API test)
- [x] T023 Ingest: upsert end-of-day schedules after organize (`lambdas/ingest/src/films.rs`)
- [x] T024 EventBridge Scheduler create/update and self-delete on the stack (decision 40)

## TV

- [x] T030 `MusicPlayer`, `FilmPlayer`, client methods, `film.ready` in the feed
- [x] T031 Reel screen: music clock, crop motion, preloading, overlays, fades, Pause, silent
  fallback (`Reel.tsx`, `CropMotion.tsx`)
- [x] T032 Film screen; Trip view (Play reel, Day reel, Day film, Make day film) and Memories
  home (Play reel)
- [~] T033 Vega `AudioPlayer` plays the `.m4a` and reports `currentTime`
  (`musicPlayer.kepler.ts`). Virtual Device: loads, then the clock stalls near 0.5 s and the
  reel goes silent; needs a real Fire TV
- [~] T034 Vega `VideoPlayer` plays the MP4 in URL mode (`filmPlayer.kepler.tsx`). Virtual
  Device: decodes and shows frames, playback crawls; needs a real Fire TV

## Infra

- [x] T040 Film Lambda (3008 MB, 15 min, 4 GB, ffmpeg layer, one retry), scheduler role,
  ingest scheduler permissions, API invoke grant, music deployment; stack tests
- [x] T041 Stack tests synth under one temporary root removed afterwards (the `cdk.out*` leak)
- [~] T042 `make deploy`, then: a reel on the Virtual Device (done), a schedule created by an
  upload (done, films made and schedules deleted), a film made from Trip view (open: the
  Virtual Device lost its network)
- [x] T043 `make down` deletes leftover `film-*` schedules (`scripts/film-sweep.sh`)

## Docs

- [x] T050 Spec, plan, research, data model, quickstart; F-713 and F-714 in the architecture;
  decision 39; README music credits
- [ ] T051 Demo beat for the video (hackathon.md section 5)
