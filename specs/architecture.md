# Mantelpiece: Architecture and Requirements

Status: Draft for hackathon execution
App name: Mantelpiece (mantelpiece.tv)
Companion: `hackathon.md` (entry strategy, rules, timeline, demo plan), `decisions-2026-09-09.md` (Fire TV research log)
Goal: zero infrastructure for the user. Install one Fire TV app, upload from phone, done.
Created: 2026-09-08

## 1. Deployment Model

An earlier design kept the archive on a home hub: Rust on a Pi 5 on the LAN, Cloudflare Tunnel
up to AgentCore Gateway, Fire TV as a LAN client. Dropped on 2026-09-08. Amazon devices cannot
host the archive themselves, and a hub costs the user a server, network config, a tunnel, and
their own AWS deployment:

- **Vega OS** (all new Fire TV Sticks): sandboxed React Native, no sideloading, weak USB and
  local-network file access. Cannot run a Rust core. Cleartext LAN HTTP, WebSocket and mDNS
  from the RN app were all unverified.
- **Fire OS** (Cube, 4K Max, 4K Plus, smart TVs): Android 11 based. MANAGE_EXTERNAL_STORAGE,
  USB and SMB are reachable, so a core could be embedded, but that locks the entry to Fire OS
  and skips the platform Amazon is pushing.
- **Echo Show**: no third-party apps. Reachable only through Alexa+ MCP.

The hub design had one property this design gives up: images analyzed at home by local ONNX
models, with the cloud seeing only metadata, narratives and thumbnails. The hosted answer is
the privacy position at the end of section 4; a local-first home node that syncs metadata up
stays a later option.

| Step | Self-hosted home hub (dropped) | This design |
|------|----------------------------|-------------|
| Buy and set up a home server or Pi | required | none |
| Configure home network access | required | none |
| Expose home server to cloud (tunnel) | required | none |
| Own AWS account, deploy agent stack | required per household | none, we host |
| Install Fire TV app from Appstore | required | required |
| Sign in | pairing code | Login with Amazon, already signed in on Fire TV |
| Get photos in | phone to home server | scan QR on TV, upload from phone browser |
| Friends' photos from same trip | copy files around by hand | they scan the same QR, upload from their own phones |
| Alexa+ | link add-on | link add-on |

User-facing steps: install app, scan QR, pick photos. Three.

## 2. Architecture

```
Fire TV app (RN for Vega + Fire OS)          Phone browsers (PWA uploader, no install)
   │ Cognito (LWA federated)                     │ owner + any number of trip contributors
   │ AppSync Events (push)                       │ joined via one QR / trip code
   │ shows trip QR                               │ presigned S3 multipart, resumable
   ▼                                             ▼
Backend API (API Gateway + Lambda, Rust)    S3 originals/{householdId}/{contributorId}/...
Backend API (API Gateway + Lambda, Rust)         │ S3 event
   │                                             ▼
   │                                   Step Functions "Ingest" per upload batch
   │                                     ├─ Lambda(Rust): EXIF, checksum, thumbnails
   │                                     ├─ Rekognition: labels, faces, text
   │                                     ├─ Bedrock Titan Multimodal Embeddings
   │                                     ├─ Lambda(Rust): near-dup grouping, quality score
   │                                     ├─ Lambda(Rust): trip clustering, route build
   │                                     ├─ Amazon Location: static route map PNG
   │                                     └─ emit `trip.detected` to EventBridge
   ▼
DynamoDB (media, trips, albums, playlists,   S3 Vectors (embeddings, durable)
            hot embedding cache per trip)
   ▲
   │ Lambda targets (no OpenAPI, no tunnel)
AgentCore Gateway + Identity (Cognito) + Policy (Cedar)
   ├─ AgentCore Runtime: Strands "Mantelpiece Concierge", Claude on Bedrock, AgentCore Memory
   └─ Alexa+ MCP add-on (same endpoint)
```

