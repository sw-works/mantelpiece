# Feature Specification: Trip Sketch Maps

**Feature Branch**: `003-trip-sketch-maps`

**Created**: 2026-10-10

**Status**: Draft

**Input**: User description: "Trip sketch maps (F-712): hand-drawn style maps for a whole multi-day trip and for each day, replacing the F-207 static route map. Built in code from real geography, no image model."

Implements a new requirement, **F-712 Trip sketch maps**, to be added to `specs/architecture.md`
section 4.8, and replaces how F-207 (route rendered as a static map image per trip and per
day) is met. Builds on F-206 (route, inferred when GPS is missing), F-210 (landmarks),
F-711 (field notes: the map data, honesty rules and home area it already uses), F-601
(sharing), F-602 (household isolation) and F-159 (read-only trip view). It also settles the
open question in decision 36: the current route maps come from a provider whose terms do
not allow keeping them, and these maps are drawn from data that may be kept.

A sketch map is the map a traveller would draw in the back of a journal: the coastline in
wobbly ink, the sea and lakes washed in watercolour, hills hatched in pencil, the route as a
dashed line with the day numbers circled, a little drawing of a waterfall where the family
stood at one, and the trip's name in a hand-lettered title box. Every line and name on it is
real; only the hand is pretend.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - See the whole trip as a hand-drawn map on the TV (Priority: P1)

The family is back from eight days in Iceland. They open the trip on the TV. Where the plain
route map used to be, there is a drawing of the island: the coast in ink, the ring road, the
glaciers washed pale blue, a dashed line going round with circled numbers 1 to 8 where each
day ended, and small sketches at Dettifoss, Diamond Beach and Skógafoss. The title box says
"Iceland, June 2015" with the dates.

**Why this priority**: the trip map is the single picture of the whole journey and the most
visible replacement for the route map; it is what makes the feature worth having.

**Independent Test**: organise a multi-day trip with GPS photos; open Trip view on the TV;
the trip map shows the trip's whole extent, every day's leg, a circled number per day, the
named landmarks as pictograms and the title box; no line or name is absent from the map
data or the trip's landmarks.

**Acceptance Scenarios**:

1. **Given** an organised trip of several days with GPS photos, **When** Trip view opens,
   **Then** the trip map fills the place of the old route map and shows every day's leg
   with that day's number circled at the leg's end.
2. **Given** the trip's named landmarks, **When** the trip map is drawn, **Then** up to
   twelve of them appear as small pictograms of their kind with their names in hand
   lettering, and none overlap.
3. **Given** the same trip opened twice, **When** the map is shown, **Then** it is the same
   picture both times (the hand wobble does not change between draws).

---

### User Story 2 - Each day opens with its own map (Priority: P1)

In Trip view, choosing day 3 shows that day's map: zoomed to the day's drive, the road from
Mývatn to Dettifoss in dashed ink, small dots where photos were taken, Dettifoss and Lake
Mývatn named. In the slideshow, each new day starts with that day's map for a few seconds
before its first photo.

**Why this priority**: days are how the family remembers a trip; a map per day gives the
slideshow a chaptered rhythm and answers "where were we that day?".

**Independent Test**: open a day in Trip view and play the slideshow across a day boundary;
the day map shows that day's route, photo spots and landmarks only, and the slideshow shows
it for a few seconds before the day's first photo.

**Acceptance Scenarios**:

1. **Given** a day with a route, **When** that day is chosen in Trip view, **Then** its day
   map shows the day's route, a dot for each photo spot and the day's named landmarks.
2. **Given** the slideshow reaching a new day, **When** the day changes, **Then** the day
   map shows for about four seconds, then the day's first photo; Pause holds it.
3. **Given** a day with no place at all (no GPS, no estimated route, no landmark), **When**
   the day is shown, **Then** there is no day map and nothing stands in for one.

---

### User Story 3 - Honest about what is known (Priority: P1)

Some of day 5's photos had no GPS; the route between them was estimated from landmarks and
scene order. The family's own home is in Reykjavík, where the trip started and ended.

**Why this priority**: the maps must never claim more than the trip knows, and never show
where the family lives to people outside it. This is the same promise field notes make.

**Independent Test**: a trip mixing GPS and estimated days, starting and ending near the
household's home; estimated legs are visibly fainter; the home end of the route is drawn
only as a soft area for the owner and not at all on shared or member views.

