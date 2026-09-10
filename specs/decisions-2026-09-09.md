# Decision Log: 2026-09-09

Companion: `architecture.md` (design), `hackathon.md` (plan). This file records the findings
and decisions from one day of research on the Fire TV side: the Amazon multi-TV sample, the
Vega SDK version pin, the Vega Virtual Device, Amazon Devices Builder Tools, the Vega versus
Fire OS question, and voice input. Each section ends with what changed in the repo or the
specs. Claims marked *unverified* come from documentation, not from a device.

## 1. Borrowed from AmazonAppDev/react-native-multi-tv-app-sample

Read the whole repo. Its Vega app is React Native 0.72 with `@amazon-devices/react-native-kepler`
major 2, built by `@amazon-devices/kepler-cli-platform` from the SDK's private registry. Its
shape is one `packages/shared-ui` of pure RN code plus thin `apps/vega` and
`apps/expo-multi-tv` shells, so two React Native versions coexist in one repo.

Taken:

- `manifest.toml` structure, trimmed to network and input method only (no media, DRM, audio).
- Reverse-DNS `app.json` name that matches the manifest component id.
- `kepler` block and `react-native build-kepler` scripts in `package.json`.
- `react-native.config.js` with `newArchEnabled: false` for the spike.
- Metro `sourceExts` with `.kepler.ts` first, for platform variants next to the default file.
- Back-key handling: `TVEventHandler` on Vega, `BackHandler` on Fire OS, as `useRemoteKeys`.
- Safe-zone constants (5% title-safe, 3.5% action-safe at 1080p) in `theme.ts`.
- CONTRIBUTING, CODE_OF_CONDUCT, issue and PR templates, `.prettierrc`, gitignore entries.

Not taken: `react-tv-space-navigation`, Amazon ports of react-navigation, reanimated,
gesture-handler, the W3C media stack, Expo for Vega. All against the no-native-modules rule.
Also not taken: the sample's committed `buildinfo.json` with absolute paths.

Repo: `apps/tv/manifest.toml`, `apps/tv/app.json`, `apps/tv/react-native.config.js`,
`apps/tv/metro.config.js`, `apps/tv/src/hooks/useRemoteKeys*.ts`, root hygiene files.

## 2. Vega SDK pins React Native; apps/tv must split

`apps/tv` is React Native 0.87.1 with React 19. React Native for Vega supports 0.72, and since
SDK 0.24 (August 2026) also 0.83 as early access: React 19.2, Hermes v1, new architecture on,
kepler package major 4. The bundled Builder Tools alpha note says 0.83 needs a private npm
registry and allowlist and cannot be submitted to the Appstore; that note references SDK
0.23 and may be stale against the 0.24 release notes. The hackathon needs a demo, not an
Appstore listing, so either version works.

Decision: split the TV code into `packages/tv-ui` (screens, hooks, api client, all RN core)
with thin `apps/tv-vega` (RN pinned by the SDK, 0.83 if reachable, else 0.72) and later
`apps/tv-fireos`. pnpm isolates per-workspace by default, so no package manager change.
`@amazon-devices/*` packages never go into `package.json` on `main` until CI can install
them; they break `pnpm install --frozen-lockfile`.

Manifest addition required by SDK 0.24: `[os.version] target = "1.2" min = "1.2"`.

Done the same evening (Spec Kit Phase 1, tasks T001 to T007). Findings that changed the plan:

- `@amazon-devices/react-native-kepler` 4.0.1 (`+rn0.83.0`, a full fork of React Native
  0.83 with `.kepler.js` platform files), `@amazon-devices/kepler-cli-platform` 0.22.14,
  `@amazon-devices/kepler-module-resolver-preset` 0.1.16 and
  `@amazon-devices/keplerscript-commonmodules` 1.1.0 are on public npm under `latest`. The
  private registry and allowlist note was stale. So the constitution VI gate is met by
  `pnpm install --frozen-lockfile` with no token, and `apps/tv-vega` declares them now.
- Package shape follows `AmazonAppDev/react-native-multi-tv-helloworld` (RN 0.83): stock
  `react-native` 0.83.0 plus the kepler package, `@react-native/babel-preset` and
  `metro-config` 0.83.0, `@react-native-community/cli` 20, `@babel/runtime` 7 (8.0 drops
  `regenerator`, which `react-devtools-core` still imports).