Single deploy for us: one CDK stack. Multi-tenant by household. Owner's Cognito `sub` is the
household id; contributors attach to it through a trip code. Per-household S3 prefix,
DynamoDB partition key, Memory namespace. Per-contributor sub-prefix for attribution and
revocation.

## 2a. Photo Sources

Amazon Photos has no public API, no OAuth scope, and no content provider on Fire TV. Amazon
Drive's third-party API ended in 2019 and the service closed in 2023. Login with Amazon scopes
are `profile` and `postal_code` only.

| Source | Status | Notes |
|--------|--------|-------|
| Phone camera roll via QR uploader | Primary, in scope | HTML5 file picker reaches the native roll and any cloud picker the phone exposes. Zero install. |
| Amazon Photos desktop bulk download, then upload | Supported, in scope | User-driven, first-party tool, EXIF preserved per reports (unverified). |
| Amazon Photos shared-album link ingest | Stretch, week 6 only if time | Lambda resolves a public share link and pulls assets. No credentials, but brittle against UI changes. |
| Google Photos Picker API, OneDrive | Post-hackathon | Official user-consented pickers. Widens the funnel later. |
| Amazon Photos private web API with session cookies | Rejected | Cookie scraping, Terms of Service violation, account-ban risk, and the judges are Amazon. |

Submission asset: the friction log and feature-request section name the missing Amazon Photos
MCP tool and LWA `photos:read` scope as the single biggest ecosystem gap for Alexa+ and Fire TV
memory apps.

## 3. Why Each Piece

- **First launch by pairing, no account (2026-09-17)**: the TV names its household (default:
  the TV's own name from `@amazon-devices/kepler-identifiers`, editable on the TV and only
  there) and `POST /tv/setup` creates it. The TV then shows a pairing code and QR
  (`POST /pair/code`, link `/?p=`). A phone that scans it gives a name
  (`POST /pair/requests`) and waits (`GET /pair/status`, held like the feed); the name sits
  in the TV's approval queue (`GET /pair/requests`, `phone.requested` on the feed) until
  Allow or Not now on the remote (`POST /pair/requests/{id}/approve|deny`). Approved, the
  phone holds a household session whose subject is its contributor id, so its photos are
  credited to the name it gave; Forget on the TV ends that session at once. Modelled on
  Bluetooth pairing: the scan is the out-of-band secret, Allow is the confirmation, the
  stored session is the bond. Only the TV approves phones, renames the household or deletes
  everything. Amazon sign-in below stays for the Alexa add-ons and is not part of onboarding;
  claiming a TV-made household with an Amazon account is not built yet.
- **Sign-in on TV by device code (Amazon account), optional**: TV shows an 8-character code and QR. Phone opens
  the link, signs in with Login with Amazon through Cognito, approves the code. TV polls and
  receives tokens. Cognito does not implement the OAuth device grant (RFC 8628) natively, so
  this is a small custom flow: Lambda issues and verifies codes in a DynamoDB table with TTL.
  Pure HTTPS from the TV app, no native bridge. Native LWA on-device sign-in is an upgrade
  only if the week 1 Vega spike passes (timeline in `hackathon.md`).
- **Concierge on AgentCore Runtime, tools through the Gateway (2026-09-21)**: the Strands
  agent in `agents/` deploys as code (no container; `scripts/build-agent.sh` gathers arm64
  wheels) to a Runtime. EventBridge has no Runtime target, so a small function forwards
  `trip.detected` to `InvokeAgentRuntime`. The agent calls its tools over MCP at the Gateway
  as its own Cognito client (client credentials), and the Gateway's REQUEST interceptor seals
  the verified caller into each tool call; see `contracts/tools.md`, "Who is calling".
- **Push by HTTP long-poll, default**: TV app polls `/events?since=` with a 25-second hold.
  AppSync Events over WebSocket is an upgrade if the Vega spike passes. Same event schema
  either way.
