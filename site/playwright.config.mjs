import { defineConfig } from "@playwright/test";

export default defineConfig({
  testDir: "./tests",
  outputDir: "./test-results",
  reporter: "line",
  workers: 1,
  use: { browserName: "chromium", screenshot: "only-on-failure" },
});
