import { defineConfig } from "@playwright/test";

// Smoke tests run against the production build (vite preview), in browser mode with the mock daemon.
export default defineConfig({
  testDir: "tests",
  testMatch: /.*\.spec\.ts/,
  fullyParallel: false,
  workers: 1,
  reporter: [["list"]],
  use: {
    browserName: "chromium",
    baseURL: "http://localhost:4173",
    viewport: { width: 1440, height: 900 },
    deviceScaleFactor: 1,
    colorScheme: "dark",
  },
  webServer: {
    command: "pnpm exec vite preview --port 4173 --strictPort",
    url: "http://localhost:4173",
    reuseExistingServer: false,
    timeout: 30_000,
  },
});
