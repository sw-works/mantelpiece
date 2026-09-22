# Feature Specification: Mantelpiece Baseline

**Feature Branch**: `001-mantelpiece-baseline`

**Created**: 2026-09-09

**Status**: Draft

**Input**: User description: "all the features"

This is the product baseline: every requirement in `specs/architecture.md` sections 4.1 to
4.8 (F-101 to F-710), restated as user journeys and testable requirements. Each requirement
cites the F-number it implements. Later feature specs slice this baseline; nothing here
contradicts the decision log of 2026-09-09.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Sign in the TV from the phone (Priority: P1)

A household installs Mantelpiece on their Fire TV. The first screen shows a short code and a
QR. Someone scans it with their phone, signs in with their Amazon account, approves the TV,
and the TV is signed in. Nothing is typed on the remote.

**Why this priority**: every other journey needs a signed-in TV. It is also the first 20
seconds of the demo.

**Independent Test**: install the app on a Vega Virtual Device, scan the QR with a phone,
approve, and see the TV move to the Memories home. Delivers a working household with no
photos yet.

**Acceptance Scenarios**:

1. **Given** a fresh install, **When** the app launches, **Then** it shows an 8-character code, a QR, and the link the QR opens, and starts waiting for approval (F-401).
2. **Given** the code is shown, **When** the phone opens the link, signs in with Amazon, and approves, **Then** the TV shows the Memories home within 5 seconds and stays signed in across a restart.
3. **Given** the code is shown, **When** ten minutes pass with no approval, **Then** the TV shows that the code expired and offers a new one.
4. **Given** the phone approved the TV, **When** the approval page finishes, **Then** the phone lands on the upload page for that household without signing in again.

---

### User Story 2 - Add photos from the phone and watch them arrive (Priority: P1)

From the upload page, a household member picks photos and videos from the phone's camera
roll or cloud picker. Uploads resume if the phone loses signal. The TV shows progress and
each photo as it lands.

**Why this priority**: photos are the product. Without this journey nothing else has data.

**Independent Test**: upload 50 mixed JPG, HEIC and MOV files from a phone and see progress
on both screens and thumbnails on the TV within a minute of the last file.

**Acceptance Scenarios**:

1. **Given** the upload page, **When** the user picks files, **Then** JPG, HEIC, PNG, MP4 and MOV up to 2 GB each are accepted and queued (F-101).
2. **Given** an upload in progress, **When** the phone loses connectivity for a minute, **Then** the queue resumes where it stopped when the phone reconnects, with no file restarted from zero (F-161).
3. **Given** a file identical to one already in the household, **When** it is uploaded, **Then** it is rejected as a duplicate before any processing and the page says so (F-103).
4. **Given** files arriving, **When** the user looks at the TV, **Then** it shows "Uploading N of M" live and a thumbnail for each photo once processed (F-102, F-105, F-107).
5. **Given** an interrupted batch, **When** the same batch is re-run, **Then** the result is identical and no duplicate records exist (F-106).

---

### User Story 3 - Everyone on the trip adds their photos by one QR (Priority: P1)

Inside a trip, the owner presses Invite and the TV shows a QR scoped to that trip. Everyone
in the room scans it, signs in with Amazon or gives just a name, and their photos land in the
same trip, credited to them. The TV ticks up as they arrive.

**Why this priority**: the headline feature. It is what makes the product different from a
personal photo app.

**Independent Test**: two phones scan the same trip QR, one as an Amazon account, one as a
guest named Maya; both upload; the TV shows both contributors' names on their photos and the
ticker announces each arrival.

**Acceptance Scenarios**:

1. **Given** a trip is open, **When** the owner chooses Invite, **Then** the TV shows a full-screen QR and code for that trip with the live contributor count (F-151, F-409).
2. **Given** the trip QR, **When** a guest joins with only a display name, **Then** their uploads are accepted and credited to that name, and their session works only for that trip and that phone (F-152).
3. **Given** contributions from three phones, **When** the trip is viewed, **Then** every photo shows who took it and the timeline is ordered by capture time, not upload time (F-154, F-155).
4. **Given** a contributor's phone clock was 3 minutes off, **When** their photos overlap others' GPS, **Then** the offset is corrected and shown as a hint (F-155).
5. **Given** five phones shot the same landmark, **When** organised, **Then** they form one group with a keeper and every contributor credited (F-156).
6. **Given** a contributor's own uploads, **When** they remove one from the phone page, **Then** it disappears from the trip, thumbnails, search and albums (F-157).
7. **Given** the owner expires the code, **When** someone scans the old QR, **Then** uploads are refused while existing contributions stay (F-153).
8. **Given** a contribution lands, **When** the TV is on any screen, **Then** the ticker shows "Maya added 42 photos from day 3" within 30 seconds (F-160).

