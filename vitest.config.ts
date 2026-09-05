import { fileURLToPath } from "node:url";
import { defineConfig } from "vitest/config";
import react from "@vitejs/plugin-react";

const double = (name: string) =>
  fileURLToPath(new URL(`./src/test/doubles/${name}.ts`, import.meta.url));

// Kept separate from vite.config.ts on purpose: that file pins port 1420 with
// `strictPort` for `tauri dev`, and loading it here would make the test run
// depend on a free port it never uses.
export default defineConfig({
  plugins: [react()],
  test: {
    environment: "jsdom",
    include: ["src/**/*.test.ts", "src/**/*.test.tsx"],
    setupFiles: ["./src/test/setup.ts"],
    // The stores register their `listen` handlers at module import time, so the
    // Tauri API has to be replaced by resolution — a per-test `vi.mock` would
    // race with the import that already ran.
    alias: {
      "@tauri-apps/api/event": double("tauriEvent"),
      "@tauri-apps/api/core": double("tauriCore"),
    },
  },
});
