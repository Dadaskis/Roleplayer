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
  envPrefix: ["VITE_", "TAURI_"],
  build: {
    target: "es2021",
    outDir: "dist",
    sourcemap: false,
  },
})
