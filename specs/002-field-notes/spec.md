# Feature Specification: Field Notes

**Feature Branch**: `002-field-notes`

**Created**: 2026-10-09

**Status**: Draft

**Input**: User description: "Field notes: editorial map overlays on trip highlights. Inspired by the 'travel zine' idea (a travel photo with a restrained, real map of where it was taken laid over its empty space), but built for Mantelpiece and without an image model." Reference look: the sixteen examples in the original post (Barcelona, Gran Paradiso, Pisa, Budapest, Florence, Turin, Tre Cime, Hemu), reviewed 2026-10-10.

Implements a new requirement, **F-711 Field notes**, to be added to `specs/architecture.md`
section 4.8. Builds on F-206 (route, inferred when GPS is missing), F-207 (route maps),
F-210 (landmarks), F-601 (sharing), F-602 (household isolation) and F-159 (read-only trip
view). Nothing here contradicts the decision log; it adds to it.

A field note is a trip highlight shown with a small, quiet map of where it was taken, laid
into the photo's empty sky, water or ground, the way an independent travel magazine prints
a photograph with a hand-placed map beside the subject. The photo itself is never touched:
the map is a separate layer that the TV can show or hide and the phone can flatten into a
picture to share.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Watch a trip as field notes on the TV (Priority: P1)

The family has come back from Kyoto. The trip has organised itself and picked its
highlights. On the TV, someone opens the trip's slideshow and turns on Field notes with one
button. Each highlight now carries a small map in its quiet part: the river and the streets
around the temple in the evening sky, a red dot where the photo was taken, the day's walk
as a fine line, "Gion" and "Kamo River" in small type. One press turns the maps off again;
the photos underneath are exactly as they were.

**Why this priority**: this is the feature. It turns highlights the family has already seen
into something new without anyone doing any work, and it is the demo moment.

**Independent Test**: organise a trip from a sample archive with GPS, open its slideshow on
the Virtual Device, turn on Field notes, and see maps on the highlights that have quiet
space; turn it off and see the unchanged photos.

**Acceptance Scenarios**:

1. **Given** an organised trip whose highlights carry GPS, **When** the slideshow runs with Field notes on, **Then** every highlight that has a field note shows its map layer over the photo, and the ones without play as plain photos.
2. **Given** Field notes is on, **When** someone presses the toggle, **Then** the layer disappears within one second and the photo is identical to the plain highlight.
3. **Given** Field notes was turned on in one slideshow, **When** the same TV opens another trip's slideshow, **Then** Field notes is still on.
4. **Given** a highlight in Trip view has a field note, **When** it is focused, **Then** a small mark says so and one press shows it as a field note full screen.

---

### User Story 2 - The map tells the truth about where the photo was taken (Priority: P1)

Some photos carry exact GPS; some were taken on a camera without it and were placed by the
trip's inferred route or a recognised landmark; some cannot be placed at all. The field note
shows exactly as much as is known: a small red dot and rounded coordinates only for real
GPS, a soft ring around the likely area and no coordinates for an inferred place, and no
field note at all for a photo whose place is unknown.

**Why this priority**: a confident wrong map in a family memory is worse than no map, the
same rule the narrative follows. Without this the feature is decoration that lies.

**Independent Test**: organise a trip mixing photos with GPS, photos placed only by an
inferred route, and photos with no place; check each highlight's field note for dot, ring
or absence.

**Acceptance Scenarios**:

1. **Given** a highlight with GPS, **When** its field note is made, **Then** it shows a dot at that position and, in the caption, coordinates rounded to two decimal places.
2. **Given** a highlight placed only by the inferred route or a landmark, **When** its field note is made, **Then** it shows a ring covering the uncertain area, no dot and no coordinates, and the caption names the area, not a point.
3. **Given** a highlight with no GPS, no inferred route position and no landmark, **When** field notes are made, **Then** it gets none.
4. **Given** a highlight taken within the household's home area, **When** its field note is made, **Then** it shows no dot and no coordinates, only the area ring, whatever GPS it carries.

---

### User Story 3 - The map never covers what the photo is about (Priority: P1)

A photo of two children on a sea wall has the sea and sky behind them. The map goes into the
sky, not across a face. A photo of a temple roof against the sky puts the map in the sky
beside the roof, not on it. A close-up of a busy market stall has no quiet space, so it gets
no field note rather than a map scrawled over the produce.

