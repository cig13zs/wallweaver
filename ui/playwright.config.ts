import { defineConfig, devices } from "@playwright/test";

// Smoke tests run against the production build (vite preview), in browser mode with the mock daemon.
export default defineConfig({
  testDir: "tests",
  testMatch: /.*\.spec\.ts/,
  fullyParallel: false,
  workers: 1,
  reporter: [["list"]],
  use: {
    baseURL: "http://localhost:4173",
    viewport: { width: 1440, height: 900 },
    colorScheme: "dark",
    ...devices["Desktop Chrome"],
    deviceScaleFactor: 1,
  },
  projects: [{ name: "chromium", use: { browserName: "chromium", viewport: { width: 1440, height: 900 } } }],
  webServer: {
    command: "pnpm exec vite preview --port 4173 --strictPort",
    url: "http://localhost:4173",
    reuseExistingServer: false,
    timeout: 30_000,
  },
});
