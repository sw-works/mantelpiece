# API contract changes: Trip Sketch Maps

## `GET /trips/{trip}/map?day=` (changed)

Same request. The response now points at the sketch map:

```json
{ "url": "https://…/sketch/h/t/day-3.jpg?…", "inferred": false, "credit": "© OpenStreetMap contributors · Terrain Tiles (AWS Open Data)" }
```

- Owner household: the owner form. Member households and code views (`?code=`): the public
  form. 404 `no map yet` when the marker object is missing.
- `inferred` is true when any drawn leg is estimated (unchanged meaning).

## Trip JSON (changed)

`days[].route.map_key` and `route.map_key` are no longer written. Clients decide whether to
ask for a map from `days[].route.points` (non-empty) as before; a 404 means none yet.

## Ingest task (new)

```json
{ "task": "sketch_map", "household": "h", "trip": "t", "day": 3 }
```

Output: `{ "trip": "t", "day": 3, "made": true, "skipped": null }`, or `made: false` with
`skipped`: `up to date`, `no place`, `map unavailable`, `no trip`.
