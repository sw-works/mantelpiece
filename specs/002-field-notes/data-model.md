# Data Model: Field Notes

Additions to the baseline model (`specs/001-mantelpiece-baseline/data-model.md`). All new
fields are `#[serde(default)]`, so rows written before this feature still read.

## Subject (new, in `mantelpiece-core::media`)

A main subject of a photo, as a box in the photo's upright frame.

| Field | Type | Notes |
|---|---|---|
| `kind` | `person` \| `face` \| `text` \| `object` \| `landscape` | Where it came from: Rekognition labels with instances (`person`, `object`), faces, text, or the model's subject call (`landscape`) |
| `label` | String | `Person`, `Boat`, `Temple roof`; for display in debugging only |
| `bbox` | `NormBox { left, top, width, height }` | Fractions of the upright frame, 0 to 1 |
| `confidence` | f32 | 0 to 1 |

Validation: boxes are clamped to the frame; boxes under 0.5 percent of the frame are
dropped (noise); text boxes are kept at any size.

## Enrichment (changed)

| Field | Change |
|---|---|
| `subjects: Vec<Subject>` | NEW. Filled at enrich from the Rekognition calls already made (label instances, faces, text lines); `landscape` subjects added when a field note is made. Inherited by non-keepers like the rest of enrichment. |

## FieldNote (new, on `MediaItem.field_note: Option<FieldNote>`)

The map layer for one highlight. The drawing itself is a display list in S3; the record
says where it is and what it was made from.

| Field | Type | Notes |
|---|---|---|
| `placement` | `NormBox` | Where the map sits in the upright frame; 15 to 25 percent of its area |
| `mode` | `coast` \| `mountains` \| `town` \| `trail` | Chosen from the geography in view (FR-004) |
| `certainty` | `Certainty` | See below |
| `ink` | `light` \| `dark` | Chosen from the brightness under `placement` (FR-005) |
| `caption` | `Caption { place, date, coords: Option<String> }` | `coords` only for `Certainty::Precise` outside the home area, already rounded to two decimals |
| `source` | `openstreetmap` | Map data source (research R1); terrain for contours from open terrain tiles (R2) |
| `credit` | String | `© OpenStreetMap contributors`, plus the terrain credit when contours are drawn; shown wherever the note is shown (FR-025) |
| `drawing_key` | String | `renditions/{h}/{m}/fieldnote.json`, the display list |
| `fingerprint` | String | Hash of what it was made from: photo sha256, certainty and position, the day's route, the style version. A different fingerprint at organize means remake |
| `made_at` | RFC 3339 | |
| `feature` | `Feature { name, kind: landmark \| peak \| water \| place, at, region: Option<String>, country: Option<String> }` | The main feature for "This place" (US6): the trip landmark tied to the photo, else the most prominent named feature near the marker. Its description comes from the cached encyclopedia facts (F-210) by name and position. |

### Certainty

| Variant | When | Shown |
|---|---|---|
| `Precise { at: GeoPoint }` | The photo's own EXIF GPS, outside the home area | Dot; coordinates in the owner's caption |
| `Approximate { at: GeoPoint, radius_m: u32 }` | Position from an estimated route point (F-206) or a landmark whose `media` lists the photo (F-210), or a precise point inside the home area | Ring of `radius_m`; no dot, no coordinates |
| (no note) | None of the above | Nothing; the photo gets no field note (FR-014) |

Radius for an estimated route point: half the distance to the nearer geotagged neighbour,
clamped to 300 m to 5 km. For a landmark: `Landmark::reach_m()`. Home area: 2 km.

## Drawing (S3 JSON, `renditions/{h}/{m}/fieldnote.json`)

A resolution-independent display list in the coordinates of `placement` (0 to 1 each way),
so the same note rasterises at TV size, at the original's full size for sharing, and in the
public form without a second map fetch.

| Field | Notes |
|---|---|
| `version` | Style version; part of the fingerprint |
| `strokes[]` | `{ class: water \| coast \| road_major \| road_minor \| path \| contour \| contour_index \| route, points: [[x,y]...], closed }`, simplified to the target size |
| `labels[]` | `{ text, x, y, size, class: place \| water \| peak }`, at most four (FR-003) |
| `marker` | `{ kind: dot \| ring, x, y, r }`; `dot` only for `Precise` |
| `fade` | Edge feather width as a fraction of the box (FR-006) |
| `grain_seed` | Seed for the paper grain, from the media id, so re-renders match |

The public form (FR-016) is the same list with `marker.kind` forced to `ring` (radius at
least 500 m) and `caption.coords` dropped; it is rasterised, never stored as a second list.

## Rasters (S3, beside the TV rendition)

| Key | What | Who gets it |
|---|---|---|
| `renditions/{h}/{m}/fieldnote.png` | Transparent PNG, the TV rendition's size, owner form | The household's own TV and paired phones |
| `renditions/{h}/{m}/fieldnote-public.png` | Same size, public form | Member households, the read-only view, shared links |
| `renditions/{h}/{m}/fieldnote-full.jpg` | The original composited with the owner form at full resolution | Made on request for sharing (FR-020); deleted with the photo |

All four field-note objects join the per-photo delete list in `POST /media/purge` and
account deletion (FR-022, F-604).

## Household (changed)

| Field | Change |
|---|---|
| `home: Option<GeoPoint>` | NEW. Set at organize: the centre of the busiest 0.02° cell among the household's geotagged photos outside any trip, when that cell holds at least 20 photos. `None` until then. |

## TV session (changed, `packages/tv-ui/src/session.ts`)

| Field | Change |
|---|---|
| `fieldNotes?: boolean` | NEW. The Field notes toggle, per TV, across trips and restarts (FR-017). |

## State: when a field note exists

```
highlight, place known, calm region found, map data fetched  ──make──▶  FieldNote
FieldNote, fingerprint unchanged at organize                  ──keep──▶  FieldNote
FieldNote, fingerprint changed                                ──remake─▶ FieldNote (or none)
FieldNote, photo no longer a highlight                        ──drop───▶ none (objects deleted)
FieldNote, photo soft-removed                                 ──hidden─▶ kept, not served; back on restore
map fetch failed                                              ──none───▶ retried next organize
```
