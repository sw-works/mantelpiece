# Feasibility: FaceTime-style multi-TV video calling on Fire TV

Status: Research note, no code
Created: 2026-09-15
Question: can a Fire TV app let people at several TVs / locations join one live video call?
Method: Amazon Devices Builder Tools doc search (Vega 0.21 to 0.24), Amazon developer
community threads, public press on Fire TV hardware, AWS IVS / Chime SDK docs. Nothing here
was verified on a device. Claims marked *unverified* come from documentation or forum posts.

## 1. Short answer

| Design | Vega OS (Stick 4K Select, Stick HD) | Fire OS (Cube 2nd/3rd gen, Omni TVs) |
|---|---|---|
| A. TV camera + TV mic, real-time both ways (true FaceTime) | **Not possible today.** No camera hardware, no camera API, no WebRTC, WebView `getUserMedia` unsupported | **Plausible**, USB webcam + Android WebRTC, native modules required, small device reach |
| B. Phone is camera + mic, TV shows the composited call | **Feasible** with IVS Real-Time + server-side composition + HLS on MSE. TV picture lags 3 to 5 s | Same, or A |
| C. B plus TV microphone as the room mic | Spike only. Raw PCM capture exists, no echo cancellation, needs a server-side WebRTC bot | Same as A |

Vega is the primary target for this repo and the platform Amazon is pushing, so A is off the
table for the hackathon. B is the only design that reaches every Fire TV and every household.

## 2. What Vega OS gives an app today

Facts from the Vega docs and Amazon staff replies on the developer community:

- **Camera.** A privilege ID exists, `com.amazon.camera.privilege.access` (PermissionsKepler
  `PERMISSIONS.CAMERA`), but no camera library appears anywhere in the Vega API reference
  (0.21 to 0.24). The migration KB maps `android.permission.CAMERA` to the privilege and
  nothing else. No Vega device has a camera or a USB host port: Stick 4K Select and Stick HD
  use USB-C for power only. Amazon staff, 2026, on USB host and BLE GATT: "not part of the
  current developer surface."
- **Microphone.** `@amazon-devices/keplerscript-audio-lib` exposes `AudioRecordStreamBuilder`,
  raw PCM capture, gated by `com.amazon.audio.privilege.microphone.access` as a run-time
  privilege. Amazon staff (2026-04-21): "a native audio capture API (raw PCM), not a WebRTC
  replacement." This corrects the earlier note in `decisions-2026-09-09.md` section 7 that no
  mic API exists; the mic is reachable, only speech recognition is not.
- **WebRTC.** Not in the runtime. No `RTCPeerConnection` in React Native for Vega, no
  react-native-webrtc port (native module, not allowed in the bundle).
- **WebView.** `@amazon-devices/webview` runs Chromium but `navigator.mediaDevices.getUserMedia`
  returns `NotSupportedError: NotSupported` and no permission prompt appears. Amazon staff
  (2026-07-24): "getUserMedia for audio is not yet supported in Vega WebView for 3P apps ...
  our team is actively evaluating microphone support for WebView." Jitsi IFrame and React
  Native SDKs both fail on Vega for this reason. Silk browser fails the same way.
- **Networking.** WebSocket supported on RN for Vega 0.83 (repo memory, verified by docs not
  device). Socket.io is on the supported-libraries list.
- **Playback.** W3C MSE/EME. Live HLS and DASH through MSE mode with hls.js, Shaka or dash.js
  (Amazon publishes Vega patches for all three). Native MPEG-TS parser exists behind MSE.
  Latency figures for live playback on Vega are not documented.
- **Alexa Communications.** "Alexa, call Mom" works on Cube and Omni with a webcam, but there
  is no third-party API to place or join those calls from an app. The Alexa Smart Properties
  WebRTC calling API is for hospitality and senior-living deployments, not consumer apps.

## 3. What Fire OS gives an app today

- Fire TV Cube 2nd gen (2020 update), Cube 3rd gen (USB-A port) and Fire TV Omni / Omni QLED
  TVs accept a UVC webcam (720p30 minimum, 1080p recommended, no 4K). Fire OS 7 on Cube 3rd
  gen, Fire OS 7 or 8 on Omni. Sticks and 4K Max have no USB host.
- Zoom ships a Fire TV app for exactly those devices, so the camera does reach a third-party
  Android app. One developer forum thread (now offline, seen via search snippet) reports USB
  webcam video working in their own app with microphone permission problems. *Unverified.*
