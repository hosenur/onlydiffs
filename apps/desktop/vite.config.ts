import { fileURLToPath, URL } from "node:url";
import tailwindcss from "@tailwindcss/vite";
import { tanstackRouter } from "@tanstack/router-plugin/vite";
import react from "@vitejs/plugin-react";
import { defineConfig } from "vite";

const resolvePath = (relative: string) =>
  fileURLToPath(new URL(relative, import.meta.url));

// Tauri drives this server itself, so the port is fixed and failing loudly on a
// clash beats silently serving somewhere the window will not look.
const DEV_PORT = 1420;

export default defineConfig({
  plugins: [
    tanstackRouter({ target: "react", autoCodeSplitting: true }),
    tailwindcss(),
    react(),
  ],
  resolve: {
    alias: {
      "@": resolvePath("./src"),
      "@shared": resolvePath("./src/shared"),
    },
  },
  server: {
    port: DEV_PORT,
    strictPort: true,
    watch: {
      // Rust rebuilds are Cargo's job; watching `target/` here would restart
      // the renderer on every incremental build.
      ignored: ["**/src-tauri/**"],
    },
  },
  build: {
    outDir: "dist",
    // Tauri decides how far to shrink from the profile, and keeping the
    // renderer's own sourcemaps in debug builds makes a stack trace readable.
    sourcemap: process.env.TAURI_ENV_DEBUG === "true",
  },
});
