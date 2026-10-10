# Data Model: Trip Sketch Maps

No database change. Everything is in S3 beside the trip.

## Sketch map objects

| Key | Content |
|-----|---------|
| `sketch/{h}/{trip}/trip.png` | trip map, owner form, 1600 × 900 PNG |
| `sketch/{h}/{trip}/trip-public.png` | trip map, public form |
| `sketch/{h}/{trip}/trip.fingerprint` | hex sha256 the pair was drawn from |
| `sketch/{h}/{trip}/day-{n}.png`, `day-{n}-public.png`, `day-{n}.fingerprint` | the same per day, `n` the day's index |

`mantelpiece_core::sketch::SketchKeys` builds them; `keys(trip, days)` lists every object of
a trip for deletion.

## Fingerprint

sha256 over: `STYLE_VERSION`, the map (`trip` or `day-n`), the trip name and dates (trip map),
each included route point's position (5 decimals) and `estimated` flag, each drawn landmark's
name, kind and position, and the home point (4 decimals) or none. A changed fingerprint
means redraw both forms; equal means skip.

## Display list (in memory only)

`SketchDrawing { size, view, layers: [Wash | Ink | Hatch], legs: [Leg], days: [DayMark],
pictograms: [Pictogram], labels: [Label], title: Option<Cartouche>, scale, compass,
home: Option<HomeWash>, credit }`; `public()` drops `home`. Not stored: unlike field notes,
nothing re-rasterises it later.

- `Leg { day, points, estimated, hop }`: `hop` for a gap over 300 km, drawn as a dotted arc.
- `DayMark { day, at }`: circled number at the leg's last point outside the home area.
- `Pictogram { kind, at, name }`, kind one of waterfall, lighthouse, glacier, church, peak,
  hot_spring, castle, beach, lake, town, star.

## Manifest item

`{"household": h, "trip": t, "day": n | null}`; `null` is the trip map.
