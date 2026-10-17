# Quickstart: Trip Reels and Day Films

## Music library

```sh
make music        # fetch the CC BY tracks, transcode to .m4a in infra/assets/music/,
                  # write services/crates/reels/assets/music.json (committed)
```

`make deploy` uploads `infra/assets/music/*.m4a` to the media bucket under `music/`.

## A film from a folder of photos, without AWS

```sh
cd services
cargo run --release -p mantelpiece-film --example local -- <jpeg-dir> ../infra/assets/music out.mp4 "Florence"
```

The same planner, renderer and encoder the Lambda runs, with in-memory storage and the local
ffmpeg. Photos are one day, in file-name order.

## On the stack

- Reel: Trip view → Play reel (whole trip) or Day reel; `GET /trips/{trip}/reel?day=N`.
- Film now: Trip view → Make day film (owner), or `POST /trips/{trip}/film {"day": N}`; the
  ticker says "Day N film is ready" and Day film appears.
- Film at the end of the day: upload photos of today; after organize, a schedule
  `film-…-day-N` exists in EventBridge Scheduler for 21:00 local.