---

### User Story 4 - The trip organises itself (Priority: P1)

After a batch lands, the household sees a trip with days, a route, highlights and a
day-by-day story that names who took what. No one tags, sorts or writes anything.

**Why this priority**: the promise in the tagline, "an album that builds itself".

**Independent Test**: upload a 400-photo, 7-day archive with GPS; see one trip with seven
days, a route map, 24 highlights and a narrative that names only places and dates present in
the photos.

**Acceptance Scenarios**:

1. **Given** a batch with bursts, **When** organised, **Then** each burst is one group with a keeper chosen by sharpness and exposure (F-201).
2. **Given** processed photos, **When** viewed, **Then** each has scene tags with confidence, and non-keepers carry their keeper's tags (F-202).
3. **Given** photos of a landmark, **When** organised, **Then** the landmark is named with a confidence, and low-confidence cases are escalated for a second opinion (F-203).
4. **Given** processed photos, **When** scored, **Then** each has a 0 to 10 interest score from faces, smiles, landmark, sharpness and uniqueness (F-204).
5. **Given** photos spanning several days and places, **When** clustered, **Then** multi-day clusters become trips and are named automatically (F-205, F-208).
6. **Given** a trip with GPS, **When** viewed, **Then** a route is drawn per trip and per day; without GPS it is inferred from landmarks and marked inferred (F-206, F-207).
7. **Given** a trip, **When** viewed, **Then** the top N photos are highlights and the user can pin or hide any of them (F-209).
8. **Given** a finished trip, **When** the story is written, **Then** it contains no place or date absent from the photo metadata, credits contributors where their photos anchor a moment, and shows its model and time; a story that fails the check is withheld (F-301, F-302).
9. **Given** a new contributor joins a finished trip, **When** their photos land, **Then** clocks are re-aligned, duplicates regrouped across contributors, days re-clustered and the story refreshed with their credit (W6).

---

### User Story 5 - Ask about the archive (Priority: P2)

From the TV or the phone, someone asks in plain words: "beach days with the kids" or, with
the slideshow paused, "what's that statue in the background". The answer is a playlist that
plays on the TV or a card drawn over the paused photo.

**Why this priority**: the multi-modal, AI-enhanced viewing story for judging, after the
core album exists.

**Independent Test**: type "the ramen place on day 1" on the TV and get a playlist; hold the
phone's mic on a paused Nara photo and ask about the statue; get a card naming it.

**Acceptance Scenarios**:

1. **Given** a household with trips, **When** a natural-language query is entered, **Then** matching photos are returned combined with scene, trip, date and people filters, and within a trip the answer appears in under 300 ms once warm (F-303).
2. **Given** a query with results, **When** confirmed, **Then** a playlist is created and plays as a slideshow on the TV (F-304).
3. **Given** the slideshow is paused, **When** someone asks from the phone or Alexa+ about "that" photo, **Then** the answer is about the photo on screen and is shown as a card on the TV and read back on the asking device (F-405, F-505).
4. **Given** a query with no matches, **When** asked, **Then** the TV says nothing was found and no empty playlist is created.
5. **Given** the TV, **When** the Ask screen is used, **Then** text entry is the input; voice is never captured by the TV itself (decision 2026-09-09).

---

### User Story 6 - Nothing irreversible without the remote (Priority: P2)

An assistant notices duplicates across two phones, or screenshots and receipts in a trip,
and proposes removing them. The TV shows before-and-after counts and samples. Nothing is
removed until someone confirms on the remote.

**Why this priority**: the trust boundary the whole agent story depends on, and the
required gate for every later cleanup feature.

**Independent Test**: upload the same 20 photos from two phones; see a dedup proposal on the
TV; cancel and see nothing removed; confirm and see 20 removed and recoverable for 30 days.

**Acceptance Scenarios**:

