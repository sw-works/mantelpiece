// `vega-hls` is either scripts/vega-hls.sh's build (apps/tv-vega/vendor/vega-hls) or the
// stub in apps/tv-vega/vega-hls-stub; Metro maps the name (metro.config.js).
declare module "vega-hls" {
  export const available: boolean;
  // eslint-disable-next-line @typescript-eslint/no-explicit-any -- hls.js is untyped vendor code here
  export const Hls: any;
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  export const Events: any;
  export function installPolyfills(): void;
}
