# Friction log

What slowed us down building for Fire TV, Vega OS, Alexa+ and AWS, kept from day one for
the hackathon's friction-log bonus. Each entry says what we expected, what happened, and
what would have helped. Dates are when we hit it.

## Vega OS and the SDK

- **2026-09-08, React Native version.** Expected to use the current React Native. Vega
  pins the version: 0.72 on the released SDK, 0.83 as early access on SDK 0.24. Our app was
  on 0.87 and had to split into a shared `packages/tv-ui` and a thin `apps/tv-vega` shell.
  A published compatibility table with the pin per SDK version, one row, would have saved
  a day.
- **2026-09-09, Metro resolver.** Adding a `resolver.resolveRequest` to the shell's Metro
  config silently replaced the React Native CLI resolver that maps `react-native` to
  `@amazon-devices/react-native-kepler`; the bundle then pulled upstream React Native and
  failed on `ReactDevToolsSettingsManager`. A warning from `bundle-vega` when the resolver
  is overridden would make this a five-minute fix instead of an afternoon.
- **2026-09-09, Babel runtime.** `@babel/runtime` 8 drops the `regenerator` helper the
  Vega Babel preset still emits; pinning to 7.x fixed it. The preset could declare a peer
  range.
- **2026-09-09, jest preset.** `@react-native/jest-preset@0.83` does not exist on npm;
  the shared UI tests use the plain `react-native` preset against upstream 0.83.10.
- **2026-09-09, no SDK on CI.** The `.vpkg` build, install and every Virtual Device check
  need the Vega SDK on the machine; `bundle-vega` runs without it, so CI proves the bundle
  and nothing after it. A downloadable CLI-only package, or a hosted build, would let the
  packaging step run in GitHub Actions.
- **2026-09-09, system-deployed ports.** The ported-library table lists AsyncStorage as a
  system module (exported from `react-native` on kepler 4) but not `react-native-w3cmedia`,
  so narration audio is behind a flag and looks for an `Audio` constructor at runtime.
  A single "what is on the device" manifest per OS version would remove the guessing.
- **2026-09-14, session storage.** The legacy `AsyncStorage` export on react-native-kepler
  logs "Application data root path is not set" on the Virtual Device, and the TV session
  did not survive a terminate-and-relaunch. The system-deployed port
  `@amazon-devices/react-native-async-storage__async-storage` 2.1.9000000001 works there:
  terminate and relaunch comes back signed in. Its `-rn-83` prerelease fails the SDK's
  kepler-compatibility check. If the legacy export is not meant to persist, the
  ported-library table could point to the port instead.
- **2026-09-15, transitive system modules.** `@amazon-devices/react-native-w3cmedia`
  depends on `keplermediadescriptor` and `kepler-media-controls`, both system turbo modules,
  but `build-vega` only writes `needs.module` entries for the app's direct dependencies. The
  app installed fine and crashed on first render with `'KeplerMediaDescriptorTurboModule'
  could not be found`. Adding both as direct dependencies fixed it. The packager could walk
  the dependency graph, or the docs could list the full manifest a W3C media app needs.
- **2026-09-15, rn83 port versions.** `react-native-w3cmedia@2.3.4-rn-83` (dist-tag
  `rn83-alpha`) is not in its own `kepler-compatibility.json`; `build-vega` prints a
  validation ERROR and continues, and the `2.3.4` row (`IW3cmedia_3`) works on the SDK 0.24
  Virtual Device. The prerelease rows should be in the file.
- **2026-09-15, hls.js on Hermes.** The Vega hls.js 1.5.11 patch set plays VOD, but live
  playlists never load a fragment: hls.js calls `clearTimeout(-1)` on its reload timer and
  Hermes on Vega throws "clearTimeout called with an invalid handle", which kills the
  playlist loop. A tolerant `clearTimeout`, either in the runtime or in the shipped
  polyfills, would make the patch set cover live streams.
- **2026-09-15, no camera, no WebRTC, no `getUserMedia`.** Explored a video-call feature.
  Vega exposes a camera privilege id but no camera library, no WebRTC, and the WebView
  returns `NotSupportedError` for `getUserMedia` (Amazon confirmed on the forum,
  2026-07-24). Raw PCM microphone capture exists. The only reachable shape is phones as
  cameras and the TV as a delayed screen; measured 7.9 s glass-to-glass over IVS standard
  HLS with tuned hls.js, 30 s with hls.js defaults (`specs/video-call-feasibility-2026-09-15.md`).
  `getUserMedia` in the WebView would unlock the whole use case.