- `react-native build-kepler` is deprecated for `build-vega`; `bundle-vega` produces the JS
  and Hermes bundle without the SDK and runs in CI. The `.vpkg` step needs the `vega` CLI.
- The React Native CLI rewrites `react-native` imports to the kepler package through its
  own Metro `resolveRequest`. A project-level `resolveRequest` replaces it and the bundle
  pulls in stock React Native, which has no `.kepler` variants and fails. The shell's Metro
  config therefore only sets `watchFolders`, `nodeModulesPaths` and `sourceExts`.
- Manifest for kepler 4: `runtime-module` is
  `/com.amazon.kepler.runtime.react_native_kepler_4@IReactNativeKepler_0` and `[needs]`
  lists `/com.amazon.vega.os@IVega_1_2` and `/com.amazon.kepler.keplerscript_4@IKeplerScript_0`,
  copied from the sample. *Unverified* on a device.
- Verified locally: the Debug and Release bundles build, contain the 17 `tv-ui` modules,
  35 kepler modules and no stock React Native, and pick `useRemoteKeys.kepler.ts`. `packages/tv-ui`
  tests run against stock React Native 0.83.10 (no 0.83 `@react-native/jest-preset` exists;
  the preset inside `react-native` is used).

## 3. Vega Virtual Device in the dev process

VVD ships with the SDK. macOS (Apple Silicon needs `aarch64` builds; Intel Mac support ends
2026-12-15) and Ubuntu with KVM. Keyboard maps to the remote: arrows, Enter select, Esc back,
F4 play/pause. Registration with an Amazon account is optional; our sign-in happens on the
phone, so the TV never needs it unless the native LWA upgrade ships.

Uses, in order of value:

1. Daily loop: `vega virtual-device start`, reverse port forward 8081, Metro, debug `.vpkg`,
   `vega run-app ... -d VirtualDevice`, `vega device start-log-stream`. Fast Refresh works.
2. Full QR flow on a laptop: the phone camera scans the QR off the monitor, hits the real
   `link.` and `upload.` pages, the TV long-polls the deployed API.
3. Local backend: the Metro reverse port forward should let the TV app reach a local API on
   another port. *Unverified* for arbitrary ports.
4. Scripted walkthroughs and golden screenshots: enable Developer Mode in the device shell
   (`vsm developer-mode enable`), then `inputd-cli button_press KEY_DOWN|KEY_ENTER|KEY_BACK`
   and `vega exec vda shell gwsi-tool-screenshooter out.png` pulled with
   `vega device copy-from`. Correction from the CLI reference: the screenshooter runs only
   on a physical Fire TV, so Virtual Device captures come from the host window. SDK 0.24
   adds a text dump of the rendered screen and the view hierarchy. Appium Vega Driver is
   the heavier option.
5. Demo video: rules accept simulator footage. VVD crashes under simultaneous screen, audio
   and video recording when free memory drops under 2 GB; record the real stick if possible.

Not covered by VVD: performance (docs say stick before submission), hardware Back semantics
with no handler on the home screen, microphone, Fire OS, CI (needs GUI and KVM). Physical
stick path: `vega devmode login`, `vega devmode enable-device --code <6 digits>`, `armv7`.

Added (unverified, no SDK on this machine yet): Makefile targets `tv-vvd-start`,
`tv-vvd-install`, `tv-vvd-logs`, `tv-vvd-shot`, `tv-walkthrough`; `scripts/tv-walkthrough.sh`
writing to `docs/screens/`.

## 4. Amazon Devices Builder Tools (ADBT) installed

`@amazon-devices/amazon-devices-buildertools-mcp` 1.0.10, installed 2026-09-09 for Claude
Code: MCP entry in `~/.claude.json`, context document merged into this repo's `CLAUDE.md`,
`.adbt-config.json` with `platform.default = vega_os`, 13 skills in `~/.claude/skills`.
Only doc-search queries and anonymous telemetry leave the machine, per Amazon.

Its doc search answered four spec open questions in one session (section 6). Skills relevant
here: `vega-setup-sdk`, `vega-build-and-run`, `vega-app-manifest`, `vega-focus-management`,
`vega-developer-mode-init`, `vega-rn-upgrade`, and community `vega-multi-tv-migration`,
which is the split in section 2. Media player, IAP, Content Launcher, Live TV and porting
skills are noise for this app; the two least relevant (`media-player`, `app-migration`) can be
deleted from `~/.claude/skills` by hand.

