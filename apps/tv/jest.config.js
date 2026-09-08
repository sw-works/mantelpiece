// pnpm stores packages under node_modules/.pnpm/<name>@<version>/node_modules/<name>, so
// the preset's allowlist (which assumes a flat layout) never matches. Allow both layouts.
module.exports = {
  preset: "@react-native/jest-preset",
  testMatch: ["<rootDir>/test/**/*.test.ts?(x)"],
  setupFiles: ["<rootDir>/test/setup.ts"],
  transformIgnorePatterns: [
    "node_modules/(?!\\.pnpm/)(?!((jest-)?react-native|@react-native(-community)?)/)",
    "node_modules/\\.pnpm/(?!(jest-)?react-native[@-]|@react-native(-community)?\\+)",
  ],
};