- **2026-09-09, screenshots.** `gwsi-tool-screenshooter` hangs on the Virtual Device
  (physical devices only) and a QEMU QMP `screendump` is black because rendering goes
  through host GL. The Virtual Device turns out to be an Android-emulator fork, and its
  console on port 5554 (`screenrecord screenshot`) gives a host-rendered PNG. A `vega
  virtual-device screenshot` command would save the archaeology.
- **2026-09-09, JavaScript logs.** `console.log` from the app reaches neither the device
  syslog nor the Metro terminal on React Native 0.83 (DevTools only), so an agent driving
  the Virtual Device has no text channel for app state; screenshots are the only signal.
- **2026-09-09, first device run.** Good news: with the SDK installed, the `.vpkg` built,
  installed and launched on the first attempt, and Fast Refresh worked with the documented
  three-terminal setup.
- **2026-09-16, the window is 960x540 points.** Rebuilt every screen from a 1920x1080
  design and it came up at twice the size on the Virtual Device: React Native on Vega
  reports the window in points, 960x540 on a 1080p set, like Android TV at 2x. Nothing in
  the getting-started docs says so, and the old screens had simply been authored small.
  Fix was one component: a root `Stage` laid out at 1920x1080 with `transform: scale` and
  `transformOrigin: "top left"` from `useWindowDimensions`, so the design numbers stay the
  design numbers and a 4K set scales the other way. Text stays crisp under the transform.
  Also learned: a horizontal `ScrollView` inside a column grows to fill it unless it gets
  `flexGrow: 0`, which left a 400px hole under the day chips until it did.
- **2026-09-21, live HLS on the W3C media port.** On the Virtual Device, hls.js with Amazon's
  Vega patches needed two more fixes for IVS live after the `clearTimeout` guard. The TV
  hears that a broadcast has started before IVS has a playlist, and hls.js does not retry
  the 404; rebuilding hls.js on each retry re-attached MediaSource, which the Vega media
  element does not survive, so the player now polls the playlist and attaches once. hls.js
  also forces HE-AAC for any user agent that is neither Firefox nor Android (a Chrome
  workaround); IVS sends AAC-LC, and the resulting `changeType` is an error on Vega.
  Appending "Android" to the polyfilled user agent (`AFTCA001`) keeps AAC-LC. The Vega
  patch set could carry the user-agent fix, and the media docs could say whether a media
  element takes a second MediaSource.
- **2026-09-21, media service wedges on the Virtual Device.** After a string of failed live
  sessions, every later session, the known-good spike included, froze on its first seek
  with `novaaudiosink ... InvalidArgumentError: Bad Value`, even on a video-only stream.
  `vega virtual-device stop` and `start` cleared it. An earlier screenshot that afternoon
  had been a frozen frame, so playback is now judged by two screenshots, not one. On
  2026-09-22 the pipeline wedged another way, hls.js reloading the playlist while the
  buffer sat still; the TV now rebuilds the player when the picture stops. Trip reels later
  pick only music whose first bar falls within 4 s, so the TV never seeks (2026-10-16).
- **2026-09-22, developer shell dies on the Virtual Device.** Twice, starting live video
  took the emulator's developer shell service with it: the screen went black,
  `vega device list` said "No devices found" while the emulator console still answered
  screenshots, and every shell command failed with "Invalid component id:
  com.amazon.dev.shell.service". On 2026-10-09 it died again, with "failed to get running
  components for com.amazon.dev.shell.service". Only `vega virtual-device stop` then
  `start` recovers it, and that wipes the app's stored session, so the household is
  abandoned and its phone has to pair again. Relaunching with `vega run-app` wiped the TV
  session too, whereas a terminate and relaunch keeps it. Live has never failed this way on
  anything but the Virtual Device; a real Fire TV is untested. A restart that keeps app
  data, or a command that restarts the shell service alone, would keep a test household
  across a crash.
- **2026-10-09, preferred focus.** Expected a true `hasTVPreferredFocus` to place focus
  once. On Vega it is re-applied on every re-render, so whenever the slideshow re-rendered
  (its controls fading, for one) focus jumped back to Pause, and Select on a field-note map
  pressed Play instead of opening the map. Fixed with an edge-triggered hook that is true
  for one render when the preference turns true (`usePreferOnce`); a button that appears
  later, such as a pairing request's Allow, still takes focus. A Back button flickering on
  the Live screen (2026-09-22) is probably the same cause; not chased. If this is intended,
  the focus-management guide could say so.