1. **Given** near-duplicates across contributors, **When** the proposal arrives, **Then** the TV shows "N before, M after" and sample thumbnails before anything happens (F-410).
2. **Given** the proposal, **When** the user chooses Keep everything, **Then** nothing changes and the proposal is closed.
3. **Given** the proposal, **When** the user confirms on the remote, **Then** exactly the proposed photos are removed, and any attempt to apply without that confirmation is refused (F-504).
4. **Given** screenshots, documents, receipts and blurry shots in a trip, **When** cleanup runs, **Then** they are proposed as one batch through the same review, never removed silently (F-705).
5. **Given** a removal, **When** 30 days have not passed, **Then** the photos can be restored.

---

### User Story 7 - Join a trip from another TV (Priority: P2)

Grandma's Fire TV runs Mantelpiece in her own household. She enters the trip code from the
family's Invite screen and the Kyoto trip appears on her TV the evening it happens, with
everyone's photos, the story and the route. Her reactions show up on the family's TV.

**Why this priority**: the second headline (F-162), and the family-entertainment demo beat.

**Independent Test**: two households on two Virtual Devices; the second joins by code and
sees the trip live; a reaction from the second appears on the first's ticker.

**Acceptance Scenarios**:

1. **Given** a signed-in TV, **When** Join a trip is chosen and a valid code entered on the remote or scanned from the phone, **Then** the trip appears in that household's Memories with a "shared by" badge and a Leave action (F-162, F-411).
2. **Given** a joined trip, **When** the owner's household adds photos, **Then** the member household sees them live; the trip is never copied, only referenced (F-162).
3. **Given** a joined trip, **When** the member household pins or hides a highlight, **Then** only their TV changes (F-163).
4. **Given** a contributor with an Amazon account, **When** the owner promotes them to co-editor, **Then** they can rename the trip, choose keepers, remove any photo and run cleanup on that trip, each through their own on-remote confirmation; removing another contributor's photo notifies that contributor and stays reversible for the grace period (F-164).
5. **Given** a guest or the phone's read-only view, **When** any edit is attempted, **Then** it is refused (F-165).
6. **Given** a member household, **When** they look for other trips, **Then** they see only trips they joined (F-602).

---

### User Story 8 - Memories that resurface (Priority: P3)

The Memories home shows what happened on this day in earlier years, pairs a place then and
now, and offers best-of playlists. A map shows everywhere the household has been.

**Why this priority**: consumption features on data the pipeline already produces; high
delight per effort, after the core is stable.

**Independent Test**: load a multi-year archive; on a date with history the home shows an
On this day row; a place visited twice shows a Then and now card; the map shows every trip.

**Acceptance Scenarios**:

1. **Given** photos from the same calendar day in earlier years, **When** the home opens, **Then** an On this day row appears and a nightly playlist exists for it (F-701).
2. **Given** photos of the same place years apart, **When** the home opens, **Then** a Then and now card pairs them (F-702).
3. **Given** a month or year of photos, **When** requested, **Then** best-of playlists are produced from the interest score (F-703).
4. **Given** trips with GPS, **When** the map view opens, **Then** every trip is pinned on one map (F-707).
5. **Given** any slideshow, **When** it plays, **Then** each still scales from 1.0 to 1.08 with a slow pan over the slide's duration and holds still while paused, without any video encoding (F-708).

---

### User Story 9 - People, opt in (Priority: P3)

A household turns People on. Faces are grouped, named from the phone, and become a search
filter. Nothing leaves the household's archive and everything is deleted with the account.

**Why this priority**: computer-vision credit and better search, but privacy-sensitive and
not needed for the core demo.

**Independent Test**: turn People on, name two clusters from the phone, search "Maya at the
beach" and get only her photos; delete the account and confirm face data is gone.

**Acceptance Scenarios**:

1. **Given** People is off (default), **When** the household turns it on, **Then** faces are grouped and shown as unnamed clusters on the TV (F-704).
2. **Given** clusters, **When** named from the phone page, **Then** the name appears on the TV and is available as a search and playlist filter.
3. **Given** People is on, **When** the account is deleted, **Then** face data is removed with everything else (F-604).
4. **Given** pets in photos, **When** searched, **Then** "the dog" works by label only; no pet identity is claimed.

---

### User Story 10 - Reactions and comments from contributors (Priority: P3)

Contributors and member households open the read-only trip view on their phones, like a
day or a photo, leave a short comment, and it shows on the family's TV ticker and beside the
photo.