- Camera2 and `RECORD_AUDIO` follow normal Android rules; Fire OS 8 adds one-time permissions.
- WebRTC: react-native-webrtc, Amazon Chime SDK for Android, or the IVS Real-Time Android SDK
  all run on Android 9/11. All are native modules, fine in a Fire OS build, not in `tv-vega`.
- Reach: Cube owners with a webcam plus Omni owners with a webcam. A niche.

## 4. Design B in detail: phone is the camera, TV is the screen

Fits the MemoryLane pattern already in the repo: the TV shows a QR, each phone joins through
a browser page, no install.

```
TV app (Vega + Fire OS)                     Phones at each location (browser, WebRTC)
   │ join code / QR                              │ camera + mic publish
   │ hls.js on MSE, muted or low volume          ▼
   ▼                                      IVS Real-Time stage (WebRTC, < 300 ms phone to phone)
IVS channel (HLS, 3 to 5 s)  ◄── StartComposition (server-side grid, HD $0.30/h)
   ▲
   └── EventBridge "Participant Published" triggers composition; auto-stops after 60 s idle
```

- Phones carry the conversation. Latency phone to phone is WebRTC class. People hear each
  other through their phones (or one phone on speaker per room).
- The TV shows the composited grid of everyone, 3 to 5 s behind. It is the big shared
  picture, not the audio path. Lips on the TV will not match the audio from the phones.
  This must be stated in the UI ("TV view runs a few seconds behind") or the demo reads as
  broken.
- Alternative composer: Amazon Chime SDK meeting plus a media live connector pipeline to an
  IVS channel (RTMPS, 720p or 1080p at 30 fps). Same TV path, similar lag, more moving parts.
  IVS Real-Time alone is simpler for a hackathon.
- Both variants use only fetch, WebSocket and MSE on the TV, so the "no native modules" rule
  holds and one `packages/tv-ui` codebase serves Vega and Fire OS.

Lag mitigation, in order of effort:

1. IVS Low-Latency channel with hls.js `lowLatencyMode` on Vega. Docs say "under 3 s" with
   the IVS player, 3 to 5 s typical. Untested on Vega.
2. Own compositor (ffmpeg on Fargate subscribed to the stage via a headless WebRTC client)
   pushing fragmented MP4 over WebSocket straight into a Vega MSE `SourceBuffer`. Sub-second
   on desktop Chromium; Vega's MSE jitter and decoder pipeline are an unknown. One-day spike.
3. Wait for Amazon: WebView `getUserMedia` is "actively evaluated" as of July 2026. If it
   lands with audio and video, the whole call moves into a WebView with the IVS Web SDK, and
   only the camera hardware problem remains.

## 5. Design C: TV microphone as the room mic

Adds `AudioRecordStream` PCM over WebSocket to a server that joins the stage as a bot
participant. Problems: no acoustic echo cancellation on the TV side (WebRTC does this in the
browser; raw PCM does not), so TV speakers plus TV mic echo the remote party unless the TV
stays muted. A privilege prompt, `[[wants.privilege]]` in `manifest.toml`, and the app is
killed if the user later revokes the privilege in Settings. Not for the hackathon window.

## 6. Recommendation

- Do not pitch "FaceTime on Fire TV". Nothing in Vega's public surface supports it, and Amazon
  staff have said so on record for the WebView path.
- If a call feature is wanted for MemoryLane, scope it as "watch this trip together": phones
  carry faces and voices, the TV shows the trip plus a small composited grid of the callers,
  built on IVS Real-Time and server-side composition. Add the lag line to the UI. Budget a
  one-day spike first: hls.js live playback on the Vega Virtual Device, measure glass-to-glass
  lag against the phone.
- Log both gaps in `docs/friction-log.md` as feature requests: (1) camera and WebRTC on Vega,
  (2) `getUserMedia` in `@amazon-devices/webview`. The hackathon scores friction logs.

## 7. Spike result: live HLS on the Vega Virtual Device (2026-09-15)

Setup: IVS low-latency channel (BASIC, us-west-2, deleted afterwards). ffmpeg pushed a
1280x720@30 test pattern over RTMPS with the host's UTC wall clock burned in at millisecond
precision (`apps/tv-vega/spike/`, feed scripts kept out of the repo). The Vega app
played the channel's standard HLS playback URL through hls.js 1.5.11 with Amazon's Vega
patches on `@amazon-devices/react-native-w3cmedia` 2.3.4-rn-83 (MSE mode) and overlaid the
device's own UTC clock; the Virtual Device clock matched the host to the second. Lag is the
difference between the two clocks in one screenshot. Screenshots:
`docs/screens/live-spike-ivs-3s.png`, `docs/screens/live-spike-ivs-stock.png`.