**Acceptance Scenarios**:

1. **Given** a leg from an estimated route, **When** it is drawn, **Then** it is visibly
   fainter and more broken than a GPS leg, and no photo dot is drawn for estimated points.
2. **Given** route points within two kilometres of the household's home, **When** any map
   is drawn, **Then** those points are not drawn precisely: the route stops at the edge of
   a soft area for the owner household, and is cut there with no area at all on shared and
   member-household views.
3. **Given** a map with nothing to draw from (the map data cannot be fetched), **When** the
   map would be made, **Then** no map is stored and the screen falls back to the trip's
   pictures with no map, never to an invented drawing.

---

### User Story 4 - Share the trip map from the phone (Priority: P2)

On the phone's trip page, the trip map sits at the top. Share hands it to the phone's share
sheet as a picture.

**Why this priority**: the map is a keepsake worth sending to the family chat, but the TV
experience stands without it.

**Independent Test**: open the trip on the phone as the owner; the trip map shows with its
credit; Share produces one picture of the map at full size.

**Acceptance Scenarios**:

1. **Given** the owner's trip page, **When** it opens, **Then** the trip map shows at the
   top with the map-data credit beneath it.
2. **Given** Share, **When** pressed, **Then** the share sheet receives one picture of the
   trip map (download where sharing files is not available).
3. **Given** a read-only shared link, **When** it opens, **Then** it shows the public form of
   the trip map (home end cut) and no Share of a private form.

---

### User Story 5 - Maps follow the trip as it changes (Priority: P2)

A second phone adds photos from day 6; organize runs again and day 6 gains a stop.

**Why this priority**: a trip keeps changing for days after the family gets home; stale maps
would contradict the photos.

**Independent Test**: change a day's photos so its route changes; after organize, that day's
map and the trip map are redrawn and the other days' maps are not.

**Acceptance Scenarios**:

1. **Given** a day whose route or landmarks changed, **When** organize finishes, **Then** that
   day's map and the trip map are redrawn; unchanged days keep theirs.
2. **Given** a day that disappears (its photos removed), **When** organize finishes, **Then**
   its map is deleted.

### Edge Cases

- A one-day trip: the trip map and the day map show the same ground; only the day map is
  drawn and the trip map uses it.
- A trip spanning a sea crossing or a flight (a leg longer than 300 km between consecutive
  points): the long hop is drawn as a thin dotted arc, not along roads, and does not force
  the map to the scale of a continent when it is only the journey out; when the hop is the
  first or last leg from home, it is cut by the home rule.
- A trip whose points span more than 2,000 km (a tour of several countries): the trip map is
  drawn at the scale that fits, with only coast, borders and the largest places.
- A landmark whose kind is unknown: a plain star pictogram.
- More than twelve landmarks: the most prominent twelve, spread out, are drawn; the rest are
  only on the day maps.
- Names in a script the hand lettering lacks: the English or Latin name from the map data,
  else the name is left off rather than drawn as boxes.
- A day whose route is a single point: a small circle with the day's place name, no route.
- Member households (joined trips): the public form, as for field notes.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: The system MUST draw one trip map per organised trip and one day map per trip
  day that has at least one known place (GPS, estimated route point or landmark).
- **FR-002**: Every coastline, water body, glacier, river, road, peak and place name on a map
  MUST come from the stored map data or the trip's own landmarks; nothing may be invented.
- **FR-003**: The trip map MUST frame the extent of all of the trip's known places with a
  margin, and show each day's leg as a dashed line with the day number circled at the
  leg's last point.
- **FR-004**: The day map MUST frame the day's route and photo spots with a margin (at least
  2 km across), show the route as a dashed line, a small dot per GPS photo spot, and the
  day's named landmarks.
- **FR-005**: Maps MUST look hand-drawn: lines carry a gentle, smooth irregularity and a
  second slightly offset pass; sea, lakes and glaciers carry a soft watercolour wash with
  uneven edges; hills carry light pencil hatching where the ground rises; the whole sheet
  carries a paper grain; names are in a handwritten typeface.
- **FR-006**: The irregularity MUST be repeatable: drawing the same trip with the same data
  twice yields the same picture.
- **FR-007**: The trip map MUST carry a title box with the trip's name and dates, a small
  compass rose and a hand-drawn scale bar in metric units.