Friction: tool argument shapes are not shown by the CLI (`target_platform` must be
`{"device_os": ["vega_os"]}`, `read_document` takes `document_uri` only); skills install
globally; 16 MB package per MCP start unless cached; build tools need SDK 0.22 or later.

## 5. Vega OS versus Fire OS

Vega is primary: all new Fire TV Sticks ship it, real-device footage is the Fire TV Stick 4K
Select, and the Vega-first rule keeps the app free of Android dependencies. Fire OS stays in
F-408 as a stretch. It costs an Android toolchain, an emulator or a Cube, and a second set of
build issues for no extra track credit; the rules accept either a Fire TV device or the Vega
simulator in the video. If Fire OS does not land, the submission text says so.

## 6. Open questions answered by documentation (not yet on a device)

| Open question | Finding | Consequence |
|---------------|---------|-------------|
| 1. Native LWA from the Vega app | `@amazon-devices/keplerscript-identity-lwa-lib` `AuthorizationManager.authorize` supports `AUTH_CODE` with PKCE | Device code stays default; native sign-in is a real upgrade |
| 2. WebSocket from the Vega runtime | Supported on React Native for Vega 0.83 per its network docs | AppSync Events upgrade viable |
| 7. Microphone from the Vega app | No app-level mic API found; Voice Search is a system overlay that blurs the app | Voice goes through Alexa+ or the phone (section 7) |
| Session persistence (was an unlisted gap) | `@amazon-devices/react-native-async-storage__async-storage` is system-deployed: native side in the OS, app aliases it as `@react-native-async-storage/async-storage` | Replaces `MemorySessionStore` without breaking the no-native-modules rule |
| QR rendering | `@amazon-devices/react-native-qrcode-svg` is system-deployed | Current `qrcode` npm plus custom `QrView` also fine |

Rule refinement: "no native modules" means none bundled by us. System-deployed
`@amazon-devices/*` ports are allowed.

## 7. Voice input: Alexa+ and the phone, not the TV app

Scenario: the user pauses the slideshow and asks "what's that statue in the background".

On Vega the app cannot open the microphone. The remote's Alexa button goes to system Alexa.
So voice travels Alexa+ to our MCP tools (F-505, W2). Three pieces are missing:

1. Screen context. Alexa+ add-ons receive no device or on-screen context. The TV app posts a
   presence record on every slide change and pause (household, media id, paused). New MCP
   tool `get_now_showing` reads it.
2. Routing. Alexa+ routes by tool descriptions and `examplePhrases`, no invocation name. A
   bare "what's that statue" may be answered from general knowledge or Prime Video's own
   screen context. Example phrases and demo lines mention the photo or the TV.
   *Unverified* until preview access.
3. Answering. New tool `describe_photo(media_id, question)`: rendition, EXIF GPS,
   Rekognition labels, trip and day context, into a Bedrock multimodal model. Answer returns
   as voice and as an `answer.card` event on the long-poll feed, drawn over the paused photo.

Risks: Alexa+ requires a 500 ms round trip from the MCP server, so landmark and scene
descriptions for keepers are precomputed at ingest and `describe_photo` answers from cache,
live model call only as fallback. Alexa+ also requires OAuth 2.1 with PKCE and a protected
resource metadata document on the MCP server; added to open question 3.

Fallback without Alexa+ access: an Ask button on the phone PWA using the browser's speech
recognition, calling the same MCP tools, answer card on the TV. Phone as voice remote. Works
on Vega, and is what judges see if the preview never arrives. Fire OS `SpeechRecognizer`
skipped under Vega-first.

Spec changes: F-405 becomes text on the TV, voice through Alexa+ or the phone. Open
question 7 closes.

## 8. Devpost

Project page created 2026-09-08 with name, tagline, description and built-with tags:
https://devpost.com/software/memorylane-0lsmz3. No repo link yet (repo not public), no video,
form not submitted. Still owed: public repo with visible MIT license, video under 3 minutes
on a Fire TV device or VVD, track and mini-challenge answers, five feedback questions,
friction log URL.

## 9. Google Photos parity