| hls.js live-sync setting | Glass-to-glass lag | hls.js's own `latency` | Notes |
|---|---|---|---|
| `liveSyncDuration: 3` | 7.9 s, 7.9 s | 7.4 s | Two samples 12 s apart, steady |
| `liveSyncDuration: 4` | 9.3 s, 9.4 s | 7.9 s to 8.0 s | Two samples, steady |
| stock (`liveSyncDurationCount: 3`) | 30.6 s | not reported | 20 s of buffer ahead |
| `liveSyncDuration: 2.5` | never started in 47 s | | see caveat |
| `liveSyncDuration: 4` + `maxLiveSyncPlaybackRate: 1.5` | never started in 52 s | | see caveat |

Why stock is 30 s: IVS serves 2 s MPEG-TS segments but advertises `EXT-X-TARGETDURATION:6`,
and hls.js's count-based sync multiplies the target duration, so it aims 18 s back and
buffers from there. Seconds-based `liveSyncDuration` fixes it. No LL-HLS parts in the IVS
playlist, so `lowLatencyMode` has nothing to use.

Caveat on the two "never started" rows: both were the first mode switched to after a
running player was destroyed and a new one attached to the same VideoPlayer, and the third
switch (stock) did play. A re-attach problem in the spike code is as likely as the config.
Each would need a cold-start run to settle. Not done.

What it took to get W3C media playing from a fetch-only Vega app, all reproducible:

- `@amazon-devices/react-native-w3cmedia@2.3.4-rn-83` plus `@amazon-devices/keplermediadescriptor`
  and `@amazon-devices/kepler-media-controls` as direct dependencies. The packager only
  records `needs.module` for direct dependencies, so without the last two the app crashed on
  start with `TurboModuleRegistry.getEnforcing(...): 'KeplerMediaDescriptorTurboModule'
  could not be found`. Manifest entries in `apps/tv-vega/manifest.toml`.
- `react-native build-vega` reports "Library @amazon-devices/react-native-w3cmedia is not
  properly configured. The version is set to 2.3.4-rn-83 but that value is not part of it's
  'kepler-compatibility.json'" and continues; the 2.3.4 row maps to `IW3cmedia_3`, which the
  SDK 0.24 Virtual Device has.
- hls.js's live reload calls `clearTimeout(-1)`; Hermes on Vega throws "clearTimeout called
  with an invalid handle" and the playlist loop dies before the first fragment. A guard on
  `clearTimeout` and `clearInterval` fixed it (`installPolyfills` in `LiveSpike.tsx`).
- `com.amazon.media.playersession.service` in `[wants]`, otherwise "PlayerService is not
  available" in the log (playback still ran without it; added anyway).
- Metro `sourceExts` needs `mjs` for the patched `hls.mjs`.
- The Amazon hls.js patch set applies cleanly to upstream v1.5.11 with `git apply -3`; the
  `npm run build` output goes to `apps/tv-vega/spike/vendor/dist`, gitignored with the
  Amazon-licensed polyfills.

Reading for the design: the TV picture in design B sits about 8 s behind the room with the
best hls.js setting found, not the 3 to 5 s the IVS marketing number suggests for its own
player. Sub-4 s on the TV needs one of the section 4 mitigations (own compositor over
WebSocket into MSE, or Amazon shipping `getUserMedia` and WebRTC), not more hls.js tuning.

## 8. AirPlay (checked 2026-09-15)

Idea: the iPhone runs the FaceTime call and mirrors its screen to the Fire TV over AirPlay,
so the TV shows the call in real time and the phone stays the camera. Two ways to get an
AirPlay receiver on a Fire TV. Neither is available to a Vega app.

**Native AirPlay 2.** Amazon ships AirPlay 2 and HomeKit on its own TV sets only: Fire TV
Omni, Omni QLED and 4-Series, plus some Toshiba and Insignia Fire TV Edition sets. No stick,
no Cube, no Vega device. On those sets FaceTime-on-the-TV already works with no app at all
(Control Center, Screen Mirroring, then FaceTime; Apple documents this for AirPlay 2 TVs).
Nothing for us to build there, and nothing our app can hook into.

