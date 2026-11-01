import { defineConfig } from "@playwright/test";

export default defineConfig({
  testDir: ".",
  testMatch: "console.spec.js",
  workers: 1,
  reporter: "list",
  use: {
    browserName: "chromium",
    baseURL: "http://127.0.0.1:8792",
    viewport: { width: 1440, height: 900 },
  },
});
