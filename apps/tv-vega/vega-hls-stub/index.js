// Stand-in for apps/tv-vega/vendor/vega-hls, which scripts/vega-hls.sh builds from Amazon's
// hls.js patch set and polyfills (not redistributable, so not committed). Without it the app
// still bundles and runs; the Live screen says the TV cannot play live video yet.
export const available = false;
export const Hls = null;
export const Events = null;
export function installPolyfills() {}