**A receiver inside our app.** An AirPlay receiver has to advertise `_airplay._tcp` and
`_raop._tcp` over mDNS, accept TCP and UDP connections from the phone, run the FairPlay
handshake, receive RTP, and decode H.264 or H.265.

- Vega: none of that is reachable. The React Native for Vega network surface is fetch,
  WebSocket client and Socket.io client. No listening socket, no UDP, no mDNS registration
  for apps. DIAL and Matter Casting exist, but the OS owns the discovery and the app only
  receives a "launch this content" command (Content Launcher cluster) plus media-control
  commands; no media flows over the LAN and there is no mirroring. Not possible on Vega
  today, and it is a platform gap, not an app gap.
- Fire OS: proven by the Appstore. AirScreen, AirReceiver and "AirPlay for Fire TV" are
  receiver apps that run on sticks and Cubes. Open source exists too: `android-airplay-server`
  (GPL-3.0, UxPlay C core over JNI, Android 7+, Android TV, H.264/H.265 mirroring, AAC-ELD
  audio). So a Fire OS build of our app could embed a receiver. Costs: a native Android app
  the repo does not have yet (`apps/tv-vega` and `apps/web` only); GPL-3.0 code cannot ship
  inside the MIT app, so it would be a separate process or a clean-room port; FairPlay is
  reverse-engineered, an Appstore-tolerated grey area that iOS updates break from time to
  time (iOS 18.1.1 reports); mirroring lag on a LAN is in the hundreds of milliseconds by
  reputation, not measured here.

**What it would buy.** On a Fire OS device with an iPhone in the room: a true real-time
FaceTime picture on the TV, phone camera, TV speakers. The TV speakers plus the phone mic
can echo for the far end; Apple TV avoids that with Continuity Camera and we could not.
Android callers get nothing (no AirPlay sender). And it is FaceTime's call, not ours: no
MemoryLane content in the call, no multi-TV join, nothing the Vega primary track can show.

**Verdict.** Not for this project. Vega cannot host a receiver, Fire OS could but only as a
separate GPL Android app outside the hackathon's primary track, and the result is Apple's
feature on Amazon's screen. If a phone-to-TV handoff is wanted, Matter Casting is Amazon's
sanctioned path on both Vega and Fire OS: our phone page cannot use it (needs a native iOS
or Android app with the Matter Casting SDK and a CSA vendor id), so the QR join code stays
the handoff.

## 8a. Chromecast-like casting (checked 2026-09-15)

Two different things hide under "Chromecast":

**The Google Cast protocol.** Phone finds the TV over mDNS (`_googlecast._tcp`), opens a TLS
channel on port 8009, and the receiver app fetches the media itself. Same shape as AirPlay,
same answer: a receiver needs mDNS registration and a listening socket, neither of which a
Vega app has. Google's receiver SDK is also partner-licensed; AirScreen on Fire OS is an
unofficial implementation. Off the table on Vega for the same platform reason as AirPlay.

**The Chromecast experience.** Tap on the phone, the TV plays. Cast's real trick is that the
media never crosses the LAN: the phone sends a URL and a command, the TV fetches from the
cloud. That works on Vega with cloud signalling instead of LAN discovery, and MemoryLane
already has the plumbing: the TV pairs once by QR, the household feed reaches the TV by HTTP
long-poll (`packages/tv-ui/src/hooks/useEvents.ts`, F-407), and `play.requested` already
makes the TV open a trip or playlist (F-405); the owner's trips page already had a
whole-trip "Play on TV" button. Built on 2026-09-15 as F-412: the play request now carries a
`start` photo, the trips page casts a day or a single photo, a member household can cast a
joined trip to its own TVs, and the phone watches `/presence` to confirm which TV switched.
Every TV in the household picks it up, which plain Chromecast cannot do across LANs. Round
trip is one long-poll wake, about a second. AppSync Events over WebSocket would trim it.

What it does not give: screen mirroring or a live camera under a second. Mobile browsers have
no screen capture, and real-time video into the TV is the WebRTC gap from section 2; the IVS
route measured 7.9 s in section 7. Vega sticks do keep the Fire OS "Display Mirroring"
(Miracast) system feature for Windows and older Android senders, not iPhones, and not
something an app can drive.

