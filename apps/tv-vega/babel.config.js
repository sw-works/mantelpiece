// The Vega Module Resolver Preset rewrites `react-native` (and ported library) imports in the
// shared UI to the @amazon-devices/* packages at bundle time, so packages/tv-ui keeps stock
// React Native imports.
module.exports = {
  presets: ["module:@react-native/babel-preset", "module:@amazon-devices/kepler-module-resolver-preset"],
};