- **2026-10-09, LogBox takes focus.** In debug builds on the Virtual Device, React Native's
  LogBox banner ("2 warnings") takes D-pad focus from the app, so the remote could not
  reliably reach a given slide; with the shell crash above, it kept a new detail screen
  from being checked on the device. The media port adds noise of its own: a "forwardRef
  render functions accept exactly two parameters" console error from inside the video
  surface (2026-09-21), and React warnings the app now hides. A LogBox banner that never
  takes remote focus would keep debug builds drivable.
- **2026-10-10, Back leaves the app.** On the Virtual Device, Back from Trip view (twice) and
  Back on a playing reel (2026-10-17) left the app for the launcher instead of going back a
  screen. Not chased, so it may be our own Back handling.
- **2026-10-17, audio clock stops on the Virtual Device.** The Vega `AudioPlayer` loads a
  presigned AAC-LC `.m4a` in URL mode (`canplay` fires) and reports Playing, but
  `currentTime` stops near 0.5 s ("No Time update even in Playing state"), with the
  device's AudioSystemService refusing IPC at the same time. Trip reels fall back to silent
  playback. Whether the music plays on a real Fire TV is open.
- **2026-10-17, Virtual Device speed and network.** The emulator ran at about a sixth of
  real time all session. A day film through the `VideoPlayer` in URL mode linked the H.264
  decoder and showed its first frame, then crawled with late frames; on 2026-10-09 each TV
  rendition took 10 to 20 s to appear, longer than a slide. The emulator's network also
  dropped (curl errors 7 and 55, no API requests from the TV after 10:45 UTC), so starting
  a film from the TV went unverified. None of it says anything about speed on a stick.

## Amazon ecosystem

- **2026-09-08, Amazon Photos.** No public API, so photos come from the phone's camera
  roll through the browser file picker. An Amazon Photos MCP tool, or an LWA scope such
  as `photos:read`, is the single biggest gap for memory apps on Fire TV and Alexa+.
- **2026-09-12, Amazon Photos, checked.** No public API, developer program, plugin, LWA
  scope, MCP tool or Fire TV hook for third parties. Amazon Drive API access ended for
  third parties on 2020-11-01 and the service closed on 2023-12-31; its docs now return
  404. Amazon staff on the developer forum, 2026-03-16: "there's no direct successor to
  Amazon Drive" (community.amazondeveloper.com/t/amazon-drive-api-documentation/27885).
  LWA scopes are `profile`, `profile:user_id`, `postal_code` only. The Fire TV Ambient
  Experience and the Vega screensaver have no public API (staff, 2025-01-15 and
  2026-07-07). The 2026-05 Amazon Photos redesign added Alexa+ voice search, first party
  only; the Alexa+ add-on categories and the hackathon resources page do not mention
  Photos. Only an unofficial cookie-based scraper exists, against the terms. Mantelpiece
  treats Amazon Photos as closed: camera roll in, nothing pulled.
- **2026-09-08, device sign-in.** Cognito has no OAuth device grant. The TV shows a code,
  the phone approves it, the TV polls; a small custom flow that every TV app rebuilds.
- **2026-09-09, Alexa+ preview.** The add-on console is behind a preview we do not have,
  so the OAuth metadata, PKCE client and scope are built from the documentation and the
  redirect URI is a placeholder (`docs/alexa-plus.md`). Still not granted on 2026-10-21, so
  the add-on was never registered and the 500 ms per-call budget is unmeasured.
- **2026-09-12, no local drop.** Fire TV has nothing like AirDrop. DIAL only launches an
  app with a small payload and needs a native second-screen client (UDP multicast, out
  of reach for a web page); Matter Casting is for video; the Vega app has no listening
  socket and no external storage, so a USB drive or a phone on the same Wi-Fi cannot hand
  bytes to the TV. Uploads go through S3, and the phone side shortens the path instead: a
  Web Share Target so the page sits in the Android share sheet, and a remembered trip so
  a return visit skips the QR. iOS Safari has no share target, so iPhones keep the QR.
- **2026-09-15, Alexa calling.** Looked at video calls between TVs. Alexa's own two-way
  video calling works on a Fire TV Cube or Omni with a webcam, but there is no third-party
  API to place or join those calls from an app, and the Alexa Smart Properties WebRTC
  calling API is for hospitality and senior-living deployments only. From the docs, not
  tried.

## AWS

- **2026-09-09, AgentCore CloudFormation.** `AWS::BedrockAgentCore::Policy` takes Cedar
  text and the action namespace is `AgentCore::Action::"api___<tool>"` by convention in the
  docs; no synth-time validation, so a typo only shows at deploy. Compile-checked only at
  the time; the first deploy turned up six rules (2026-09-14, below).
