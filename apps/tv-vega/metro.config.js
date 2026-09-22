const path = require("path");
const { getDefaultConfig, mergeConfig } = require("@react-native/metro-config");

const fs = require("fs");

const workspaceRoot = path.resolve(__dirname, "../..");
// Live video: the Vega hls.js build from scripts/vega-hls.sh when it has run, else a stub
// that turns live video off. Either answers the `vega-hls` import in src/livePlayer.kepler.tsx.
const vegaHls = fs.existsSync(path.resolve(__dirname, "vendor/vega-hls/index.js")) ? path.resolve(__dirname, "vendor/vega-hls") : path.resolve(__dirname, "vega-hls-stub");

/**
 * Monorepo Metro config for the Vega shell (Vega SDK "monorepo issues" guidance).
 *
 * - `watchFolders` lets Metro see `packages/tv-ui` and the pnpm store under the root
 *   `node_modules`.
 * - `nodeModulesPaths` lets files in `packages/tv-ui` fall back to this shell's node_modules,
 *   which is where `@amazon-devices/react-native-kepler` and Babel's runtime helpers live
 *   under pnpm's strict layout.
 * - `sourceExts` puts `.kepler.ts` first so platform variants sit next to the default file.
 *
 * Do not set `resolver.resolveRequest` here: the React Native CLI installs one that rewrites
 * `react-native` imports to the kepler platform package, and a project-level resolver
 * replaces it (the bundle then pulls in stock React Native and fails).
 *
 * Bundles locally with `react-native build-vega`; not yet run on a Vega Virtual Device.
 * @type {import('metro-config').MetroConfig}
 */
const config = {
  projectRoot: __dirname,
  watchFolders: [path.resolve(workspaceRoot, "packages/tv-ui"), path.resolve(workspaceRoot, "node_modules"), vegaHls],
  resolver: {
    sourceExts: ["kepler.tsx", "kepler.ts", "kepler.js", "tsx", "ts", "jsx", "js", "mjs", "json"],
    nodeModulesPaths: [path.resolve(__dirname, "node_modules"), path.resolve(workspaceRoot, "node_modules")],
    extraNodeModules: { "vega-hls": vegaHls },
  },
};

module.exports = mergeConfig(getDefaultConfig(__dirname), config);
