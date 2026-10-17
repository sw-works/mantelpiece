# Research: Trip Reels and Day Films

Decisions behind spec 004, with what was rejected. Much of it comes from the Editbay travel
POC (a separate project, 2026-10-16), which cut two trips to music by hand and then with
agents.

## R1. Plan deterministically, no model

**Decision**: `crates/reels` plans from ingest's metadata: order from capture time, selection
from the F-204 score, pins and highlights, moves from F-711 subject boxes, labels from
landmark tags at 85 % or more.

**Rationale**: the expensive part of the POC (a vision model describing every clip, about
19k tokens a clip) is already done by ingest here. What remains is arithmetic on bars and
windows. Constitution V.

**Rejected**: a Bedrock planner in W1. The agent runtime has no tool-calling path and no
per-photo data (W1 calls fixed tools for text only), and it would add cost and a fact check
for no better plan.

## R2. Music: a small CC BY library with offline beat grids

**Decision**: three Kevin MacLeod tracks (CC BY 3.0, Wikimedia Commons), transcoded to AAC-LC
in `.m4a` and analysed offline with librosa (`scripts/music-library.sh`); the grid is embedded
in the crate. Only tracks whose first bar falls within 4 s are used, so the music starts at
its own 0.

**Rationale**: no network or model at plan time; CC BY allows published video with credit.
Vega's media pipeline needs AAC-LC (decision 34: HE-AAC switched codecs).

**Rejected**: a music API (licence per use, network at plan time); seeking into a track on
the TV (the Virtual Device's media service wedged on seeks, decision 34).

## R3. Bars, not beats

**Decision**: slides last whole bars (2 to 2.5 s at these tempos); the title slide and the
last slide are longer; a slide grows to fit its label's reading time (1 s + 0.3 s a word).

**Rationale**: in the POC, cuts every bar read as a montage; every beat was frantic, and
labels on 1-bar slides were too short to read.

## R4. Moves

**Decision**: crop windows (centre in picture coordinates, zoom over a cover fit), linear
between `from` and `to`; portraits tilt along their band toward the subject, panoramas pan,
the rest push in and pull out alternately; the speed is capped so content takes 4 s or more
to cross the frame.

**Rationale**: the POC's review found the same move twice in a row and a pan crossing the
frame in 1.9 s (it juddered and tripped cut detection).

## R5. Films drawn in Rust, encoded by ffmpeg

**Decision**: the film Lambda draws every frame with tiny-skia using the TV's crop formula
(bilinear, sub-pixel) and composites text layers drawn once; ffmpeg reads raw RGBA frames,
encodes H.264 (CRF 20) and AAC, trims and fades the music, normalises to -14 LUFS and limits
at -3 dBFS.

**Rationale**: ffmpeg's `crop` cannot change size per frame and `zoompan` cannot keep a crop
window of another aspect; the layer has no `drawtext` (decision 39). Drawing in Rust is
testable and matches the TV exactly. The limiter: AAC overshoots a -1 dBTP limit by up to 1.3
dB (measured in the POC; the first local film measured -0.9 dBTP with loudnorm alone, -2.1
with the limiter).

**Rejected**: MediaConvert (another service and its job model for a few renders a day);
per-frame `scale` with `eval=frame` (reconfigures the graph every frame).

## R6. MP4, not HLS

**Decision**: one MP4 with `+faststart`, presigned, played by the w3cmedia `VideoPlayer` with
`src` set after the surface handle.

**Rationale**: the TV gets presigned single objects; a presigned playlist does not sign its
segments.

## R7. End of day

**Decision**: after each organize, one EventBridge Scheduler one-time schedule per day of a
touched trip no more than two days old, at 21:00 in the day's most common photo offset, or
ten minutes from now for a finished day; named `film-<hash>-day-<n>`, upserted, deleted after
it runs. Older days get films on request.

**Rationale**: households have no time zone and the stack has no working timer (the nightly
tick reaches nothing); the photos' own offsets say where the family is. Upserting by name
moves a schedule instead of adding one.

## R8. Memory

**Decision**: 3008 MB, 15 min, 4 GB `/tmp`, one async retry.

**Rationale**: new accounts are capped at 3008 MB until a quota increase (RecordingFn uses the
same); a broken render must not run three times for 15 minutes each.
