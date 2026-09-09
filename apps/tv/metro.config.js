const { getDefaultConfig, mergeConfig } = require("@react-native/metro-config");

/**
 * Platform-specific files resolve in this order: `.kepler.ts` (Vega OS), `.android.ts`
 * (Fire OS), then the plain file. Keep platform variants next to the default file.
 * @type {import('metro-config').MetroConfig}
 */
const config = {
  resolver: {
    sourceExts: ["kepler.tsx", "kepler.ts", "kepler.js", "tsx", "ts", "jsx", "js", "json"],
  },
};

module.exports = mergeConfig(getDefaultConfig(__dirname), config);