Compared Google Photos against the spec. Most of its consumption features were already
planned under other names: trip memories (F-205 to F-208, with narrative and contributor
credit Google lacks), photo stacks (F-201), highlights (F-209), shared albums (F-151 to
F-161, no app or account needed), Ask Photos (F-303, F-405), storage saver (W4), ambient mode
(F-406). Added as section 4.8, F-701 to F-710: on this day, then and now, best of month and
year, opt-in people, quick cleanup, text in photos, map view, Ken Burns slideshow, Polly
narration, and reactions and comments from contributors.

Considered and left out: share suggestions by person (after F-704 exists), private trips,
highlight reel MP4 export via ffmpeg (playback on the TV would need the W3C media port),
Magic Eraser by voice through Nova Canvas text-mask inpainting (novel, scope risk, licence
check). Skipped outright: Photo Unblur, Best Take, portrait blur, HDR, Reimagine, Video
Boost (no AWS primitive, no editing UX on a remote), phone backup and free-up-space (a PWA
cannot delete from a camera roll), prints, pet identity.

## 10. Sharing roles and a second TV

Sharing was contribute by QR (F-151 to F-161), read-only web view (F-159) with reactions
(F-710), and share-out links (F-601, W5). Nobody but the owner could edit, and a trip lived
on one TV only. Added F-162 to F-165 and F-411: join a trip from another Fire TV by the same
code (headline alongside F-151), per-household pin and hide, a co-editor role gated by
Review Changes and Cedar, and no edits for guests or from the web view. W5 now prefers a
join code over an emailed link when the recipient has a TV. Demo beat sheet gains the
second-TV moment.

## 11. Phase 2 foundations (Spec Kit tasks T008 to T029)

Domain types, storage, model backends, roles and the Gateway policy that every user story
builds on. All of it runs under `make test` with no AWS; the AWS backends are
compile-checked only.

- Core: `Role` on `Contributor` (guests never co-edit), `Membership`, `Presence` (10
  minute staleness), `Proposal` (open, cancelled, confirmed, applied, expired after 24 h),
  `Reaction` (one like per actor per target, comments at most 200 characters), `Person`,
  `ShareLink` (7 days), `Household` (people and narration flags, deletion marker),
  `ConfirmedAction::{ApplyCleanup, RemoveMedia}` with a token `subject`, soft removal on
  `MediaItem` with a 30 day restore window and per-household pins and hides, six new feed
  kinds plus the member-household filter, `NarrativeCheck` and per-day stories on trips.
- Store: the `Storage` trait grew memberships, presence, proposals, reactions, people,
  share links and playlists (sort keys in `dynamo.rs`). New traits with in-memory fakes:
  `Vectors` (cosine ranking; `DynamoVectors` per-household rows; `S3Vectors` with a
  household and trip metadata filter), `Faces` (Rekognition collections), `Vision`
  (moved out of the ingest Lambda; Rekognition labels, faces and text plus a Bedrock
  Converse description, default model `amazon.nova-lite-v1:0`), `Maps` (Amazon Location
  Maps v2 `GetStaticMap` with a GeoJSON overlay) and `Speech` (Polly neural, MP3).
- API: `App::trip_access` resolves Owner, Member, CoEditor or Contributor for one trip,
  with `owner_or_co_editor` and `member_or_owner` guards; co-editors mint confirmation
  tokens for the actions their role allows and the token records who confirmed.
