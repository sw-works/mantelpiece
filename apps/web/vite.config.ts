import { resolve } from "node:path";
import { defineConfig } from "vitest/config";

// Two entry pages: link.html (sign-in approval) and index.html (uploader).
export default defineConfig({
  build: {
    rollupOptions: {
      input: { upload: resolve(__dirname, "index.html"), link: resolve(__dirname, "link.html"), trip: resolve(__dirname, "trip.html"), ask: resolve(__dirname, "ask.html"), people: resolve(__dirname, "people.html"), share: resolve(__dirname, "s.html") },
    },
  },
  test: { include: ["test/**/*.test.ts"], environment: "node" },
});
