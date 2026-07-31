import { defineConfig } from "vite"
import react from "@vitejs/plugin-react"

// Vite config for the Roleplayer webview. The dev server host/port must match
// `tauri.conf.json` -> build.devUrl so Tauri can load it during `npm run tauri dev`.
export default defineConfig({
  plugins: [react()],
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
  },
  // Restrict the dependency pre-bundle scan to our real entry. Vite's default
  // scan globs `**/*.html` over the whole project root, which pulled in the
  // `__ref__/` reference projects (read-only, not part of the build) and
  // failed to resolve their deps. `index.html` fully determines our graph.
  optimizeDeps: {
    entries: ["index.html"],
  },
  envPrefix: ["VITE_", "TAURI_"],
  build: {
    target: "es2021",
    outDir: "dist",
    sourcemap: false,
  },
})