- **FR-008**: The trip map MUST show at most twelve of the trip's landmarks as small
  pictograms by kind (waterfall, lighthouse, glacier, church, peak, hot spring, castle,
  beach, lake, town; a star otherwise) with their names, chosen by prominence and spread,
  with no two labels overlapping.
- **FR-009**: Legs and points from an estimated route MUST be drawn fainter and more broken
  than GPS legs; estimated points get no photo dot.
- **FR-010**: Route points within 2 km of the household's home area MUST NOT be drawn
  precisely: the owner's form ends the route at a soft area; shared and member-household
  forms cut the route there with no area. The home area is the one field notes use.
- **FR-011**: Legs longer than 300 km between consecutive points MUST be drawn as a thin
  dotted arc and MUST NOT set the trip map's frame unless no other leg exists.
- **FR-012**: Each map MUST carry the map-data credit ("© OpenStreetMap contributors", plus
  the terrain source when hatching is drawn) on the picture or directly beneath it on every
  surface.
- **FR-013**: Maps MUST be made after organize, alongside field notes, for every trip and day
  whose fingerprint (route, landmarks, home area, style version) changed, and MUST be
  deleted when their day or trip goes.
- **FR-014**: When the map data cannot be fetched, no map is stored for that trip or day and
  the run is retried later; nothing invented stands in.
- **FR-015**: The TV Trip view MUST show the trip map where the route map was, and the chosen
  day's map in that day's section.
- **FR-016**: The TV slideshow MUST open each new day with its day map for about four seconds
  before the day's first photo, respecting Pause; a day without a map opens on its first
  photo.
- **FR-017**: The phone's trip page MUST show the trip map with its credit, and the owner and
  co-editors MUST be able to share it as a picture (download as a fallback).
- **FR-018**: Shared links, read-only views and member households MUST receive the public
  form of each map (FR-010); the private form MUST NOT be reachable from those views.
- **FR-019**: Maps MUST be drawn without an image-generation model and without a per-map paid
  call; map data and terrain come from openly licensed sources that permit storing derived
  pictures.
- **FR-020**: Account deletion and trip removal MUST delete the maps with the trip.

### Key Entities

- **Sketch map**: a picture of a trip or of one day, in two forms (owner, public), with the
  fingerprint it was drawn from, its frame (the ground it covers), its credit and when it
  was made.
- **Trip** (existing): gains a reference to its trip sketch map.
- **Trip day** (existing): gains a reference to its day sketch map.
- **Home area** (existing, from field notes): the household's home point and its 2 km radius.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: On the Iceland test album, every organised day with a known place gets a day map
  and the trip gets a trip map within the same organize run as field notes.
- **SC-002**: A reviewer comparing each map with a plain reference map of the same ground finds
  no invented coastline, road or name on any map of the test trip.
- **SC-003**: Drawing the same trip twice gives pixel-identical pictures.
- **SC-004**: No shared or member view of a test trip that starts at home shows any route
  point within 2 km of the home point.
- **SC-005**: A trip map opens on the TV within 2 seconds of Trip view opening on a warm
  connection, and the slideshow's day-map pause adds no more than 4 seconds per day.
- **SC-006**: Four of five people shown the trip map describe it as "hand-drawn" or
  "sketched" rather than "a map screenshot".
- **SC-007**: Making the maps costs no model calls and no per-map provider charges.

## Assumptions

- Map data, terrain and the home area are the ones field notes already use (decision 36):
  OpenStreetMap through OpenFreeMap vector tiles and AWS Open Data terrain tiles; both allow
  keeping drawn pictures with credit.
- Zoomed-out tiles (for a whole-country trip) are served by the same source; this is to be
  confirmed in research before planning commits to it.
- The trip already knows each day's route (GPS or estimated, F-206) and named landmarks with
  their kinds (F-210); no new recognition is added.
- "About four seconds" for the day map is a default; the slideshow interval elsewhere is six.
- A handwritten typeface under the SIL Open Font License is bundled, as Instrument Serif is
  for field notes.
- The old F-207 route maps stop being made once sketch maps ship; stored ones are left until
  their trip is next organised, then replaced.
- Out of scope: interactive or zoomable maps, user-chosen styles, editing the map, printing,
  image-generation models.