- **PWA uploader**: browser page, no app install. `<input type="file" multiple
  accept="image/*,video/*">` opens the phone's native camera roll and cloud pickers, which is
  the only user-consented way to reach photos on the phone. S3 multipart with presigned part
  URLs gives resumable uploads. Amazon Photos desktop bulk download feeds in the same way from
  a laptop browser. See section 2a for why this is the primary photo source.
- **Cheap before expensive in ingest**: hashing, sharpness, exposure and burst grouping run in
  the first Rust Lambda. Only keepers go to Rekognition and Titan. Non-keepers inherit the
  keeper's tags and embedding. Cuts model spend by the burst ratio, typically 3 to 5x on
  phone archives.
- **HEIC and MOV in Lambda**: iOS uploads HEIC and MOV by default. Rust Lambda statically links
  libheif and turbojpeg via cargo-lambda; video poster frames come from a separate Lambda with
  an ffmpeg layer. No transcoding of originals in the hackathon build.
- **Distributed Map**: an upload batch becomes an S3 manifest; Step Functions Distributed Map
  fans out per file with concurrency caps, so a 5,000-photo archive does not hit state
  machine history limits or Lambda timeouts.
- **Trip codes on the QR**: the same uploader link carries a trip code. Anyone on the trip
  scans the TV, signs in with their own Amazon account or continues as a named guest, and
  their photos land in the same trip. No app, no account on the owner's TV, no file passing.
  This is the headline feature: one QR turns a family trip into a shared album that builds
  itself.
- **Rust Lambdas via cargo-lambda**: metadata extraction, dedup, scoring, trip clustering
  and route building are pure compute with no model weights. Rust keeps them fast and cheap
  on Lambda arm64.
- **Rekognition + Titan embeddings**: no model hosting, no cold-start weight loading. Scene
  tags from labels, landmark hints from labels plus Bedrock Nova on low-confidence cases.
- **S3 Vectors plus hot cache**: S3 Vectors is the durable, cheap store. Query latency is
  sub-second to low-second, too slow for D-pad browsing and voice. Per-trip embeddings are
  cached in DynamoDB and loaded into the query Lambda's memory on first hit; Ask and Alexa+
  search the cache first, S3 Vectors for cross-trip queries.
- **Step Functions** for ingest: visible, retryable, per-batch. Also what AWS Builder judges
  call a "multi-service pipeline".
- **AgentCore Gateway with Lambda targets**: tools are the same Lambdas the API uses. No
  OpenAPI shim, no tunnel, no bearer secrets.
- **AppSync Events**: push `memory.ready`, `playlist.created`, `dedup.proposal` to TV.
  Unverified on Vega: WebSocket support in RN runtime. Fallback: polling.

## 4. Required Features

### 4.1 Ingest

| ID | Requirement |
|----|-------------|
| F-101 | Resumable upload from a phone browser: JPG, HEIC, PNG, MP4, MOV, up to 2 GB per file |
| F-102 | Per-file and batch progress visible on phone and on TV |
| F-103 | Checksum on arrival, exact-duplicate rejection before processing |
| F-104 | EXIF extraction: timestamp, GPS, camera, orientation. Timestamps normalized to UTC with original zone kept |
| F-105 | Thumbnail and TV-resolution rendition per photo, poster frame per video; HEIC decoded server-side, JPEG renditions served to TV |
| F-106 | Batches are idempotent and re-runnable |
| F-107 | Perceptual hash, sharpness, and exposure computed before any model call; burst groups pruned to keepers first |
| F-108 | Batches of any size process without hitting orchestration limits; per-household concurrency cap |

### 4.2 Shared Trip Contribution (headline feature)

