# Research: Field Notes

Phase 0 for `plan.md`. Each decision says how it was verified: **docs** (read in the
source's documentation on 2026-10-09), **code** (read in this repo), or **unverified**
(believed, to be checked on the first deploy or in the first spike task).

## R1. Map data source: OpenStreetMap, not Amazon Location

**Decision**: draw field notes from OpenStreetMap vector tiles (OpenMapTiles schema), fetched
from OpenFreeMap at make time; credit "© OpenStreetMap contributors" wherever a field note is
shown. A Protomaps extract in our own S3, read by range requests, is the fallback behind the
same trait if OpenFreeMap becomes unavailable.

**Rationale** (docs): AWS Service Terms §82.4(a) forbid storing or caching "the data
provided to you from Amazon Location Service (e.g., map tiles …)" with exceptions only for
routes, geocodes and legal holds. A field note is kept in S3 for as long as the photo, so a
map drawn from Location tiles is at best a grey area and a `GetStaticMap` image is plainly
stored Location Data. §82.4(d) also bars combining Location Data with open-licensed data,
so the two cannot be mixed in one note. OpenStreetMap is ODbL: a rendered map is a Produced
Work, kept freely with attribution ("© OpenStreetMap contributors", one credit per page is
enough for several images). OpenFreeMap: no key, no request limits, commercial use allowed,
attribution "OpenFreeMap © OpenMapTiles Data from OpenStreetMap" (docs).

**Consequence for the baseline** (code): the existing F-207 route maps are `GetStaticMap`
images stored under `Route.map_key`. That is the case §82.4 forbids. Logged as an open
question in the decision log; not changed by this feature.

**Alternatives**: Amazon Location `vector.basemap` with `ContourLines` (the only source with
ready contours; rejected on the storage terms); Protomaps planet extract in S3 (about 120 GB
for the planet, range reads from Lambda proven by Protomaps' own AWS deployment; kept as the
fallback, not the first choice, to avoid hosting the planet for a hackathon).

## R2. Contours: open terrain tiles, contoured in Rust

**Decision**: fetch elevation from AWS Open Data Terrain Tiles (Terrarium PNG,
`https://s3.amazonaws.com/elevation-tiles-prod/terrarium/{z}/{x}/{y}.png`), decode
`elevation = R*256 + G + B/256 - 32768`, and trace contour lines with marching squares (the
`contour` crate). Interval chosen from the relief in view (10, 20, 50 or 100 m, every fifth
line heavier).

**Rationale**: OpenMapTiles has peaks (`mountain_peak` with `ele`) but no contours. Terrain
Tiles are open data (attribution to the source datasets, listed on the registry page) and
storing a derived drawing is allowed. Marching squares over a 256 px tile is milliseconds.

**Verified**: Terrarium encoding and bucket path (docs, registry.opendata.aws/terrain-tiles);
crate builds for aarch64 (unverified until the first build).

## R3. Tile decoding, drawing, text

**Decision**: `mvt-reader` 2.5 (MIT) to decode vector tiles; `tiny-skia` 0.12 (BSD-3) to
draw anti-aliased strokes into a transparent RGBA canvas; `ttf-parser` 0.25 (MIT/Apache)
to turn glyphs into outlines fed to `tiny_skia::PathBuilder`, so labels get a halo (stroke
wide in the paper colour, then fill). One bundled font under the SIL Open Font Licence, the
same family the phone pages use (Figtree), shipped as bytes in the crate.

**Rationale**: all pure Rust, no system libraries, so cargo-lambda's zig build for arm64
works as it does for the other Lambdas (docs for licences and APIs; aarch64 build
unverified). No shaping engine is needed for at most four short English labels.

**Alternatives**: `geozero` (heavier); `fontdue` (bitmaps only, no outline for halos);
`resvg` via SVG (adds a parser and font database for no gain).

## R4. Finding subjects

**Decision**: two sources, unioned with a margin.

1. **At enrich, no new calls**: keep the boxes Rekognition already returns. `DetectLabels`
   label `Instances[].BoundingBox` for common objects (people, vehicles, animals, some
   buildings), `DetectFaces` `FaceDetails[].BoundingBox`, `DetectText` `TextDetections[]`
   of type `LINE`. All are `{Left, Top, Width, Height}` ratios (docs).
2. **At field-note time, one call per highlight**: Nova Lite through Bedrock Converse, asked
   for the photo's main subjects as JSON `[{"label", "box": [x1, y1, x2, y2]}]` on Nova's
   0 to 1000 grid, temperature 0 (docs: Nova user guide, "bounding boxes ... on a scale of
   [0, 1000)"). This is what catches a peak, a temple roof or a lone tree, which
   Rekognition does not box.

**Rationale**: Rekognition boxes are tight and free (we already pay for the calls); the
model call covers scenery. Highlights are about 24 per trip, inside constitution V (keepers
only). A malformed answer is ignored, leaving the Rekognition boxes and the calm-region test.

## R5. Placement: calmest region outside the subjects

**Decision**: deterministic, on the TV rendition scaled to a 96-cell-wide grid.

- Per cell: detail = mean Sobel gradient magnitude; tone = mean luminance; sky-likeness =
  cell in the upper half, low detail, bright or blue-dominant; water-likeness = low detail
  with horizontal-only gradients.
- Mask: subject boxes padded by 4 percent of the shorter side; a 3 percent frame margin.
- Candidates: rectangles of 15, 20 and 25 percent of the frame at aspect 4:3, 3:2 and 1:1
  (portrait photos also 3:4), slid over the grid on a coarse step via an integral image.
- Score = mean detail, minus a bonus for sky- or water-likeness, plus a penalty for the
  frame's centre third. Lowest wins if its mean detail is under the readability threshold;
  otherwise no field note (FR-011).
- Ink: light when the region's mean luminance is under 0.45, else dark.

**Rationale**: cheap, testable without AWS (constitution VI), and the threshold is a single
number to tune against the SC-002/SC-003 test set. Unverified: the threshold value, tuned on
real photos in the first spike.

## R6. Where in the pipeline

**Decision**: a new ingest task, `field_note`, run by a Distributed Map over a manifest that
`organize` writes after `OrganizeFinal`, four in flight per household, mirroring
`EnrichKeepers` (code: `infra/lib/mantelpiece-stack.ts`). `organize` computes each
highlight's fingerprint and lists only missing or stale notes; it drops notes for photos
that stopped being highlights and sets `Household.home`.

**Rationale**: the ingest Lambda already has storage, blobs and vision; field notes are an
organize-time artefact like highlights and route maps. A separate Lambda would duplicate
those clients for no isolation gain. The ingest timeout (5 min) and memory (2048 MB) cover
a 1920 px render plus a few tile fetches per photo.

## R7. How the TV shows it

**Decision**: Field notes mode in the slideshow shows the whole photo (`contain`, no Ken
Burns zoom) on the paper-dark background with the transparent `fieldnote.png` absolutely
over it at the same rect, and fades the bottom controls after three seconds without a key
press. Plain mode is unchanged.

**Rationale** (code): `Slideshow.tsx` draws the photo with `resizeMode="cover"` at
1920×1080 inside `KenBurns`, with a dark band over the bottom 300 px. A map placed in the
photo's frame would be cropped off a portrait photo and zoomed out of register by Ken Burns.
Both images share one aspect ratio, so two `contain` images in the same box align exactly.

## R8. Full-resolution share

**Decision**: `POST /media/{id}/fieldnote/full` in the API Lambda decodes the original,
rasterises the stored display list at that size, composites, writes `fieldnote-full.jpg`
and returns a presigned URL. The phone shares it with the Web Share API (files) where
available, else offers a download.

**Rationale**: an API Gateway Lambda response caps at 6 MB, smaller than many full-size
JPEGs, so the picture goes through S3. Re-rasterising the vector display list avoids a
second map fetch and gives sharp lines at any size.

## R9. Home area

**Decision**: at organize, the centre of the busiest 0.02° cell (about 2 km) among the
household's geotagged photos outside any trip, if it holds at least 20 photos; stored as
`Household.home`. Inside 2 km of it, precise certainty is downgraded to a ring.

**Rationale** (code): `organize` already lists every media item; trips are by definition
away from the everyday cluster. Unverified: whether 20 is the right floor for small archives.
