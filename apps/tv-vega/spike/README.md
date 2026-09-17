# Live-latency spike

Measures glass-to-glass lag for a live HLS stream on the Vega Virtual Device: the encoder
burns the host wall clock into the picture, the TV overlays its own clock, one screenshot
gives the difference. Results and the reasons behind each step are in
`specs/video-call-feasibility-2026-09-15.md` section 7.

Off the normal build on purpose: `tsconfig.json` and Metro only see `index.js` and `src/`,
and `vendor/` is gitignored (Amazon-licensed polyfills plus a patched hls.js build), so CI
and the app bundle never touch this directory.

## Run it

1. Build the Vega hls.js once. From a scratch directory:

   ```sh
   curl -sSLO https://amzndevresources.com/vega/media-player/hls-rel-v1.5.11-r1.7-devices_scope.tar.gz
   tar -xzf hls-rel-v1.5.11-r1.7-devices_scope.tar.gz
   git clone --depth 1 --branch v1.5.11 https://github.com/video-dev/hls.js.git
   cd hls.js && git apply ../hls-rel/hls-patch/*.patch -3 && npm install && npm run build && cd ..
   mkdir -p <repo>/apps/tv-vega/spike/vendor/dist
   cp hls.js/dist/hls.mjs <repo>/apps/tv-vega/spike/vendor/dist/
   cp -r hls-rel/polyfills <repo>/apps/tv-vega/spike/vendor/
   ```

2. Create an IVS low-latency channel and push the clock pattern (`scripts/live-spike/`):

   ```sh
   aws ivs create-channel --name latency-spike --latency-mode LOW --type BASIC \
     --region us-west-2 --output json > /tmp/ivs-channel.json
   scripts/live-spike/push-clock.sh /tmp/ivs-channel.json
   ```

3. Point the shell at the spike. In `index.js`, replace the `AppRegistry.registerComponent`
   call with:

   ```js
   const { LiveSpike } = require("./spike/LiveSpike");
   AppRegistry.registerComponent(name, () => () => createElement(LiveSpike, { uri: "<playbackUrl from the channel json>" }));
   ```

4. `pnpm build:vega:debug`, `vega run-app build/aarch64-debug/*.vpkg com.hanyu.memorylane.main -d VirtualDevice`,
   then `scripts/vvd-screenshot.sh out.png`. Select on the remote
   (`vega exec vda -s emulator-5554 shell "inputd-cli button_press KEY_ENTER"`) cycles the
   hls.js live-sync modes in `LiveSpike.tsx`.

5. Afterwards: revert `index.js`, stop the push, `aws ivs delete-channel --arn <arn>`
   (refused while the stream is still live; wait a minute).

`manifest.toml` already carries the W3C media services and modules the spike needs; the
app itself does not use them yet.

## Design preview

`Preview.tsx` cycles every redesigned screen with scripted data and public sample photos,
one screen every nine seconds, so a screenshot loop can capture each on the Virtual Device.
Same loop as the spike: point `index.js` at it by hand,

```js
const { Preview } = require("./spike/Preview");
AppRegistry.registerComponent(name, () => () => createElement(Preview));
```

`pnpm build:vega:debug`, `vega run-app …`, then `scripts/vvd-screenshot.sh` every nine
seconds. Revert `index.js` afterwards. The debug build's yellow warning banner covers the
top strip; a release build has none.