**Why this priority**: closes the loop for people who are not in the room.

**Independent Test**: a contributor likes day 3 and comments "That ramen!"; the TV ticker
shows "Grandma Ruth loved day 3" and the comment appears beside the photo in the trip view.

**Acceptance Scenarios**:

1. **Given** the read-only trip link, **When** a contributor opens it, **Then** they see the narrative, highlights and route, gated by the trip code (F-159).
2. **Given** the read-only view, **When** they react or comment, **Then** it appears on the owning TV within 30 seconds and beside the photo in the trip view (F-710).
3. **Given** the read-only view, **When** they try to edit anything, **Then** there is no edit affordance and no edit is accepted (F-165).

---

### User Story 11 - Ambient and narrated slideshow (Priority: P3)

When idle, the TV plays a slideshow adapted to what the household pins and hides. The story
can be read aloud as the photos play.

**Why this priority**: living-room value between sessions; narration depends on an
unverified audio path.

**Independent Test**: leave the TV idle; the ambient set favours pinned photos and skips
hidden ones; with narration on, the day's story is read as its photos show.

**Acceptance Scenarios**:

1. **Given** the TV is idle, **When** ambient starts, **Then** the set reflects household preferences from pins and hides (F-406, W3).
2. **Given** narration is on, **When** a day's photos play, **Then** its story is read aloud in sync (F-709).

---

### User Story 12 - Share outside the household (Priority: P3)

The owner shares an album or playlist with someone outside the household by an expiring
link, or, when the recipient has a TV, by a join code. Sharing needs the same on-remote
confirmation as deletion.

**Why this priority**: useful, irreversible, and last.

**Independent Test**: share "Kyoto highlights" to an email address; the link opens the
album; after expiry it does not.

**Acceptance Scenarios**:

1. **Given** a playlist, **When** shared, **Then** the TV asks for confirmation first, then produces an expiring link and optionally emails it (F-601, F-504).
2. **Given** a recipient with a Mantelpiece TV, **When** sharing, **Then** a join code is offered before a link (W5).
3. **Given** an expired link, **When** opened, **Then** it shows nothing.

---

### Edge Cases

- A phone scans a trip QR after the code was rotated: uploads refused, message explains, existing contributions untouched.
- Two people approve the same TV code: the first approval wins, the second sees "already signed in".
- A 5,000-photo archive in one batch: processes without hitting orchestration limits, with a per-household concurrency cap; progress stays visible throughout (F-108).
- A photo with no timestamp and no GPS: placed by upload time at the end of the nearest trip, flagged as undated.
- All photos in a burst are blurry: the least blurry is the keeper; the score reflects it.
- Story check fails after one retry: the trip shows "The story of this trip is being written" and no story (F-301).
- A contributor removes photos that were highlights or the trip hero: highlights are recomputed, a new hero chosen.
- Owner removes a contributor: their originals are detached, then deleted after a grace period or exported to them by link (F-158).
- Member household leaves a trip: membership record removed, nothing deleted from the owner.
- The same household joins its own trip by code: refused with an explanation.
- Alexa+ asks about "that photo" while nothing is showing: the answer says no photo is on screen and offers a search instead.
- Two TVs in one household: the now-showing record is per TV; Ask from a phone targets the most recently active TV.
- Account deletion while uploads are in flight: uploads are refused, then everything is removed (F-604).

## Requirements *(mandatory)*

### Functional Requirements

Ingest (F-101 to F-108)

- **FR-001**: The upload page MUST accept JPG, HEIC, PNG, MP4 and MOV up to 2 GB per file, resumable across disconnects (F-101, F-161).
- **FR-002**: Upload progress MUST be visible per file and per batch on the phone and on the TV (F-102).
- **FR-003**: Exact duplicates MUST be rejected on arrival, before any processing (F-103).
- **FR-004**: Each photo MUST carry timestamp, GPS, camera and orientation where present, with timestamps normalised and the original zone kept (F-104).
- **FR-005**: Each photo MUST get a thumbnail and a TV-resolution rendition; each video a poster frame; HEIC MUST be converted server-side so the TV only ever receives JPEG (F-105).
- **FR-006**: Batches MUST be idempotent and re-runnable, and MUST process any size within a per-household concurrency cap (F-106, F-108).
- **FR-007**: Perceptual hash, sharpness and exposure MUST be computed and bursts pruned to keepers before any model is called (F-107).