| ID | Requirement |
|----|-------------|
| F-151 | TV shows a QR for the household and, inside a trip, a QR scoped to that trip; both open the same uploader with a trip code in the link |
| F-152 | Contributor joins with their Amazon account (LWA) or as a named guest with only a display name; guest sessions bound to the code and device |
| F-153 | Owner can rotate or expire a trip code at any time; expired codes stop new uploads, keep existing contributions |
| F-154 | Every photo carries contributor identity; TV, narrative, and highlights show who took it |
| F-155 | Timelines from many phones merge on capture time, not upload time; per-contributor clock offset detected from overlapping GPS and corrected, correction shown as a hint |
| F-156 | Near-duplicate grouping runs across contributors: five phones at the same landmark yields one group with a keeper, contributors' shots credited |
| F-157 | Contributor can see and remove their own uploads from the phone page; removal cascades to renditions, embeddings, albums, playlists |
| F-158 | Owner can remove any contributor; their originals are detached from the trip and deleted after a grace period, or exported to them by link |
| F-159 | Contributors get a read-only web view of the finished trip (narrative, highlights, route) by the same link, gated by the code |
| F-160 | Contribution activity streams to the TV live: "Maya added 42 photos from day 3" |
| F-161 | Upload page works while still on the trip, offline-tolerant: queued uploads resume when the phone reconnects |

Roles (decided 2026-09-09): **owner** (the household that created the trip), **co-editor**
(a contributor with an Amazon account, promoted by the owner), **contributor** (Amazon
account or guest, can add and remove their own photos, view, react), **member household**
(another TV that joined the trip by code, view and react, personal pin and hide).

| ID | Requirement |
|----|-------------|
| F-162 | **Join a trip from another TV (headline with F-151).** The trip code on the Invite QR also works on any other Fire TV running Mantelpiece: "Join a trip" on the Memories home takes the code typed on the remote or scanned from the phone page while that TV's session is linked. The trip appears in that household's rows with live updates, narrative, route and highlights, credited contributors, reactions. Grandma's TV shows the Kyoto trip the evening it happens. Trip stays owned and stored by the creating household; the member household holds a membership record, no copy |
| F-163 | Pin and hide are per household on shared trips: a member household's highlights differ from the owner's without changing the trip. Owner and co-editor edits (F-164) are shared |
| F-164 | Co-editor role: owner promotes a contributor with an Amazon account from the TV. Co-editors can rename the trip, choose keepers, remove any photo, run dedup and cleanup on that trip, each through their own Review Changes modal (F-410) with a confirmation token bound to their identity; Cedar policy at the Gateway checks the role per tool call (F-504). Removing another contributor's photo notifies that contributor and is reversible for the grace period in F-158 |
| F-165 | Guests (no Amazon account) never edit; the web view (F-159) never edits. Editing happens only on a TV with a signed-in owner or co-editor session |

### 4.3 Organization

| ID | Requirement |
|----|-------------|
| F-201 | Near-duplicate grouping (burst shots) with a suggested keeper chosen by sharpness and exposure |
| F-202 | Scene tags per photo (beach, mountain, city, museum, food, people) with confidence; non-keepers in a burst inherit the keeper's tags and embedding |
| F-203 | Landmark detection with confidence; low-confidence cases escalated to a multimodal model |
| F-204 | Quality and interest score 0 to 10 per photo: faces, smiles, landmark, sharpness, uniqueness |
| F-205 | Event clustering by time gap and geographic proximity; trip = multi-day cluster |
| F-206 | Trip route from GPS; when GPS missing, infer from landmarks and scene progression, flagged as inferred |
| F-207 | Route rendered as a static map image per trip and per day. Met by F-712's sketch maps since spec 003 (the Amazon Location static maps could not be kept) |
| F-208 | Auto-named albums per trip and per event |
| F-209 | Highlights: top N per trip, user can pin or hide |
| F-210 | Landmark facts: recognition picks from the sights near the photo's position; each recognised landmark gets its encyclopedia summary (Wikipedia, credited on screen), cached once for all households; the Concierge writes up to three trivia lines, stored only when every number, date and name is in the summary. Shown on the first slideshow photo of a place, in the day panel, and in "what's that?" answers |

### 4.4 Narrative and Search

