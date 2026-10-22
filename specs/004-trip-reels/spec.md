# Feature Specification: Trip Reels and Day Films

**Feature Branch**: `004-trip-reels`

**Created**: 2026-10-16

**Status**: Implemented; open tasks and unverified items are in `tasks.md`

**Input**: User description: "Do both A and B for the hackathon: (A) a music-synced slideshow on
the TV, planned from the trip; (B) a rendered film of each day, made at the end of the day."

Implements two new requirements, to be added to `specs/architecture.md` section 4.8:
**F-713 Trip reels** and **F-714 Day films**. Builds on F-105 (TV renditions), F-204 (quality
score), F-209 (highlights, pins, hides), F-210 (landmarks), F-708 (Ken Burns motion), F-711
(subject boxes) and F-162 (member households). F-714 amends F-708's "no video encoding"
(decision 39): films are rendered from the TV renditions in the cloud; originals are still
never transcoded (F-105) and the TV still has no editing stack.

A reel is the trip as a music video: the photos in the order they were taken, each held for
whole bars of a music track so the picture changes on the beat, each moving toward what it
is about (the people, the landmark), with the trip's name at the start, the places named as
they come, and the music credited at the end. A day film is the same thing for one day,
rendered to a video file at the end of that day, so the family can watch "today" that
evening, and keep it.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Play the trip as a reel (Priority: P1)

The family opens the Florence trip on the TV and presses Play reel. Music starts; the first
photo, the station hall, slowly pushes in under the title "Florence". On the first beat of
the next bar it cuts to the Duomo, which pans across; then the Galleria tilts up toward its
glass roof. "Ponte Vecchio" appears on the first photo of the bridge. After the sunset over
the river the picture fades to black with the music, and the credit reads "Music: Carefree by
Kevin MacLeod, CC BY 3.0".

**Why this priority**: it is the most visible new thing on the TV and needs nothing rendered.

**Independent Test**: open a trip with at least 3 usable photos; Play reel; every cut after
the first photo lands on a bar line of the track; each photo moves; the title, landmark labels
and credit appear only from the trip's metadata.

**Acceptance Scenarios**:

1. **Given** a trip, **When** Play reel is pressed, **Then** the reel plays the trip's photos
   in capture order over a library track, cutting on its bar lines, ending with a fade and
   the music credit.
2. **Given** a photo with people, **When** its slide plays, **Then** its crop moves toward the
   people and ends with them in frame.
3. **Given** the remote's Play/Pause, **When** pressed, **Then** both the music and the
   picture hold, and resume together.
4. **Given** a TV that cannot play the music, **When** the reel plays, **Then** it plays
   silently on the same timing.

---

### User Story 2 - A film of each day, made that evening (Priority: P1)

At 21:00 in Florence, the TV ticker says "Day 2 film is ready". In Trip view, day 2 has a Day
film button; it plays a 40-second film of the day, cut to music, with the day's places named.

**Why this priority**: the film is the keepsake, and the end-of-day moment is the hook.

**Independent Test**: a trip day in progress gets a schedule for 21:00 local; when it fires,
a film is stored and announced; the TV plays it from Trip view.

**Acceptance Scenarios**:

1. **Given** photos of today arriving, **When** organize runs, **Then** a one-time schedule
   for 21:00 local on that day exists for the day's film.
2. **Given** the schedule fires, **When** the day has at least 3 usable photos, **Then** an
   MP4 film is stored under `films/` and `film.ready` is announced.
3. **Given** more photos of a finished day arrive, **When** organize runs, **Then** the film
   is remade ten minutes later; an unchanged plan is not re-rendered.
4. **Given** an older trip, **When** the owner presses Make day film, **Then** the film is
   made within a few minutes.

---

### User Story 3 - Honest text, licensed music (Priority: P1)

**Why this priority**: constitution IV, and a shared video must carry its music licence.

**Acceptance Scenarios**:

1. **Given** a photo whose landmark is less than 85 % certain, **When** its slide plays,
   **Then** no label is shown.
