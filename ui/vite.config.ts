import { defineConfig } from "vitest/config";
import { svelte } from "@sveltejs/vite-plugin-svelte";
import { CSP } from "./csp.ts";

// Tauri expects a fixed port in dev and serves the built files from dist/.
export default defineConfig({
  plugins: [svelte()],
  clearScreen: false,
  server: { port: 1420, strictPort: true },
  preview: {
    port: 4173,
    strictPort: true,
    // Serve the production build under the same CSP the desktop shell uses,
    // so the smoke test catches violations before they reach the app.
    headers: { "Content-Security-Policy": CSP },
  },
  build: {
    target: "es2022",
    cssCodeSplit: true,
    modulePreload: { polyfill: false },
    reportCompressedSize: true,
  },
  test: {
    include: ["src/**/*.test.ts"],
    environment: "node",
  },
});