| ID | Requirement |
|----|-------------|
| F-301 | Day-by-day trip narrative from structured data only; no place or date not present in metadata; names contributors where their photos anchor a moment |
| F-302 | Narrative marked with model id and generation time; regenerate on demand |
| F-303 | Semantic search over photos by natural language, combined with scene, trip, date, people filters; within-trip queries answer under 300 ms warm |
| F-304 | Playlists: ordered photo sets built from any query, playable as slideshow on TV |

### 4.5 Fire TV Experience

| ID | Requirement |
|----|-------------|
| F-401 | First launch: name the household on the TV (default the TV's name), then pair phones by QR: the phone gives a name, the TV approves it from a queue. No account. Amazon sign-in by device code remains for the Alexa add-ons |
| F-409 | Trip view has an "Invite" action that shows the trip-scoped QR full screen, with live contributor count and recent arrivals |
| F-402 | Memories home: trips as rows, hero image from top-scored photo, "new memory" badge |
| F-403 | Trip view: horizontal day timeline, route map, narrative, play slideshow |
| F-404 | Highlights view with D-pad navigation and pin / hide |
| F-405 | Ask: text prompt on the TV, voice through Alexa+ or the phone page; answer as card or playlist |
| F-406 | Ambient slideshow when idle, adapted to household preferences |
| F-407 | Live status on TV: upload progress, memory ready, dedup proposal; works over plain HTTPS polling, WebSocket optional |
| F-410 | Review Changes modal: any agent-proposed destructive action (dedup apply, delete, contributor removal) shows before/after counts and sample thumbnails; nothing applies until the user confirms on the remote |
| F-411 | Join a trip (F-162): code entry on the Memories home and a scan-from-phone path; joined trips show a "shared by <household>" badge and a Leave action |
| F-412 | Play on TV from the phone, Chromecast-style: a trip, a day or one photo from the owner's trips page; `play.requested` carries the photo to open at, every TV in the household picks it up, and the phone confirms from `/presence` which TV is showing it. A member household plays a joined trip on its own TVs. No LAN discovery: the cloud feed is the cast channel (decision 2026-09-15, `video-call-feasibility-2026-09-15.md` section 8a) |
| F-408 | Runs on Vega OS; passes Vega Virtual Device. Fire OS from the same `packages/tv-ui` code is a stretch goal (decision 2026-09-09) |

### 4.6 Agents and Alexa+

| ID | Requirement |
|----|-------------|
| F-501 | All read and write capabilities exposed as MCP tools through one governed endpoint |
| F-502 | Concierge agent with per-household long-term memory and per-session short-term memory |
| F-503 | Event-driven workflows (trip detected, dedup proposal, nightly) without user prompt |
| F-504 | Irreversible tools (apply dedup, delete, share) require a confirmation token minted by the TV Review Changes modal (F-410) and a Cedar policy check at the Gateway; agents cannot self-confirm |
| F-505 | Alexa+ can call the same tools; answers render as voice and Echo Show visuals |
| F-506 | Every agent run traceable end to end for debugging and demo |

### 4.8 Memories (Google Photos parity, decided 2026-09-09)

Everything here runs on data ingest already produces. Judging priorities covered: AI-enhanced
viewing, family entertainment, computer vision. Full comparison, including what was skipped,
in `decisions-2026-09-09.md` section 9.

| ID | Requirement |
|----|-------------|
| F-701 | On this day: photos from the same month and day in earlier years, as a home row and as a W3 nightly playlist; Alexa+ answers "what were we doing this week three years ago" |
| F-702 | Then and now: pairs of photos from the same geohash cell taken in different years, shown as a card |
| F-703 | Best of month and best of year playlists from the F-204 score |
| F-704 | People: opt-in per household, switched on and off only on the TV; faces indexed into a per-household Rekognition collection, clustered by match, named from the phone page; people become a search and playlist filter, narrow a trip and its slideshow to the photos they are in, and feed the F-204 score. Every face in a photo is matched on its own. Vectors only, deleted with the account (F-604). Pets are label search only, no identity |
| F-705 | Quick cleanup: screenshots, documents, receipts and blurry shots detected from Rekognition text and labels plus F-107 sharpness, proposed as a W4-style Review Changes batch; nothing removed without the remote |
| F-706 | Text in photos: Rekognition DetectText indexed for search and Ask ("that restaurant sign in Kyoto") |
| F-707 | Map view: one Amazon Location static map with every trip pinned, as a TV screen |
| F-708 | Ken Burns motion on stills in every slideshow; no video encoding, no video stack on the TV |
| F-709 | Narrated memories: Polly reads the trip narrative during the slideshow; gated on the Vega audio spike (open question 10) |
| F-710 | Reactions and comments from contributors on the F-159 read-only trip view (like, short comment per photo or day); they stream to the TV ticker ("Grandma loved day 3") and appear beside the photo in the trip view |
| F-711 | **Field notes.** Each trip highlight gets a quiet map of where it was taken, drawn from OpenStreetMap and open terrain data into the photo's calmest empty space, clear of its subjects, as a separate layer: the photo is never altered. A dot and rounded coordinates only for the photo's own GPS, a ring for an inferred place, none for an unknown one, rings only near home and on shared or member views. A one-button Field notes mode in the TV slideshow; the map takes remote focus and opens This place (when, where, the main feature with its credited description); the phone views and shares it at full resolution. Spec 002 |
| F-712 | **Trip sketch maps.** A hand-drawn map of each trip and each day from OpenStreetMap and open terrain data: wobbly ink coast, watercolour sea and glaciers, pencil hachures, the route as dashed ink with circled day numbers, sketched pictograms at the trip's places, a hand-lettered title box. Nothing invented; estimated legs fainter; route points near home left out, with a soft home area on the owner's form only. Shown in TV Trip view and for four seconds at each new day in the slideshow; shared from the phone. Spec 003 |

### 4.7 Sharing, Security, Operations

| ID | Requirement |
|----|-------------|
| F-601 | Share an album or playlist by expiring link, optional email |
| F-602 | Per-household data isolation across storage, index, memory, and tools; contributors and member households (F-162) see only trips they joined, and only that trip's data |
| F-603 | Encryption at rest and in transit; no model training on user content |
| F-604 | Delete account removes originals, renditions, embeddings, memory |
| F-605 | Audit log for auth, share, delete, and agent tool calls |
| F-606 | One-command deploy of the whole backend from source |

Privacy position: originals live in S3 under the user's identity, encrypted at rest. Bedrock
and Rekognition do not train on customer data. State this plainly in the submission. A
local-first home node that syncs metadata up is a possible later option, not hackathon scope.

## 5. Fire TV App

Screens: Memories home, Trip view (day timeline, route map, narrative), Highlights, Ask,
Ambient, Settings. Plus:

- **Onboarding**: first launch shows an 8-character code and QR to `link.<domain>/?c=<code>`.
  Phone scans, signs in once (LWA via Cognito), approves. TV is signed in. Same phone session
  continues to `upload.<domain>/?h=<household>`. TV shows live progress by polling.
- **Invite to a trip**: inside a trip, "Invite" shows `upload.<domain>/?t=<tripcode>` as a
  full-screen QR. Everyone in the room scans it. Their photos merge into this trip, credited
  to them, and the TV ticks up as they arrive. Same QR works later from the phone page, so
  people who left early can still contribute.
- **Voice for Ask**: not from the TV app. Vega exposes no microphone API to apps (open
  question 7, closed 2026-09-09). Voice reaches the same MCP tools through Alexa+ (F-505) or
  through an Ask button on the phone page using the browser's speech recognition. Text entry
  is the default on the TV. "What's that statue in the background" needs the TV to publish a
  now-showing record and a `describe_photo` tool answering from descriptions precomputed at
  ingest; see `decisions-2026-09-09.md` section 7.
- **No hub address, no network config screens, no typing on the remote.**
- **Vega-first rule**: no native modules bundled by us. Everything the TV app needs is `fetch`,
  timers, images over HTTPS, and RN core components. System-deployed `@amazon-devices/*` ports
  (AsyncStorage, qrcode-svg) are allowed. Anything beyond that is an upgrade gated on the
  week 1 spike.

## 6. Agentic Workflows

| # | Workflow | Trigger | Steps |
|---|----------|---------|-------|
| W1 | Trip Curator | EventBridge `trip.detected` invokes Runtime agent | get_trip, score highlights, narrative via Bedrock, create album, create playlist, push `memory.ready` |
| W2 | Ask Your Archive | TV or Alexa+ prompt | intent, vector search (S3 Vectors) + scene/trip filters, create playlist, TV plays |
| W3 | Household Patterns | nightly EventBridge schedule + pin/hide feedback | read Memory, choose ambient set, create playlist, store rationale |
| W4 | Dedup Steward | `dedup.proposal` | summarize groups, propose keepers, open Review Changes on TV, apply under Policy with the modal's confirmation token |
| W5 | Share / Export | "send Kyoto highlights to grandma" | prefer F-162: if grandma's household is known, propose a trip join code for her TV; otherwise create shared album link (presigned, expiring), email via SES |
| W6 | Trip Merge | `contribution.received` from a new contributor | align clocks against existing GPS, re-run dedup across contributors, re-cluster days, refresh narrative with contributor credits, push "Maya added day 3" to TV |

Same "confirm before irreversible" pattern throughout. Cedar policy on `apply_dedup`,
`delete_media`, `share_album`.

## 7. AWS Services Used (AWS Builder mini-challenge)

Bedrock (Claude, Titan Multimodal Embeddings, Nova), AgentCore Gateway, Runtime, Identity,
Memory, Policy, Observability, Rekognition, S3, S3 Vectors, DynamoDB, Step Functions,
EventBridge, Lambda (Rust), API Gateway, AppSync Events, Cognito, Amazon Location, SES, CDK.

## 8. Cost Envelope (rough, per 10k photos, unverified)

Rekognition labels ~$10, Titan embeddings ~$1, thumbnails/Lambda ~$1, S3 ~$0.25/month at
avg 5 MB, Bedrock Claude narrative per trip cents. Demo account well under $100 total.

## 9. Open Questions (verify week 1)

Documentation findings of 2026-09-09 are in `decisions-2026-09-09.md` section 6; items marked
"docs say" still need a device check.

1. Native LWA sign-in from Vega RN app. Docs say `@amazon-devices/keplerscript-identity-lwa-lib` supports the auth-code grant with PKCE. Default is device code; this only decides whether the upgrade ships.
2. WebSocket / AppSync Events from Vega RN runtime. Docs say WebSocket is supported on React Native for Vega 0.83. Default is long-poll; same decision shape.
3. Gateway MCP spec version vs Alexa+ 2025-11-25, plus Alexa+ inbound auth: OAuth 2.1 with PKCE and a protected resource metadata document on the MCP endpoint, and a 500 ms round-trip budget per tool call.
4. AgentCore Policy and S3 Vectors availability in the chosen region (us-east-1 or us-west-2).
5. cargo-lambda arm64 static build of libheif and turbojpeg on Amazon Linux 2023; ffmpeg layer size under the 250 MB unzipped limit.
6. Guest identity without an Amazon account: Cognito unauthenticated identities vs signed short-lived upload tokens bound to the trip code.
7. Closed: no microphone API for Vega apps. Voice goes through Alexa+ or the phone page (section 5).
8. React Native version for `apps/tv-vega`: 0.83 (SDK 0.24 early access) if the install grants it, else 0.72. Either way `apps/tv` at 0.87 splits into `packages/tv-ui` plus per-platform shells.
9. Session persistence: docs say `@amazon-devices/react-native-async-storage__async-storage` is system-deployed. Confirm on VVD, then replace `MemorySessionStore`.
10. Audio playback from the Vega app for F-709 narration without bundling a native module. The reference sample used the `@amazon-devices/react-native-w3cmedia` port; check whether it is system-deployed.
