import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

// @ts-expect-error process is a nodejs global
const host = process.env.TAURI_DEV_HOST;

// https://vite.dev/config/
export default defineConfig(async () => ({
  plugins: [react()],

  // Vite options tailored for Tauri development and only applied in `tauri dev` or `tauri build`
  //
  // 1. prevent Vite from obscuring rust errors
  clearScreen: false,
  // 2. tauri expects a fixed port, fail if that port is not available
  server: {
    port: 1420,
    strictPort: true,
    // Explicit 127.0.0.1, not `false`: with `false` Vite listens on `localhost`,
    // which on this Node/Windows resolves to `[::1]` ONLY - measured, `netstat`
    // showed a single `[::1]:1420` listener and a connection to 127.0.0.1:1420
    // was refused. WebView2 picks a family when it resolves `localhost`, so the
    // dev window came up blank on the boots where it picked IPv4, with no HTTP
    // request ever reaching Vite. Keep this in sync with `devUrl`.
    host: host || "127.0.0.1",
    hmr: host
      ? {
          protocol: "ws",
          host,
          port: 1421,
        }
      : undefined,
    watch: {
      // 3. tell Vite to ignore watching `src-tauri`
      ignored: ["**/src-tauri/**"],
    },
  },
}));
