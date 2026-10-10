# Research: Trip Sketch Maps

## R1. Zoomed-out map data

**Decision**: OpenFreeMap vector tiles at the zoom that fits the frame (trip maps of a
country land at zoom 5 to 7, day maps at 9 to 12), the same TileJSON as field notes.

**Rationale**: fetched 2026-10-10 over Iceland: zoom 5 carries `water` (ocean),
`landcover` (ice), `transportation` (motorway), `place` (city, town); zoom 6 adds lakes,
rivers and trunk roads; zoom 7 adds `mountain_peak` (peaks and volcanoes) and primary roads.
The coastline is the ocean polygon's edge, as in field notes. TileJSON says zoom 0 to 14.

**Alternatives**: Natural Earth for coasts at country scale (another source and licence to
carry, coarser than zoom 6 OSM); Amazon Location (cannot be kept, decision 36).

## R2. Terrain for hatching

**Decision**: AWS Open Data Terrarium tiles at the vector zoom plus one (capped at 12), the
existing decode and contour code; hachures drawn as short strokes across contour lines,
pointing downhill, only where relief in frame exceeds 200 m.

**Rationale**: hachures read as hand-drawn far more than contours, and need only the contour
polylines plus a downhill test (sample the elevation grid either side of the line).

## R3. Hand-drawn look, deterministically

**Decision**: every polyline is resampled at 4 px steps and pushed along its normal by a
smooth 1D value noise (amplitude 1.2 px at trip scale, period about 60 px), seeded by a hash
of the trip id, the map (trip or day n) and the feature's order; a second pass at 0.5 px
offset and 35 percent ink. Washes: the polygon filled three times at low alpha with its own
wobble seeds (uneven edge), then a 3 px edge stroke at 15 percent (pigment pooling). Paper:
a warm off-white with per-pixel grain from a hash of the pixel position. Dashes: hand-length
dashes (7 to 11 px, varied by the same noise).

**Rationale**: sums of seeded noise give a hand quality without randomness at run time, so
two draws are byte-identical (FR-006, SC-003). tiny-skia has no blur; repeated low-alpha
fills give the soft edge.

**Alternatives**: an image model restyling a plain map (invents geography, FR-002, FR-019);
a sketch filter over a rendered map (needs a blur and edge pipeline, harder to keep
names crisp).

## R4. Typeface

**Decision**: Patrick Hand (SIL OFL 1.1, Google Fonts) for names, day numbers and the title;
fetched from `google/fonts` `ofl/patrickhand`, 215 KB. Covers ð, þ, æ, ö and the Latin
accents checked (fontTools cmap, 2026-10-10). Names with letters outside the font fall back
to the English or Latin name from map data, else are dropped (spec edge case).

## R5. Where the work runs

**Decision**: a `sketch_map` task per map in a Distributed Map (`SketchMaps`, concurrency 4)
after `FieldNotes`, over `manifests/{h}/sketch-maps-{run}.json` written by the final
organize: one item per trip and per trip day. The task computes the fingerprint, returns at
once when the marker matches, else draws and stores both forms and the marker.

**Rationale**: one Lambda per map keeps a cold-tile run (up to about 30 s) far inside the
5-minute timeout. Markers beside the pictures (as field notes' share marker does) mean parallel
items never read-modify-write the trip record.

**Alternatives**: drawing inside organize (serial, timeout risk, and organize runs twice);
storing fingerprints on `Route` (concurrent writes to one trip item).

## R6. Retiring F-207's route maps

**Decision**: organize stops calling `Maps::route_map`; the API's `GET /trips/{id}/map`
serves sketch maps only. Old `maps/{h}/...png` objects are left in place and unused (no
reader after this change). The "all trips" map (`GET /map`, memories) still uses Amazon
Location and stays an open item under decision 36.

## R7. Home and public forms

**Decision**: the home area is `Household.home` with the 2 km radius field notes use. Route
points inside it are removed from both forms; the owner form draws a soft wash circle at the
home point clipped to the frame, the public form draws nothing there. Day numbers move to the
last point outside the area. Members' households and code views get the public form.
