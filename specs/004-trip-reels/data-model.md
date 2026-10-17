# Data Model: Trip Reels and Day Films

No DynamoDB change. The wire forms are in [contracts/api.md](contracts/api.md).

## Reel (computed, never stored)

`mantelpiece_reels::plan::Reel`: `trip`, `day` (null for the whole trip), `title {text, sub,
start_ms, end_ms}`, `track {id, title, artist, licence, credit, url, bpm}`, `duration_ms`,
`fade_out_ms`, `slides [{media, aspect, start_ms, end_ms, from, to, label}]`, `credit {text,
start_ms, end_ms}`. A window is `{cx, cy, z}`.

Fingerprint: SHA-256 (first 16 bytes, hex) of the style version and the reel's JSON with the
track URL blanked. Any change to photos, order, scores, labels, track or look changes it.

## Track (embedded)

`services/crates/reels/assets/music.json`, written by `scripts/music-library.sh`: `id`,
`title`, `artist`, `licence`, `source`, `file` (`music/{id}.m4a`), `bpm`, `duration_ms`,
`clarity`, `beats_ms`, `bars_ms`, `sections_ms`, `final_hit_ms`, `decay_end_ms`.

## Day film (S3)

`mantelpiece_core::film::FilmKeys`:

| Key | Content |
|---|---|
| `films/{h}/{trip}/day-{n}.mp4` | H.264 + AAC, 1920 x 1080, 30 fps, `+faststart` |
| `films/{h}/{trip}/day-{n}.fingerprint` | fingerprint, duration in ms, time made (RFC 3339), one per line |

## Film schedule (EventBridge Scheduler)

Group `default`; name `film-{24 hex of SHA-256(household/trip)}-day-{n}`; expression
`at(yyyy-mm-ddThh:mm:ss)` in UTC; target the film Lambda with `{household, trip, day}`;
deleted after completion.

## Event

`film.ready {trip, day, key}`: feed, visible to member households.
