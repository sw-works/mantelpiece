# Quickstart: Trip Sketch Maps

How to prove F-712 works. Contracts: [contracts/api.md](contracts/api.md). Data:
[data-model.md](data-model.md).

## Without AWS

```
make test
cd services && cargo run --release -p mantelpiece-fieldnotes --example sketch -- ../testbed/iceland-2015 /tmp/sketch
open /tmp/sketch/
```

The example builds a trip from the album's EXIF (days by date, GPS routes), fetches real
tiles with a disk cache and writes `trip.png`, `trip-public.png` and `day-N.png`.

| Check | Expect |
|---|---|
| Trip map | the whole island, coast in ink, glaciers washed, legs 1 to 8 with circled numbers, pictograms at named places, title box with name and dates |
| Day map | the day's drive zoomed in, photo dots, the day's landmarks named |
| Draw twice | byte-identical PNGs |
| Public form | no route inside 2 km of the home point given with `--home` |

## On the deployed stack

| Story | Run | Expect |
|---|---|---|
| US1 | upload the Iceland album; open the trip on the Virtual Device | trip map in Trip view |
| US2 | choose a day; play the slideshow across a day change | day map in the day section; four-second day map before the day's first photo |
| US3 | `GET /trips/{id}/map` as owner and with `?code=` | public form has no home end |
| US4 | phone trips page, Share | one PNG of the trip map |
| US5 | rerun organize unchanged | every `sketch_map` item says `up to date` |
