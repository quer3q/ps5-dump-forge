import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

import { version } from "./package.json";
import { MANIFEST, writeManifest } from "./scripts/appcache.mjs";

// Tauri expects a fixed dev port (tauri.conf.json devUrl).
// `--mode http` (npm run build:http) builds the PS5 web UI into dist-http instead: api.ts's
// transport becomes HTTP (src/transport-http.tsx), so no @tauri-apps code is bundled. Its entry
// is src/main-http.tsx (the tile's launcher, then the app), and the built page carries an
// AppCache manifest listing every file, so the PS5 browser shows it while nothing listens.
export default defineConfig(({ mode }) => {
  const http = mode === "http";
  return {
    // The page title carries the version: the PS5 browser shows it in its title bar (the
    // desktop window sets the same title from Cargo's version in main.rs).
    // scripts/check-versions.sh keeps package.json at Cargo.toml's version.
    plugins: [
      react(),
      {
        name: "title-version",
        transformIndexHtml: (html: string) =>
          html.replace("<title>PS5 Dump Forge</title>", `<title>PS5 Dump Forge v${version}</title>`),
      },
      ...(http
        ? [
            {
              name: "http-entry",
              transformIndexHtml: {
                order: "pre" as const,
                handler: (html: string, ctx: { server?: unknown }) => {
                  html = html.replace('src="/src/main.tsx"', 'src="/src/main-http.tsx"');
                  // Relative to the page: /forge.appcache from the tile's http://127.0.0.1:8095/.
                  return ctx.server ? html : html.replace("<html ", `<html manifest="${MANIFEST}" `);
                },
              },
            },
            {
              name: "appcache",
              apply: "build" as const,
              writeBundle: { order: "post" as const, handler: (o: { dir?: string }) => writeManifest(o.dir!) },
            },
          ]
        : []),
    ],
    clearScreen: false,
    base: "./",
    resolve: {
      alias: {
        "forge-transport": http ? "/src/transport-http.tsx" : "/src/transport-tauri.tsx",
      },
    },
    server: { port: 1420, strictPort: true, host: false },
    build: {
      outDir: http ? "dist-http" : "dist",
      emptyOutDir: true,
      // WKWebView on macOS 11 (Safari 14); the PS5's WebKit is older than current Safari too.
      target: "safari14",
    },
  };
});
