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
  Photos. Only an unofficial cookie-based scraper exists, against the terms. MemoryLane
  treats Amazon Photos as closed: camera roll in, nothing pulled.
- **2026-09-08, device sign-in.** Cognito has no OAuth device grant. The TV shows a code,
  the phone approves it, the TV polls; a small custom flow that every TV app rebuilds.
- **2026-09-09, Alexa+ preview.** The add-on console is behind a preview we do not have,
  so the OAuth metadata, PKCE client and scope are built from the documentation and the
  redirect URI is a placeholder (`docs/alexa-plus.md`).

## AWS

- **2026-09-09, AgentCore CloudFormation.** `AWS::BedrockAgentCore::Policy` takes Cedar
  text and the action namespace is `AgentCore::Action::"api___<tool>"` by convention in the
  docs; no synth-time validation, so a typo only shows at deploy. Compile-checked only.
- **2026-09-09, S3 Vectors regions.** Not available everywhere; the `Vectors` trait keeps
  a DynamoDB implementation for the hackathon region.
- **2026-09-09, SES sender.** Emailed share links need a verified identity; the first
  deploy sends a verification mail that has to be answered before the first send works.
- **2026-09-09, Polly and Rekognition offline.** Neither has a local emulator, so every
  AWS-backed trait has an in-memory fake and the real implementations are compile-checked
  until deploy.

## Open data

- **2026-09-10, Wikipedia geosearch.** Good for landmarks, lochs, villages and mountains
  along a route, with `type` and `dim` hints, but no type filter and a 10 km radius cap:
  in a dense centre the nearest 50 articles are all buildings, so "Oxford" never comes
  back for a walk down Broad Street. Amazon Location's reverse geocode is the answer
  once deployed; until then day names in cities come from a college or a memorial.

- **2026-09-12, no local drop.** Fire TV has nothing like AirDrop. DIAL only launches an
  app with a small payload and needs a native second-screen client (UDP multicast, out
  of reach for a web page); Matter Casting is for video; the Vega app has no listening
  socket and no external storage, so a USB drive or a phone on the same Wi-Fi cannot hand
  bytes to the TV. Uploads go through S3, and the phone side shortens the path instead: a
  Web Share Target so the page sits in the Android share sheet, and a remembered trip so
  a return visit skips the QR. iOS Safari has no share target, so iPhones keep the QR.

## What would help most

1. One compatibility page per Vega SDK version: React Native pin, system-deployed ports,
   CLI-only install.
2. An Amazon Photos read API or MCP tool.
3. A Cognito device-code grant.
4. A local path from phone to Fire TV: a DIAL payload a web page could send, or a
   share-sheet target on Fire OS that a Vega app can receive.