- **2026-09-09, S3 Vectors regions.** Not available everywhere; the `Vectors` trait keeps
  a DynamoDB implementation for the hackathon region.
- **2026-09-09, SES sender.** Emailed share links need a verified identity; the first
  deploy sends a verification mail that has to be answered before the first send works.
- **2026-09-09, Polly and Rekognition offline.** Neither has a local emulator, so every
  AWS-backed trait has an in-memory fake and the real implementations are compile-checked
  until deploy.
- **2026-09-14, AgentCore Policy, first deploy.** The stack came up in us-west-2 on the
  ninth deploy. All eight failures were AgentCore Policy rules, learned one failed round at
  a time, and each rollback orphaned the RETAIN table and user pool, which the next deploy
  does not reattach; they were pruned by hand.
  - One Cedar statement per `AWS::BedrockAgentCore::Policy`; a file with two rules fails
    with "unexpected token `permit`".
  - No wildcard resource. An untargeted rule says `resource is AgentCore::Gateway`; a rule
    that names actions must name the Gateway, `resource == AgentCore::Gateway::"<arn>"`.
  - A permit-everything rule is rejected as "Overly Permissive". The permit names
    `principal is AgentCore::OAuthUser`, every action explicitly, and this Gateway.
  - The Gateway's execution role needs `bedrock-agentcore:GetPolicyEngine`,
    `AuthorizeAction` and `PartiallyAuthorizeActions`, and the Gateway must depend on that
    policy explicitly (a separate `iam.Policy`; hanging it off the invoke grant makes a
    cycle through the API Lambda's `GATEWAY_URL`).
  - Policies are validated against the Gateway's registered actions, so each Policy
    depends on the GatewayTarget; before the target exists only
    `x_amz_bedrock_agentcore_search` is known and `api___remove_contributor` is
    "unrecognized".
  - A forbid with an unscoped `principal` is "Overly Restrictive": it covers
    `AgentCore::IamEntity`, which nothing permits, so the engine would deny that principal
    outright. Forbids say `principal is AgentCore::OAuthUser` and come after the permit.
  Synth-time validation of these rules, or one docs page listing them, would let a stack
  find them before CloudFormation does.
- **2026-09-14, Bedrock on a fresh account.** Bedrock answered "Your account is currently
  being verified" for about an hour, so the first Organize run failed. It was redriven
  (`aws stepfunctions redrive-execution`) once the hold lifted and succeeded.
- **2026-09-15, IVS playlist and hls.js.** IVS low-latency channels serve 2 s MPEG-TS
  segments but advertise `EXT-X-TARGETDURATION:6`, and hls.js's default live sync counts
  target durations, so stock hls.js aims 18 s back and sat 30.6 s behind on the Virtual
  Device. A seconds-based `liveSyncDuration: 3` brought it to 7.9 s. The playlist has no
  LL-HLS parts, so `lowLatencyMode` has nothing to use. From a real phone's camera through
  the IVS web broadcast SDK the picture reached the TV about 4 s behind (2026-09-22).
- **2026-09-21, AgentCore Gateway and Runtime, first real deploy.** What worked first
  time: a Runtime from a code package (no container) through CloudFormation, a Lambda
  target, Cedar in ENFORCE mode, and a client-credentials Cognito client as the agent's
  identity. What cost time:
  - A Lambda target never learns who called. The documented event is the tool arguments
    and the client context carries tool and gateway ids, nothing about the caller. We made
    the same binary the Gateway's REQUEST interceptor with `passRequestHeaders`, sealed
    the caller into the arguments and verified the seal in the target. A `callerIdentity`
    block in the client context would remove all of that.
  - The interceptor cannot be the target function: the target needs the Gateway URL, the
    Gateway needs the interceptor ARN, CloudFormation sees a cycle. Two functions, one
    binary.
  - The Gateway answers `Unsupported protocol version: 2025-03-26` when the
    `mcp-protocol-version` header is absent, although the gateway was created with
    `2025-11-25` only. Defaulting to the gateway's single supported version would be kinder.
  - EventBridge has no AgentCore Runtime target, so every event-driven agent needs a
    forwarding Lambda.
  - An Anthropic model fails at the first call with "Model use case details have not been
    submitted", from inside the Runtime, as a 500 to the invoker. Amazon Nova Pro needed
    nothing, so it is the default here.
- **2026-09-21, IVS channel lifecycle.** The web broadcast SDK does not report the
  disconnect when the TV ends a broadcast with `StopStream`, so the live phone checks
  `GET /live` every 5 s and stops itself. IVS refuses to delete a channel whose stream is
  still winding down; the API parks it and sweeps it on the household's next start or
  stop, and `make down` deletes every channel tagged `mantelpiece:household`. A recording
  is filed under the channel id alone and the channel is gone once the broadcast ends, so
  the API leaves a claim object under the channel id to tell the recording Lambda whose it
  is (2026-09-23, not verified on AWS). A disconnect event in the broadcast SDK, and a
  delete that IVS finishes on its own, would remove the polling and the sweep.
- **2026-10-09, Amazon Location storage terms.** AWS Service Terms section 82.4 bars
  storing or caching map tiles or other Amazon Location content, with exceptions for
  routes, geocodes and legal holds, and bars combining it with open-licensed data. Our trip
  and day route maps were `GetStaticMap` images kept in S3, which section 82.4 appears to
  forbid; we found it while reading the terms for a new map feature. Field notes and the
  route maps are now drawn in Rust from open data (below), and organize no longer calls
  Amazon Location for route maps. A static map that may be kept, with attribution, would
  have kept Amazon Location in the app.
- **2026-10-09, Nova subject boxes.** Field notes ask Amazon Nova Lite, through Bedrock
  Converse, for a highlight's main subjects as boxes on a 0 to 1000 grid. On the deployed
  stack it answered `[]` for the Lóndrangar cliffs and, for a geyser,
  `[{"label": "waterfall", "box": [0, 580, 749, 1000]], ["label": "flowers", [...]]]`,
  which a JSON parser rejects whole. An example label in the format line ("cliff") made
  every answer a cliff. Nova Pro found the right subjects more often but also boxed "sky"
  and "mountain" over most of the frame. Converse tool use, to force JSON, failed with
  "Model produced invalid sequence as part of ToolUse" on about half the calls, on both
  Lite and Pro. Kept Nova Lite with a prompt that names landscape subjects and the trip
  landmark as a hint, a parser that reads each `"label"` with the four numbers in the next
  brackets, and background labels such as sky and water dropped; the same 7 notes came
  back for a second household at temperature 0. Tool use that held to its schema would
  have made the custom parser unnecessary.
- **2026-10-16, CDK test synths fill the disk.** Without an `outdir`, every CDK `App`
  stages its assets into a new directory under `$TMPDIR` and never removes it. With the
  ffmpeg layer and the music library present, each synth in the stack tests copied about
  70 MB, and on 2026-10-16 they filled the disk. The tests now synth under one temporary
  root that is deleted when the file finishes. An `App` that removed the directory it made
  would avoid it.
- **2026-10-17, CDK asset upload on a slow link.** `make up` (CDK deploy) failed before
  CloudFormation started: eight parallel asset uploads to S3 on a slow, lossy uplink ended
  in `write EPIPE`, the film Lambda's 14 MB code and 14 MB music among them.
  `--asset-parallelism=false` (one upload at a time) went through. Retrying a failed asset
  upload, or falling back to one at a time, would let the default go through.

## Open data

- **2026-09-10, Wikipedia geosearch.** Good for landmarks, lochs, villages and mountains
  along a route, with `type` and `dim` hints, but no type filter and a 10 km radius cap:
  in a dense centre the nearest 50 articles are all buildings, so "Oxford" never comes
  back for a walk down Broad Street. Amazon Location's reverse geocode is the answer
  once deployed; until then day names in cities come from a college or a memorial.
- **2026-10-09, OpenStreetMap and terrain tiles.** Field notes and the trip and day maps
  are drawn from OpenFreeMap vector tiles (OpenMapTiles schema, no key) and AWS Open Data
  Terrain Tiles, contoured in Rust, and kept with credit. In the offline spike all 48
  Iceland test photos got a note; the first fetch took 7 to 29 s per photo, a note from
  cached tiles 0.3 s. The terrain tiles carry the seabed, so pencil relief covered the
  ocean until hatching stopped at sea level (2026-10-10).

## What would help most

1. One compatibility page per Vega SDK version: React Native pin, system-deployed ports,
   CLI-only install.
2. A Virtual Device that can restart its developer shell and media service without wiping
   app data.
3. An Amazon Photos read API or MCP tool.
4. A Cognito device-code grant.
5. AgentCore: Policy rules checked at synth time, caller identity (subject, client id,
   scopes) in the client context of a Gateway Lambda target, and an EventBridge target for
   AgentCore Runtime.
6. A local path from phone to Fire TV: a DIAL payload a web page could send, or a
   share-sheet target on Fire OS that a Vega app can receive.