Shared trip contribution (F-151 to F-165)

- **FR-010**: The TV MUST show a household QR and, inside a trip, a trip-scoped QR, both opening the same upload page with a code (F-151).
- **FR-011**: Contributors MUST be able to join with an Amazon account or as a named guest bound to the code and device (F-152).
- **FR-012**: Owners MUST be able to rotate or expire a trip code; expired codes stop new uploads and keep existing contributions (F-153).
- **FR-013**: Every photo MUST carry contributor identity, shown on the TV, in the story and in highlights (F-154).
- **FR-014**: Timelines MUST merge by capture time with per-contributor clock offsets corrected from overlapping GPS and shown as a hint (F-155).
- **FR-015**: Near-duplicate grouping MUST run across contributors with all contributors credited on the group (F-156).
- **FR-016**: Contributors MUST be able to remove their own uploads with cascading removal (F-157); owners MUST be able to remove a contributor with a grace period or export (F-158).
- **FR-017**: Contributors MUST get a web view of the finished trip by the same link (F-159), with reactions and comments that reach the TV (F-710); its no-edit rule is FR-020.
- **FR-018**: Contribution activity MUST stream to the TV live (F-160).
- **FR-019**: Any other signed-in TV MUST be able to join a trip by its code, seeing it live without a copy, with per-household pin and hide (F-162, F-163, F-411).
- **FR-020**: Owners MUST be able to promote a contributor with an Amazon account to co-editor with the edit rights in F-164; guests and the web view MUST never edit (F-165).

Organisation (F-201 to F-209)

- **FR-030**: Bursts MUST be grouped with a keeper chosen by sharpness and exposure (F-201).
- **FR-031**: Photos MUST have scene tags with confidence; non-keepers inherit the keeper's (F-202).
- **FR-032**: Landmarks MUST be detected with confidence and low-confidence cases escalated (F-203).
- **FR-033**: Photos MUST have a 0 to 10 interest score (F-204).
- **FR-034**: Events MUST cluster by time gap and proximity; multi-day clusters are trips; trips and events are named automatically (F-205, F-208).
- **FR-035**: Routes MUST be built from GPS or inferred and flagged, and rendered per trip and per day (F-206, F-207).
- **FR-036**: Highlights MUST be the top N per trip with pin and hide (F-209).

Narrative and search (F-301 to F-304)

- **FR-040**: Stories MUST be generated only from structured metadata, checked against it before saving, withheld on failure, and stamped with model and time; regenerate on demand (F-301, F-302).
- **FR-041**: Natural-language search MUST combine with scene, trip, date and people filters and answer within-trip queries in under 300 ms warm (F-303).
- **FR-042**: Any query MUST be turnable into a playlist playable on the TV (F-304).

Fire TV experience (F-401 to F-411)

- **FR-050**: First launch MUST sign in by device code approved from the phone; no typing on the remote (F-401).
- **FR-051**: The Memories home MUST show trips as rows with a hero and a new-memory badge (F-402); the trip view a day timeline, route, story and slideshow (F-403); highlights with D-pad pin and hide (F-404).
- **FR-052**: Ask MUST take text on the TV; voice MUST arrive via Alexa+ or the phone page; answers are cards or playlists (F-405).
- **FR-053**: An idle TV MUST play an ambient slideshow adapted to household preferences (F-406).
- **FR-054**: Live status MUST work over plain polling with push optional (F-407).
- **FR-055**: The Review Changes modal MUST precede every destructive action with counts and samples; nothing applies without on-remote confirmation (F-410).
- **FR-056**: The app MUST run on Vega OS and pass the Vega Virtual Device; Fire OS is a stretch (F-408).

Agents and Alexa+ (F-501 to F-506)

- **FR-060**: All read and write capabilities MUST be exposed as tools through one governed endpoint (F-501), with per-household long-term memory (F-502) and event-driven workflows (F-503).
- **FR-061**: Irreversible tools MUST require the TV-minted confirmation and a policy check; agents cannot self-confirm (F-504).
- **FR-062**: Alexa+ MUST be able to call the same tools with voice and Echo Show answers (F-505); every agent run MUST be traceable end to end (F-506).

Sharing, security, operations (F-601 to F-606)

