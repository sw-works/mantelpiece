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