- Infra: AgentCore Gateway (MCP, Cognito JWT authorizer) with one Lambda target listing
  all 22 tools from contracts/tools.md, a Policy Engine loading `infra/policy/tools.cedar`
  in enforce mode, and an EventBridge Scheduler schedule that puts `nightly.tick` on the
  bus at 03:00 UTC (per-household local time is the Concierge's job). *Unverified*: the
  Cedar entity and action naming (`AgentCore::Action::"api___<tool>"`, `context.input`)
  and the L1 property values (`supportedVersions`, `searchType`, `GATEWAY_IAM_ROLE`)
  follow the CloudFormation schema shipped in CDK 2.268 and have not been deployed.
- Agents: `InMemoryTools` fakes for `get_now_showing`, `describe_photo`,
  `list_cleanup_candidates`, `memories_for` and `apply_cleanup`, mirrored on
  `GatewayTools`.

## 12. US1 sign-in (tasks T030 to T034)

- Session persistence: `@amazon-devices/react-native-kepler` 4.0.1 still exports
  `AsyncStorage` from `react-native` (an `amznmod` in its `index.js`, listed as a system
  module), so the Vega shell uses that export through a runtime lookup and bundles no
  storage package. The `npm:` alias to `@amazon-devices/react-native-async-storage__async-storage`
  stays the fallback if a device run shows the export missing. *Unverified* on a device.
- The TV names the household: `POST /device/code` accepts `tv_name`, the first approval
  creates the `Household` record with it, and `GET /me` returns `household_name` plus the
  people and narration flags. A second approval of the same code answers 409
  "already signed in".
- Codes expire on the TV's own clock as well as the server's; the Onboarding screen shows
  "Get a new code" and one press issues a fresh one. The sign-in hook reads its callbacks
  through a ref so a re-render never restarts the flow.
- The phone continues from `link.` to `upload.` with the owner session in the URL
  fragment, stored and stripped on arrival; no second sign-in.
- T035 (Virtual Device run) is blocked until the Vega SDK is installed on the build
  machine.

## 13. US2 uploads (tasks T036 to T041)

- `GET /uploads/mine` lists the caller's own uploads with state, newest first, removed
  items excluded.
- Phone: "duplicate, skipped" for a 409 on init; a failed file keeps a Retry button that
  re-runs the upload from the resume record, so only the missing parts are sent and no new
  init happens. The TV ticker reads "Uploaded N, organising…" when a batch completes.
- Videos: the signal task flags `poster: true`; the signal state machine branches to a new
  `poster` Lambda that runs a static ffmpeg from a layer (`scripts/ffmpeg-layer.sh`,
  John Van Sickle arm64 build, about 80 MB unpacked, GPL) and writes `poster.jpg`, the thumb
  and TV renditions and the quality signals, so a video behaves like a still everywhere.
  Frame extraction is a trait; the test uses a scripted frame. The layer binary is not in
  git; without it the stack packages a placeholder and the poster step fails at runtime.
  *Unverified*: ffmpeg on arm64 Lambda, and the codec coverage of that build.
- F-108: organize writes a keepers manifest to `manifests/<household>/` and the enrich
  step is a Step Functions Distributed Map over it with four keepers in flight per
  household execution. Synth-checked in the CDK tests, not deployed.
- F-106: the pipeline test now asserts that re-running organize over a completed batch
  adds no media, trips or feed events and rewrites the same manifest shape.

## 14. US3 shared trip (tasks T042 to T048)

- Codes carry live `contributors` and `member_households` counts; the Invite screen shows
  them and has a "New code" button that rotates in place.
- Guest sessions are bound to a browser install: the phone sends a random `x-device-id`
  it keeps in local storage, `/join` stores it in the token, and any call from another
  device is refused. A guest token also expires with its trip code (F-153), so an expired
  code answers 401 rather than 410 on upload. Amazon-account contributors are not bound;
  they can always re-join.
- `GET /trips/{trip}` adds `contributor_names` and `media_credits`, so every thumbnail on
  the TV and the phone carries a name (F-154), and the day summary reads "from Hanyu,
  Maya". Clock offsets over a minute render as "Maya's clock corrected by 3 min" (F-155).
  A Highlights screen shows the trip's picks with credits.
- Removing media is a soft removal (state `removed`, 30-day `restore_until`) that cascades
  out of the trip's days, highlights and hero, every playlist and the embedding index;
  originals and renditions stay until `purge_expired` (US6). Contributor removal does the
  same for all their media with `restore_until` set to a 30-day `grace_until`, and
  `GET /contributors/{id}/export` gives the removed contributor (or the owner) presigned
  links to the originals until then (F-158).
- Read-only web view (F-159): `GET /view?code=` and `GET /media/{id}/url?code=` open a
  trip by its trip-scoped code with no session; household codes and rotated codes are
  refused. The phone page `trip.html?t=<code>` renders name, story, days with credits and
  thumbnails. The API Lambda now holds a `Vectors` backend (DynamoDB rows) for the
  cascade.

## 15. US4 self-organising trip (tasks T049 to T058)

- Enrich now runs labels, faces and text through the store's `Vision`, asks for a
  description when the landmark is missing or weak (GPS and labels as hints, answer
  parsed as `Landmark: <name>` plus prose), keeps it only when it names no year or month
  the photo does not carry (constitution IV; places cannot be checked without a
  gazetteer, so a model-named landmark becomes a 0.6-confidence tag), and writes an
  embedding for Ask.
- Embeddings are deterministic: labels, landmark, detected text and description are
  feature-hashed into 128 dimensions (`memorylane_ingest::embed`), and a query is hashed
  the same way. No model call, no network, stable across runs; a learned embedding can
  sit behind the same `Vectors` trait later.
- Organize draws a static route map for the trip and each day through `Maps` and stores
  the keys on the routes; a no-op re-run draws nothing. `GET /trips/{trip}/map?day=`
  presigns them; the TV shows the day map beside the summary.
- Undated photos (no capture time, no GPS) no longer shape clusters: they attach to the
  nearest trip by upload time, land at the end of its last day and are listed in
  `TripDay.undated`; the TV says "N undated".
- Stories: `POST /trips/{trip}/narrative` (the `save_narrative` tool) runs the Rust
  place-and-date check, stores a passing story with one paragraph per day, and stores a
  withheld narrative with a 422 otherwise; the agent calls `withhold_narrative` after its
  own retry fails and the TV shows "being written" for withheld or missing stories.
  `POST /trips/{trip}/narrative/regenerate` clears the story and re-emits `trip.detected`
  through the bus, which the API Lambda can now reach (`Bus` moved to the store crate).
- `POST /media/{media}/keeper` moves a burst group's keeper (owner or co-editor), the
  others inherit its tags, and the embedding index follows. The Highlights screen has a
  "Make keeper" action.
- Slideshow screen: day order, play and pause on the remote, next and previous day, and a
  `NowShowing` callback ready for presence (US5).

## 16. US5 Ask (tasks T059 to T066)

- `POST /search` ranks by the hashed embedding inside a trip or across the household,
  then adds a keyword pass over labels, landmark, text and description for items with no
  vector, drops removed and hidden items, and never calls a model; the 300 ms warm budget
  is a matter of one vector query plus one media listing. *Unmeasured* on the deployed
  stack.
- `POST /playlists`, `GET /playlists`, `GET /playlists/{id}` back the `create_playlist`
  tool. `PUT /presence` records what a TV shows (device, media, trip, day, paused); the
  Slideshow posts it on every change. `GET /presence` lists the fresh records for the
  phone.
- `POST /ask`: a named TV wins, else the freshest presence under ten minutes; with a
  photo on screen the answer comes from the stored description (or, failing that, from
  contributor, date, landmark and labels) with no model call, and lands as an
  `answer.card` on the household feed, which the Slideshow draws over the photo. Without
  a fresh presence the question becomes a search and a playlist, or "nothing" with a
  hint. The event carries no target TV: every TV in the household sees the card.
- Agents: `describe_photo` workflow answers from the stored description; only when it is
  empty does the model speak from the photo's facts, and that answer passes the same
  check as a story before it is pushed. The runtime accepts `{"question", "tv"}`.
- TV: Ask screen with text entry, matches as thumbnails, Play as a slideshow; Ask button
  on Memories home. Phone: `ask.html` with hold-to-talk through the browser's
  SpeechRecognition (Safari and Chrome), a text fallback, the now-showing preview and the
  answer. Speech recognition only runs in a real browser; the helpers around it are
  tested.

## 17. US6 Review Changes (tasks T067 to T072)

- Proposals are records: `POST /proposals` (the `request_confirmation` tool) opens one,
  computes before-and-after counts and up to four sample groups server-side from the
  action (dedup groups, cleanup candidates, media ids), and emits `dedup.proposal` or
  `cleanup.proposal` on the feed so the TV opens Review Changes. `GET /proposals` lists
  open and confirmed ones (expired after 24 hours fall away), `GET /proposals/{id}` is
  what the agent polls for the token, `POST /proposals/{id}/cancel` is "Keep everything".
- `POST /confirmations` may name a proposal: the action must match, the proposal moves to
  confirmed with the token, and the token's redemption marks it applied.
- Tool targets: `POST /media/dedup/apply`, `POST /media/cleanup/apply` and
  `POST /media/delete` (owner token, or a co-editor's RemoveMedia token whose subject holds
  the role) all refuse without a valid token and never accept one twice (F-504);
  `GET /media/cleanup` lists candidates; `POST /media/{media}/restore` undoes a removal
  inside 30 days; `POST /media/purge` (the `purge_expired` tool, nightly) deletes
  originals, renditions, vectors and records past the window.
- Cleanup candidates are deterministic (constitution V): phone-shaped frames with no
  capture time are screenshots, eight or more text lines a document, receipt words a
  receipt, sharpness under 20 blurry. Enrich flags keepers and publishes
  `cleanup.candidates` on EventBridge only; the Concierge's `quick_cleanup` workflow turns
  them into one proposal. The nightly tick now also purges.
- TV: Review Changes lists live proposals with counts, four sample groups with reasons and
  a "Show all", Keep everything and Apply; Memories home shows a Review changes button
  with a count from the feed. Other proposal kinds (share, remove contributor, co-editor
  removals) emit no feed event yet and rely on that button.

## 18. US7 join from another TV (tasks T073 to T080)

- `POST /memberships {code}` joins a trip-scoped code from another household's TV: the
  member gets a pointer, the trip records the member, the owner's ticker says who joined.
  Own trips and household codes are refused. `DELETE /memberships/{trip}` leaves.
  `GET /trips` on the member's TV lists joined trips with `shared_by`, and
  `GET /trips/{trip}` serves them through `trip_access`.
- Feed fan-out at write time (`memorylane_store::feed::emit`): events about a trip that
  members may see (contribution.received, memory.ready, reaction.received, media.removed)
  are copied to each active member's feed, so a member TV's long-poll needs no second
  cursor. Both Lambdas emit through it.
- Pins and hides are per household (F-163): a member's choices go to `pinned_by` and
  `hidden_by` on the owner's media record, shape only that member's highlights and days,
  and never touch the owner's flags. Keeper changes on a joined trip are refused.
- `POST /trips/{trip}/roles` promotes or demotes a contributor (a guest never becomes a
  co-editor, F-165); `PATCH /trips/{trip}` renames for owner or co-editor;
  `contributor_roles` rides on the trip view. A test walks every edit route with a guest
  token and expects 403, and the session-free web view gets 401.
- TV: Join a trip screen (code grid or on-screen keyboard, preview, Join), "Shared by"
  badges with Leave on Memories home, Promote to co-editor on the trip's contributor
  list, and no management actions on a joined trip. Phone: `link.html?join=<code>` makes
  the phone's linked TV join.

## 19. US8 memories (tasks T081 to T085)

- `memorylane_trips::memories`: On this day (same month and day, earlier years), Then and
  now (same 500 m place, best photo from the earliest and latest year, at least a year
  apart, one pair per roughly 1 km cell) and best of a month or year (one per burst),
  all keepers only, hidden and removed excluded, ranked by the pipeline's score. Pure
  functions, unit-tested.
- `GET /memories/home?date=` serves those sets plus last month's and last year's best;
  `GET /map` draws one pin per trip with a route through `Maps`, cached in S3 under a key
  that names the number of trips, and returns the pins for labels.
- The nightly tick now runs `memories_nightly` between household patterns and the purge:
  On this day and best-of become playlists and `memories.ready` goes to the feed on days
  that have something.
- TV: Memories home gains an On this day row with Play and a Then and now card, a Map
  screen with the pins list, and the slideshow drifts and zooms each slide (Ken Burns),
  held still while paused so the paused photo is the one a question refers to.

## 20. US9 People (tasks T086 to T091)

- Opt-in per household: `POST /people/enable` creates the Rekognition collection and sets
  the flag; `POST /people/disable` deletes the collection, every `Person` record and the
  people links on media. Nothing about faces is indexed while the flag is off (checked
  at enrich, keepers only, research R9).
- Enrich searches the collection first, then indexes the keeper's faces; a match joins
  the existing person, otherwise an unnamed person opens. `MediaItem.people` records who
  is on each item. `GET /people` lists live clusters, `PATCH /people/{id}` names or
  merges (media re-pointed to the target).
- `POST /search` takes `people` names, resolved through merges; a photo must carry every
  named person. An unknown name is a 404 rather than an empty result.
- `DELETE /me` (F-604) marks the household deleting so `POST /uploads/init` answers 410,
  then deletes originals, renditions, vectors, the face collection, people and every
  record through the new `Storage::purge_household`; the household row stays as a
  tombstone. DynamoDB purge queries the partition and deletes row by row plus the code
  and share-link rows keyed elsewhere; compile-checked only.
- Phone People page with consent copy that states what is stored and that off deletes
  it, naming and merging, and My uploads with Remove. TV People screen with clusters,
  best-of for a person, Play, and Turn off.

## 21. US10 reactions (tasks T092 to T094)

- `POST /trips/{trip}/reactions` takes likes and comments (200 characters at most) from
  three kinds of actor: a contributor's upload session, a member household or owner TV
  session, and the read-only web viewer by trip code with a name of 2 to 40 characters,
  who becomes `viewer:<name>` so one like per person per target still holds. Every
  reaction goes to the owner's feed as `reaction.received` and fans out to member TVs.
  `GET /trips/{trip}/reactions` returns the list plus per-target counts and the latest
  comment.
- Phone: the read-only page asks for a name once, keeps it on the phone, and offers Like
  on each day and photo and a comment box per day, with counts and the latest comment.
- TV: the ticker reads "Grandma Ruth loved day 3" or "Grandma Ruth: That ramen!"; the
  trip view shows counts beside the day and each photo and the latest comment, and
  refetches whenever a reaction lands on the feed.

## 22. US11 ambient and narrated slideshow (tasks T095 to T097)

- Ambient (F-406): five minutes of stillness on Memories home opens the Concierge's
  "Ambient" playlist (W3 Household Patterns) as a slower slideshow; any key returns home.
  `GET /playlists/{id}` now returns this household's pinned items first and drops hidden
  and removed ones, so the TV never sees a `hidden_by` item. No playlist yet shows a
  one-line explanation instead of a black screen.
- Narration (F-709, research R10) is behind `Household.narration_enabled`, flipped by
  `PATCH /me`. `POST /trips/{trip}/narration` (the `synthesize_narration` tool) runs one
  day's story through `Speech` (Polly neural, voice Joanna) into
  `narration/{household}/{trip}/day-{n}.mp3` and emits `narration.ready`; 409 while the
  flag is off or the day has no story. `GET /trips/{trip}/narration?day=` presigns it for
  any trip member. W1 Trip Curator calls the tool per day after `save_narrative` only when
  the flag is on. Polly path compile-checked only.
- Playback: the shared UI takes an optional `AudioPlayer`; the Slideshow always offers the
  story as captions (Captions button) and plays audio only when a player exists. The Vega
  shell's `audio.kepler.ts` looks for a system-deployed W3C `Audio` constructor at runtime
  on the `react-native` export or the global, never bundling
  `@amazon-devices/react-native-w3cmedia`; the plain `audio.ts` returns null so tooling
  and other platforms get captions. Virtual Device result: not run, the Vega SDK is not on
  this machine, so whether the port is system-deployed stays open (R10 unverified). The
  flag defaults to off per household regardless.

## 23. US12 sharing (tasks T098 to T101)

- `POST /shares` publishes a playlist, not a loose media list: the confirmation token is
  bound to `ShareAlbum(album_id = playlist id)`, and a list of ids has nothing stable to
  bind to. The `share_album` tool therefore takes `playlist` (from `create_playlist` or
  the Ask playlist) plus `media` and `name` as description. Links last seven days, list
  under `GET /shares`, and `DELETE /shares/{id}` revokes early.
- Public `GET /s/{share}` needs no session: the id is the key. It returns the playlist's
  live, unhidden items with presigned TV renditions, 404 for an unknown id and 410 once
  expired or revoked so the phone page can say which. The link itself is
  `{upload_base}/s.html?s={id}`.
- Email goes through a `Mailer` trait (SES v2, one verified sender from the stack's
  `shareSender`); without a sender the API answers 409 to an email request and still
  returns the link. The IAM grant is limited to that identity and `ses:FromAddress`. SES
  path compile-checked only; the identity needs its verification mail answered before the
  first send.
- W5 share: a recipient remembered in agent memory under `contacts` with a `household`
  gets the trip's join code (`get_trip_code`, new read tool) and the TV shows it as a
  `share.join_code` event; no link, no confirmation. A contact with only an `email`
  gets the link mailed. The trip is matched by a shared word of four letters or more
  between the request and a trip name, so "send the Kyoto trip to grandma" finds
  "Kyoto, April 2026". `share` now returns a `ShareResult` instead of a bare link.
