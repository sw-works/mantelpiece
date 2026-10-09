# Quickstart: Field Notes

How to prove F-711 works. Contracts: [contracts/api.md](contracts/api.md). Data:
[data-model.md](data-model.md).

## Without AWS

```
make test                       # placement, certainty, mode, drawing, pipeline with fakes
cd services && cargo run --release -p ingest-local -- ~/Pictures/trip --out /tmp/fn --field-notes
open /tmp/fn/field-notes/       # <media>.png (photo + layer), <media>-mask.png (subjects, calm grid, chosen box)
```

`--field-notes` fetches real tiles from OpenFreeMap and terrain tiles over HTTPS (network,
no AWS credentials) and uses Rekognition-free placement (calm-region test only) unless
`--aws` is also given.

| Check | Expect |
|---|---|
| Photo with GPS and open sky | Map in the sky, 15 to 25 percent of the frame, dot at the position, caption with two-decimal coordinates |
| Photo with people in front of the sea | Map clear of every person box in the mask image |
| Close-up of a busy scene | No field note; report says `no calm region` |
| Photo without GPS, placed by an estimated route point | Ring, no dot, no coordinates |
| Photo without any place | No field note; report says `no place` |
| Coast, mountain, town photos | Mode `coast` (shoreline and water), `mountains` (contours, a peak), `town` (streets) |
| Any note, layer off | `<media>.jpg` identical to the TV rendition (byte compare of decoded pixels) |

## On the deployed stack

| Story | Run | Expect |
|---|---|---|
| US1 TV | organise a trip with GPS photos; on the Virtual Device open the trip's slideshow, press Field notes | highlights with notes show the whole photo with the map; the toggle hides it within a second; the setting survives a relaunch |
| US2 honesty | trip mixing GPS, estimated and unplaced photos | dot only on GPS photos; ring on estimated; none on unplaced; ring only for photos within 2 km of home |
| US3 placement | the hand-marked test set (100 highlights, subjects boxed) | no overlap between any map and a marked subject (SC-002) |
| US4 phone | owner trips page, open a highlight with a note, Share | one JPEG at the original's resolution with the map composed in |
| US4 public | open the trip's read-only code view and a member TV | field notes show rings only, never a dot or coordinates |
| US5 | add photos that change the highlights, wait for organize | new highlights get notes; dropped ones lose them; `field_notes.ready` on the feed |