Sanctioned LAN handoffs, DIAL and Matter Casting, both need a native phone app to send, and
both deliver only a launch-with-payload to our app. The QR plus cloud event does the same job
from a browser.

## 9. Sources

Vega docs and community (read through the Builder Tools MCP):

- PermissionsKepler privilege list (CAMERA, RECORD_AUDIO):
  https://developer.amazon.com/docs/react-native-vega/0.72/permissionskepler
- Request run-time privileges: https://developer.amazon.com/docs/vega/0.24/request-runtime-privileges.html
- Security Manager: https://developer.amazon.com/docs/vega-api/0.24/README.amazon-devices_security-manager-lib
- AudioRecordStreamBuilder: https://developer.amazon.com/docs/vega-api/0.24/classes.AudioRecordStreamBuilder.amazon-devices_keplerscript-audio-lib.html
- Jitsi on Vega thread, Amazon replies 2026-04-21 and 2026-07-24:
  https://community.amazondeveloper.com/t/issues-with-microphone-access-for-jitsi-integration-on-vega-os/28161
- WebView getUserMedia NotSupported thread:
  https://community.amazondeveloper.com/t/seeking-help-with-speech-to-text-implementation-in-kepler-webview/20392
- USB host and BLE GATT not in the developer surface:
  https://community.amazondeveloper.com/t/in-vega-os-does-it-support-ble-gatt-and-usb-host-related-apis/28840
- Media player selection, MSE mode for live: https://developer.amazon.com/docs/vega/0.24/media-player-select-playback
- Supported players (hls.js, Shaka, dash.js patched): https://developer.amazon.com/docs/vega/0.24/media-player-faq

Fire OS hardware and Alexa:

- Cube 3rd gen USB-A webcam port: https://press.aboutamazon.com/2022/9/amazon-upgrades-fire-tv-with-third-generation-fire-tv-cube-and-new-alexa-voice-remote-pro
- Alexa two-way video calling setup and webcam requirements: https://www.amazon.com/gp/help/customer/display.html?nodeId=GABVTYNJL7QXCMUY
- Zoom on Cube 2nd gen: https://www.aftvnews.com/zoom-app-launches-on-2nd-gen-fire-tv-cube-for-2-way-video-calling-through-usb-webcam/
- Zoom on Omni series: https://www.howtogeek.com/106282/how-to-make-a-zoom-call-on-your-fire-tv-omni-series/
- Fire TV Stick HD 2026 on Vega, USB-C power only: https://androidtvbox.eu/fire-tv-stick-hd-2026-vega-os-no-sideloading/
- Alexa Smart Properties WebRTC calling (enterprise only): https://developer.amazon.com/en-US/docs/alexa/alexa-smart-properties/get-started-calling-api-for-asp.html

AWS media services:

- IVS Real-Time (WebRTC, under 300 ms): https://ivs.rocks/real-time/
- IVS server-side composition to a channel: https://docs.aws.amazon.com/ivs/latest/RealTimeUserGuide/ssc-getting-started.html
- IVS Low-Latency (HLS, 3 to 5 s): https://docs.aws.amazon.com/ivs/latest/LowLatencyUserGuide/what-is.html
- Chime SDK live connector to IVS: https://docs.aws.amazon.com/chime-sdk/latest/dg/connector-pipe-config.html
- WebSocket plus MSE fragmented MP4 technique: https://github.com/elsampsa/websocket-mse-demo

AirPlay:

- Amazon TVs with AirPlay 2 and HomeKit: https://www.engadget.com/amazon-fire-tv-omni-4-series-airplay-2-homekit-support-171505953.html
- Toshiba and Insignia Fire TV Edition sets: https://www.flatpanelshd.com/news.php?subaction=showfull&id=1626325933
- Mirror a FaceTime call to an AirPlay 2 TV: https://www.macrumors.com/how-to/mirror-facetime-call-apple-tv-airplay-2-smart-tv/
- AirScreen on the Fire TV Appstore: https://www.amazon.com/AirScreen-AirPlay-Google-Cast-Miracast/dp/B07CPZ698R
- Open-source Android receiver (GPL-3.0, UxPlay core): https://github.com/jqssun/android-airplay-server
- Vega Matter Casting integration (Content Launcher, Media Playback clusters only): https://developer.amazon.com/docs/vega/0.24/vega-matter-casting-integration.html
- Vega supported libraries (network: Socket.io client): https://developer.amazon.com/docs/vega-api/0.22/supported-libraries
