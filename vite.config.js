import { defineConfig } from "vite";

// https://vitejs.dev/config/
export default defineConfig(async () => ({
  // Vite options tailored for Tauri development.
  // Tauri expects a fixed port; fail if that port is not available.
  server: {
    port: 5173,
    strictPort: true,
    watch: {
      // tell vite to ignore watching `src-tauri`
      ignored: ["**/src-tauri/**"],
    },
  },
  // Vite uses the project root as the base, so assets resolve correctly.
  envPrefix: ["VITE_", "TAURI_"],
  build: {
    // Tauri uses Chromium on Windows and WebKit on macOS/Linux.
    target:
      process.env.TAURI_PLATFORM === "windows"
        ? "chrome105"
        : "safari13",
    // don't minify for debug builds
    minify: !process.env.TAURI_DEBUG ? "esbuild" : false,
    // produce sourcemaps for debug builds
    sourcemap: !!process.env.TAURI_DEBUG,
  },
}));