2. **Given** any reel or film, **When** it ends, **Then** the music credit is on screen.
3. **Given** a photo the viewing household has hidden, **When** its reel plays, **Then** the
   photo is not in it.

### Edge Cases

- Fewer than 3 usable photos: no reel (422) and no film (skipped).
- More photos than the track holds: the best by score are kept (at least one per day), in
  capture order.
- A track whose first bar comes later than 4 s is not used: the music always starts at its
  own 0 (the TV never seeks).
- Videos are shown as their poster frames, like the slideshow (F-105).
- A day that disappears on re-organize: its schedule runs and skips; its film stays until the
  trip goes.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: The system MUST plan a reel for a trip or one day from the trip's media: burst
  keepers in capture order, without removed, hidden (for the viewer), cleanup-flagged or
  unrendered items; at most 30 slides for a day and 48 for a trip.
- **FR-002**: Every cut after the first slide MUST land on a bar line of the track, up to its
  final hit; the first slide lasts at least 3.5 s and the last at least 4 s.
- **FR-003**: Each slide MUST carry two crop windows within the picture (contracts/api.md);
  portraits tilt toward their subjects, panoramas pan, others push in or pull out; no two
  consecutive slides make the same kind of move; content takes at least 4 s to cross the frame.
- **FR-004**: On-screen text MUST come from the metadata: the trip name, "Day N", landmarks
  tagged at 85 % confidence or more (on the first slide of a run, when the slide is long
  enough to read), and the track's credit (constitution IV).
- **FR-005**: The plan MUST be deterministic; the same trip and media give the same plan.
- **FR-006**: `GET /trips/{trip}/reel` MUST serve the plan with the music presigned, to any
  viewer of the trip; nothing is stored.
- **FR-007**: The TV MUST play a reel over its music, with slides following the music's
  clock, preloading the next pictures, Pause holding both, and a silent fallback.
- **FR-008**: The system MUST render a day film from the day's plan (1920 x 1080, 30 fps,
  H.264 and AAC, -14 LUFS, true peak at most -1 dBTP) and store it under `films/`.
- **FR-009**: After organize, each day of a touched trip no more than two days old MUST have
  a one-time schedule at 21:00 local (the day's most common photo offset), or ten minutes
  after now for a finished day.
- **FR-010**: A film MUST NOT be re-rendered when its plan's fingerprint is unchanged.
- **FR-011**: `film.ready` MUST be announced to the household and its member households.
- **FR-012**: The owner or a co-editor MUST be able to queue a day film from the TV.
- **FR-013**: Account deletion MUST delete every film.
- **FR-014**: Music MUST be openly licensed for published video (CC BY or CC0) and credited
  on screen.

### Key Entities

- **Reel**: the plan (contracts/api.md): title, track, slides with crop windows and labels,
  credit. Computed, never stored.
- **Day film**: an MP4 and a marker (fingerprint, duration, time made) under
  `films/{h}/{trip}/`.
- **Track**: a library entry: audio key, licence, credit, beat grid (bars, final hit).
- **Film schedule**: a one-time EventBridge Scheduler schedule per day, named `film-…`.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: 100 % of cuts after the first slide fall on a track bar line (planner test).
- **SC-002**: A 30-second day film renders in under 3 minutes on the film Lambda.
- **SC-003**: Films measure -14 ± 1 LUFS with true peak at most -1 dBTP.
- **SC-004**: A reel starts on the TV within 3 seconds of Play reel on a warm connection.
- **SC-005**: No word on screen in any reel or film is absent from the trip's metadata or the
  track's credit.

## Assumptions

- The Vega `AudioPlayer` and `VideoPlayer` from `@amazon-devices/react-native-w3cmedia` play
  an `.m4a` and an `.mp4` from a presigned URL (unverified; the live player already plays
  HLS through the same module).
- Three CC BY 3.0 tracks by Kevin MacLeod from Wikimedia Commons make the library; two have a
  first bar early enough to be used.
- 21:00 local is "the end of the day"; later photos remake the film.
- Out of scope: choosing the music, editing a reel, video clips inside reels and films,
  narration under the music, sharing films from the phone.