- **FR-070**: Albums and playlists MUST be shareable by expiring link with optional email, behind the confirmation gate (F-601).
- **FR-071**: Household data MUST be isolated across storage, index, memory and tools; contributors and member households see only trips they joined (F-602).
- **FR-072**: Data MUST be encrypted at rest and in transit with no model training on user content (F-603); account deletion MUST remove everything including face data (F-604); auth, share, delete and agent tool calls MUST be audited (F-605); the whole backend MUST deploy with one command (F-606).

Memories (F-701 to F-710)

- **FR-080**: On this day rows and nightly playlists (F-701), Then and now cards (F-702), best-of playlists (F-703), map view (F-707) and Ken Burns motion (F-708) MUST be produced from existing data.
- **FR-081**: People MUST be opt-in per household, named from the phone, usable as a filter, and deleted with the account; pets by label only (F-704).
- **FR-082**: Quick cleanup MUST propose screenshots, documents, receipts and blurry shots through Review Changes (F-705); text in photos MUST be searchable (F-706).
- **FR-083**: Narration MUST read the story during the slideshow when enabled (F-709).

### Key Entities

- **Household**: the unit of ownership and isolation; owns TVs, trips, preferences, memory, and the People setting.
- **TV session**: a signed-in TV within a household; holds its own now-showing state and pin/hide preferences when the trip is shared.
- **Contributor**: an Amazon account or a named guest bound to a trip code; owns their uploads; may be promoted to co-editor.
- **Trip code**: an 8-character code plus link; scoped to a household or one trip; rotatable and expirable.
- **Media**: one uploaded photo or video with metadata, renditions, contributor, scores, tags, burst group and keeper flag.
- **Burst group**: near-duplicate media across contributors with one keeper.
- **Trip**: a multi-day cluster with days, route, hero, highlights, story, contributors and memberships.
- **Day**: a slice of a trip with media, route segment and story paragraph.
- **Membership**: a household's reference to another household's trip, with role and per-household preferences.
- **Playlist**: an ordered media set from a query or a memory feature, playable on the TV.
- **Proposal**: a pending irreversible change with before/after counts and samples, awaiting confirmation.
- **Confirmation**: a single-use token minted on the TV for one proposal.
- **Reaction / Comment**: a like or short text from a contributor or member household on a day or photo.
- **Person**: an opt-in face cluster with an optional name.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: A new household goes from app install to a signed-in TV in under 60 seconds with nothing typed on the remote.
- **SC-002**: A second person scans the trip QR and their first photo is visible on the TV in under 2 minutes, credited to them.
- **SC-003**: A 400-photo, 7-day trip from three phones becomes a trip with days, route, highlights and a story with no manual step, within 15 minutes of the last upload.
- **SC-004**: Exact duplicates are rejected 100% of the time; near-duplicate groups across phones have exactly one keeper.
- **SC-005**: Zero stories shown contain a place or date absent from the photo metadata (verified by the automatic check on every story).
- **SC-006**: Zero irreversible changes occur without an on-remote confirmation (verified by audit log: every apply carries a confirmation).
- **SC-007**: Within-trip Ask answers appear in under 300 ms once warm; cross-trip answers in under 3 seconds.
- **SC-008**: A second household joins a trip by code and sees new photos within 30 seconds of them landing.
- **SC-009**: A reaction from a phone shows on the owning TV within 30 seconds.
- **SC-010**: A 5,000-photo batch completes without operator intervention.
- **SC-011**: The full test suite passes on a machine with no cloud credentials.
- **SC-012**: The demo video shows journeys 1 to 7 on a Fire TV device or Vega Virtual Device in under 3 minutes.

## Assumptions

- Photos come from the phone's camera roll or cloud picker through the browser; there is no direct import from Amazon Photos because it has no public API.
- Videos are stored and represented by poster frames; the TV does not play video in this baseline.
- One TV per household is the common case; multi-TV households are supported for now-showing but not optimised.
- Guests are identified by display name only and trusted at the level of the trip code; abuse is bounded by code rotation and owner removal.
- People is off by default and must be turned on per household; face data is matched only within that household's archive.
- Narration and Alexa+ voice depend on platform capabilities marked unverified in the decision log; text paths are the default and always available.
- Fire OS support is a stretch goal; acceptance is on Vega OS and the Vega Virtual Device.
- Storage retention: removed media is recoverable for 30 days; account deletion is immediate and complete.
- Sharing links expire after 7 days by default.