**Why this priority**: an overlay over a face or the subject ruins the photo; the whole
point is restraint.

**Independent Test**: run field notes over a test set of highlights where people, faces,
signs and landmark subjects are marked by hand; no map may overlap a marked subject, and the
busy-frame photos get none.

**Acceptance Scenarios**:

1. **Given** a highlight with people, faces, text or a recognised object, **When** its field note is placed, **Then** the map and its labels stay clear of each of them with a margin.
2. **Given** a landscape highlight whose subject is a peak, building or monument, **When** its field note is placed, **Then** the map stays clear of that subject too.
3. **Given** a highlight with several quiet regions, **When** its field note is placed, **Then** the map goes in the calmest one large enough for it.
4. **Given** a highlight with no region quiet enough to read thin lines on, **When** field notes are made, **Then** it gets none.
5. **Given** any highlight, **When** its field note is placed, **Then** the map covers between 15 and 25 percent of the frame and keeps the photo's own aspect ratio and orientation.

---

### User Story 4 - Share a field note from the phone (Priority: P2)

From the trip page on the phone, the owner opens a highlight that has a field note, sees it
composed, and shares or saves it as a single picture at the photo's full resolution, to send
to a grandparent or print.

**Why this priority**: the TV is where the family watches; the phone is how a field note
leaves the house. It needs User Stories 1 to 3 first.

**Independent Test**: on the phone trip page, open a highlight with a field note, share it,
and receive one image at the original's resolution with the map composed in.

**Acceptance Scenarios**:

1. **Given** a highlight with a field note, **When** the owner opens it on the phone, **Then** they see it composed and can switch to the plain photo.
2. **Given** the composed view, **When** the owner shares or saves it, **Then** they get one picture at the original's full resolution with the layer composed in.
3. **Given** a field note shown on a shared link, the read-only trip view or a member household's TV, **When** it is displayed, **Then** it never carries a dot or coordinates, only the area ring and place names.

---

### User Story 5 - Field notes keep up with the trip (Priority: P3)

New photos arrive after the trip was organised, a highlight changes, a photo is removed, or
the household corrects where a day was. Field notes follow: new highlights get theirs,
replaced ones lose theirs, and a field note never shows a place the trip no longer agrees
with.

**Why this priority**: correctness over time matters, but the first viewing works without it.

**Independent Test**: organise a trip, add photos that change the highlights, and see field
notes appear for the new highlights and vanish for the dropped ones.

**Acceptance Scenarios**:

1. **Given** a trip with field notes, **When** a re-organise changes the highlights, **Then** new highlights get field notes and dropped ones no longer show theirs.
2. **Given** a highlight with a field note, **When** the photo is removed, **Then** its field note goes with it, and returns with it if the photo is restored.
3. **Given** a highlight whose inferred place changes on re-organise, **When** field notes update, **Then** the field note shows the new place.

---

### User Story 6 - Open the map from the remote (Priority: P2)

In Field notes mode, someone presses Up on the remote and focus moves onto the map. It
brightens and gains a soft outline, the slideshow pauses, and a short hint says Select
opens it. Select opens "This place": the same map drawn large on its own, the place's facts
when the trip knows the landmark, and a strip of the trip's other photos from the same
place. Back returns to the photo the slideshow was on.

**Why this priority**: it turns the map from decoration into a way into the trip, and the
TV is a lean-back screen where a remote press is the only interaction. It needs User
Story 1.

**Independent Test**: on the Virtual Device, in Field notes mode, press Up until the map is
focused, see it highlighted and the slideshow paused, press Select, see the large map and
nearby photos, press Back and land on the same slide.

**Acceptance Scenarios**:

1. **Given** a highlight with a field note on screen in Field notes mode, **When** the remote moves focus up from the controls, **Then** the map takes focus, is visibly highlighted, and the slideshow pauses.
2. **Given** the map is focused, **When** Select is pressed, **Then** "This place" opens with the map drawn large, the landmark's facts when known, and up to twelve of the trip's photos within the note's ring or 1 km of its dot.
3. **Given** "This place" is open, **When** Back is pressed, **Then** the slideshow returns to the same photo, still paused, with focus on the map.
4. **Given** a slide without a field note, **When** the remote moves up, **Then** focus does not land on an empty area.
5. **Given** a public-form note (a member household's TV), **When** "This place" opens, **Then** the large map also shows only the ring, never a dot or coordinates.

---

### Edge Cases

- A highlight is a video: its field note is laid over the poster frame only, and the TV shows it while the video's poster is on screen, not during playback.
- Portrait, panorama and square photos: placement and size follow the photo's own frame; nothing is cropped or letterboxed to fit the map.
- The photo is mostly sky (a sunset): the map is capped at 25 percent and stays clear of the sun and horizon line if they are marked subjects.
- The quiet region is dark (night sky) or bright (overcast): line and label colour follow the brightness underneath so the map stays readable without becoming loud.
- The place is at sea, in a desert or otherwise has little mapped detail: the field note shows what exists (a coastline, a single road) or none if nothing is drawable.
- Map data for the place cannot be fetched: that highlight gets no field note for now and is retried on the next update, never a blank or broken map.
- Place names exist only in a local script: show the English name when one exists, otherwise leave the label out rather than show an unreadable or transliterated guess.
- The trip spans two countries: each field note is about its own photo's place, not the trip's extent.
- The household turns Field notes on but the trip has no field notes yet (still being made, or none qualified): the TV says so in one line and plays plain photos.
- The household deletes its account: field notes go with the originals and renditions.

## Requirements *(mandatory)*

### Functional Requirements

**Making a field note (F-711)**

- **FR-001**: System MUST make field notes for a trip's highlights after the trip is organised, and for no other photos.
- **FR-002**: System MUST never alter the original photo or its existing renditions; a field note is a separate layer, sized and positioned against the photo's own frame, with the photo's aspect ratio and orientation.
- **FR-003**: System MUST draw only real geography for the photo's place from a map data source: coastlines and water, streets, paths and trails, contour lines, and at most four English place names chosen by importance and distance.
- **FR-004**: System MUST choose the map's character from the geography in view: coast, mountains (contours), town (streets), or trail; and MUST draw the day's route as a fine line when the trip has one for that day, with the photo's position on it.
- **FR-005**: The map MUST use thin lines, a low-saturation palette, a subtle paper-like grain confined to the map layer, and line and label colour chosen for contrast with the region underneath. Water MAY be a soft translucent fill (a river as a pale band), parks a fainter one; in mountain mode, shaded relief from elevation data MAY sit under the contours, faint enough that the photo shows through.
- **FR-005a**: Peaks MUST carry their elevation when the map data has it ("Tre Cime di Lavaredo 2,999 m"); known kinds of place (peak, hut, church, castle, bridge, station) MAY carry a small generic pictogram; no bespoke landmark illustrations.
- **FR-005b**: When the photo records the compass direction it was taken in, the marker MUST show it as a narrow view cone; otherwise no direction is drawn.
- **FR-006**: The map MUST occupy between 15 and 25 percent of the frame and MUST NOT be enclosed in an opaque box or card; its edges fade into the photo.
- **FR-007**: A field note MAY carry a small title block (place and country in spaced capitals, no larger than 4 percent of the frame's height) with a scale bar and a north mark, placed in a calm corner clear of subjects, and one caption line with the date and, where allowed, coordinates. It MUST NOT add borders, frames, banners or other decoration.
- **FR-008**: Making field notes MUST NOT use image-generation models, and MUST make at most one model call per highlight (to find its landscape subjects), never for photos that are not highlights, and none when a highlight's field note is already up to date.

**Placement**

- **FR-009**: System MUST find the photo's main subjects (people, faces, text, recognised objects, and landscape subjects such as a peak, building or monument) and keep the map and its labels clear of each with a margin.
- **FR-010**: Among the regions left, System MUST place the map in the calmest region large enough for it, preferring sky, still water and plain ground.
- **FR-011**: When no region is calm enough for thin lines to read, System MUST make no field note for that photo.

**Honest location**

- **FR-012**: System MUST show a dot and rounded coordinates (two decimal places) only when the photo carries its own GPS.
- **FR-013**: When the place is inferred from the day's route or a landmark (F-206, F-210), System MUST show a ring covering the uncertain area and MUST NOT show a dot or coordinates.
- **FR-014**: When the place is unknown, System MUST make no field note.
- **FR-015**: For photos taken within the household's home area, System MUST show the ring only, never a dot or coordinates.
- **FR-016**: On shared links (F-601), the read-only trip view (F-159) and member households' TVs (F-162), System MUST show field notes with the ring only, never a dot or coordinates.

**Viewing and sharing**

- **FR-017**: The TV MUST offer Field notes as a one-button toggle in the trip slideshow, remembered per TV across trips and restarts.
- **FR-018**: The TV MUST mark highlights that have a field note in Trip view and show one full screen on request.
- **FR-019**: Turning field notes off MUST show the photo exactly as without the feature.
- **FR-020**: The owner and co-editors MUST be able to view a field note on the phone, switch to the plain photo, and share or save the composed picture at the original's full resolution.

- **FR-020a**: In Field notes mode the map MUST be reachable by remote focus, MUST show a visible focused state, and MUST pause the slideshow while focused; slides without a note MUST NOT offer an empty focus target.
- **FR-020b**: Selecting a focused map MUST open "This place": the same drawing rendered large on its own (in the form the viewer is allowed to see), the landmark's facts when the trip has one there, and up to twelve of the trip's photos from the same place; Back MUST return to the same slide.

**Keeping up**

- **FR-021**: System MUST make field notes for new highlights and stop showing them for photos that are no longer highlights, removed, or whose place changed, after each re-organise.
- **FR-022**: A field note MUST follow its photo through soft removal and restore, and MUST be deleted with the household's data (F-604).
- **FR-023**: When map data cannot be fetched, System MUST make no field note for that photo and MUST retry on the next update, never showing a partial map.

**Isolation and data**

- **FR-024**: Field notes MUST respect household isolation (F-602): a member household sees field notes only for trips it joined, and only in the ring-only form.
- **FR-025**: Map data MUST be used under terms that allow keeping the finished field note; if the primary source does not allow it, System MUST use an open-licensed source and credit it where field notes are shown.

### Key Entities

- **Field note**: the map layer for one highlight. Belongs to one photo; knows where it sits in the photo's frame, how large it is, which map character it uses, whether its place is precise or approximate, the caption text, the map data source and its credit, and the version of the photo and trip it was made from.
- **Subject mask**: the parts of a photo that are its main subjects, with a margin; made from what enrichment already found, kept with the photo, used for placement.
- **Place certainty**: precise (the photo's own GPS), approximate (inferred route or landmark, with a radius), or unknown. Decides dot, ring or nothing.
- **Home area**: the region around the household's home, inside which no precise marker is ever shown.
- **Field notes preference**: per TV, on or off.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: With field notes off, every highlight is identical, pixel for pixel, to the same highlight before the feature existed.
- **SC-002**: On a hand-marked test set of at least 100 highlights, no field note overlaps a marked subject.
- **SC-003**: On a sample trip whose highlights carry GPS, at least 60 percent of highlights get a field note, and none of the photos a reviewer marks "too busy" do.
- **SC-004**: No field note shows a dot or coordinates for a photo without its own GPS, for a photo in the home area, or on a shared or member view (zero cases across the test set).
- **SC-005**: Field notes for a trip are ready within 10 minutes of the trip being organised.
- **SC-006**: Turning Field notes on or off on the TV takes effect within one second.
- **SC-007**: A shared field note is a single picture at the original photo's full resolution.
- **SC-008**: In a review of 20 field notes, at least 16 are judged by two family testers as "would keep it" and none as covering the subject.

## Assumptions

- Highlights, the day's route (actual or inferred), recognised landmarks and the enrichment results already exist for an organised trip; field notes add to them and need no new upload step.
- The subject boxes found during enrichment (people, faces, text, objects) are kept from now on at no extra call. Landscape subjects (a peak, a building, a monument) are not boxed by enrichment, so each highlight gets one model call asking for them when its field note is made; a trip has about 24 highlights. Highlights enriched before this feature get their boxes on the next re-enrich.
- The household's home area is where most of its photos outside any trip cluster, with a radius of 2 km; until there are enough such photos, the home rule is skipped and the shared-view rule still hides every precise marker.
- Place names are taken in English from the map data when an English name exists.
- Map data comes from OpenStreetMap (open licence, credit shown) and contours from open terrain data. Amazon Location was considered first; its service terms bar storing map content, so it is not used for field notes (research R1).
- Field notes are made automatically; there is no per-photo editing of map style, position or content in this feature.
- Videos get a field note on their poster frame only.
- Out of scope: altering or regenerating the photo, image-generation models, user-editable map styling, printing services.
