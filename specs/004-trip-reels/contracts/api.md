# Contracts: Trip Reels and Day Films (F-713, F-714)

Everything the TV, the API, the film Lambda and the stack agree on. Times are integer
milliseconds from the start of the reel unless named otherwise.

## Crop windows

A slide's motion is two crop windows, `from` at `start_ms` and `to` at `end_ms`, interpolated
linearly between them.

```json
{ "cx": 0.5, "cy": 0.45, "z": 1.08 }
```

- `cx`, `cy`: the window centre in normalised source coordinates (0 left/top, 1 right/bottom),
  in display orientation.
- `z`: zoom over a cover fit of the frame. At `z = 1.0` the picture just covers the frame
  (scale `s0 = max(frameW / w, frameH / h)`); at `z = 1.1` it is drawn 10 % larger.
- The planner only emits windows that stay inside the picture; a viewer still clamps.

To draw a slide at progress `p` (0 to 1) on a frame `F` (TV: 1920 × 1080) for a picture of
aspect `a = w / h`:

```
c = lerp(from, to, p)               (cx, cy and z each)
s = max(F.w / w, F.h / h) * c.z     picture drawn at (w * s, h * s)
left = F.w / 2 - c.cx * w * s       clamped to [F.w - w * s, 0]
top  = F.h / 2 - c.cy * h * s       clamped to [F.h - h * s, 0]
```

## GET /trips/{trip}/reel?day=N

Any viewer who may see the trip (owner, co-editor, contributor, member household). `day`
omitted: the whole trip. Computed on demand from the trip and its media; nothing is stored.

200:

```json
{
  "trip": "t1",
  "day": 2,
  "title": { "text": "Kyoto", "sub": "Day 2", "start_ms": 0, "end_ms": 3600 },
  "track": {
    "id": "carefree",
    "title": "Carefree",
    "artist": "Kevin MacLeod",
    "licence": "CC BY 3.0",
    "credit": "Music: \"Carefree\" by Kevin MacLeod, CC BY 3.0",
    "url": "https://<presigned music/carefree.m4a>",
    "bpm": 95.7
  },
  "duration_ms": 41240,
  "fade_out_ms": 2000,
  "slides": [
    {
      "media": "m1",
      "aspect": 1.3333,
      "start_ms": 0,
      "end_ms": 4737,
      "from": { "cx": 0.5, "cy": 0.5, "z": 1.0 },
      "to": { "cx": 0.52, "cy": 0.46, "z": 1.1 },
      "label": { "text": "Kiyomizu-dera", "start_ms": 600, "end_ms": 4200 }
    }
  ],
  "credit": { "text": "Music: \"Carefree\" by Kevin MacLeod, CC BY 3.0", "start_ms": 38000, "end_ms": 41240 }
}
```

- `slides` are in play order, back to back: `slides[i].end_ms == slides[i + 1].start_ms`, and
  the last ends at `duration_ms`. Every cut after the first slide lands on a bar line of the
  track; the music starts at its own 0 together with the first slide.
- `label` is present only for a landmark grounded in the metadata (constitution IV) and only
  when the slide is long enough to read it; otherwise `null`.
- `title.sub` is the day ("Day 2") for a day reel, `null` for the whole trip.
- The TV shows pictures with `GET /media/{id}/url?rendition=tv` as today.
- 404 when the trip or day does not exist; 422 `{"error":"too few photos"}` when the day has
  fewer than 3 usable photos.

## GET /trips/{trip}/film?day=N

Any viewer who may see the trip. 200 when a film of that day exists:

```json
{ "day": 2, "url": "https://<presigned films/...mp4>", "duration_ms": 41240, "made_at": "2026-10-17T20:01:12Z" }
```

404 `{"error":"no film yet"}` otherwise.

## POST /trips/{trip}/film

Owner or co-editor of the trip. Body `{ "day": 2 }`. Queues a render of that day now (the
film Lambda, invoked asynchronously) and answers 202 `{ "queued": true }`. 404 for an unknown
trip or day; 403 for other viewers.

## Event: film.ready

Written to the household feed (and copied to member households) when a day film is stored:

```json
{ "type": "film.ready", "trip": "t1", "day": 2, "key": "films/h1/t1/day-2.mp4" }
```

The TV shows "Day 2 film is ready" in the ticker and refreshes the film button of that trip.

## Film Lambda (`film` binary)

Invoked asynchronously by the API (`POST /trips/{trip}/film`) and by one-time EventBridge
Scheduler schedules. Payload:

```json
{ "household": "h1", "trip": "t1", "day": 2 }
```

- Plans the day reel for a 1920 × 1080 frame, and skips when the fingerprint marker of that
  day matches the plan.
- Reads the TV rendition of each slide (`renditions/{h}/{m}/tv.jpg`) and the track
  (`music/{track}.m4a`), renders an H.264 + AAC MP4 with ffmpeg, writes it with its marker,
  and emits `film.ready`.
- A day with fewer than 3 usable photos is skipped (no film, no error).

Environment: `TABLE_NAME`, `MEDIA_BUCKET`, `EVENT_BUS`, `FFMPEG` (default `/opt/bin/ffmpeg`).

## End-of-day schedules

After each organize run, for each day of a touched trip whose local date is no more than two
days before today, the ingest Lambda upserts a one-time schedule named
`film-{household}-{trip}-day-{n}` (hashed to Scheduler's 64-character limit) at
`max(21:00 local on that day, now + 10 min)`, targeting the film Lambda with the payload above.
Local time uses the UTC offset most common among that day's photos (UTC when none is known).
Schedules delete themselves after running.

Ingest environment: `FILM_FUNCTION_ARN`, `FILM_SCHEDULER_ROLE_ARN` (empty disables
scheduling), `FILM_SCHEDULE_GROUP` (default `default`).

API environment: `FILM_FUNCTION_NAME` (empty: `POST /trips/{trip}/film` answers 503).

## S3 keys

| Key | Written by | Content type |
|---|---|---|
| `music/{track}.m4a` | the stack (deployed from `infra/assets/music/`) | `audio/mp4` |
| `films/{h}/{trip}/day-{n}.mp4` | film Lambda | `video/mp4` |
| `films/{h}/{trip}/day-{n}.fingerprint` | film Lambda | `text/plain` (fingerprint, then duration in ms) |

Nothing is written under `originals/`, so films never re-enter ingest. Account deletion and
trip removal delete `films/{h}/{trip}/`.
