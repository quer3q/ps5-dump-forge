import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

// Tauri expects a fixed dev port (tauri.conf.json devUrl).
export default defineConfig({
  plugins: [react()],
  clearScreen: false,
  base: "./",
  server: { port: 1420, strictPort: true, host: false },
  build: {
    outDir: "dist",
    emptyOutDir: true,
    // WKWebView on macOS 11 (Safari 14).
    target: "safari14",
  },
});
