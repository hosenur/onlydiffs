import { fileURLToPath, URL } from "node:url";
import tailwindcss from "@tailwindcss/vite";
import { tanstackRouter } from "@tanstack/router-plugin/vite";
import react from "@vitejs/plugin-react";
import { defineConfig, externalizeDepsPlugin } from "electron-vite";

const resolvePath = (relative: string) =>
  fileURLToPath(new URL(relative, import.meta.url));

const alias = {
  "@": resolvePath("./src"),
  "@shared": resolvePath("./electron/shared"),
};

/**
 * `format: "es"` plus an explicit `.mjs` extension is what lets `package.json`
 * keep `"type": "module"`: Electron loads both entries as real ES modules, and
 * the preload does so because the window disables the sandbox.
 */
const nodeOutput = {
  format: "es" as const,
  entryFileNames: "[name].mjs",
};

export default defineConfig({
  main: {
    // Effect and @effect/platform-node stay as runtime `node_modules` imports
    // rather than being inlined — they resolve Node built-ins at load time.
    plugins: [externalizeDepsPlugin()],
    resolve: { alias },
    build: {
      outDir: "out/main",
      rollupOptions: {
        input: resolvePath("./electron/main/index.ts"),
        output: nodeOutput,
      },
    },
  },
  preload: {
    plugins: [externalizeDepsPlugin()],
    resolve: { alias },
    build: {
      outDir: "out/preload",
      rollupOptions: {
        input: resolvePath("./electron/preload/index.ts"),
        output: nodeOutput,
      },
    },
  },
  renderer: {
    // The renderer keeps the repository root, so `index.html` and `src/` stay
    // where they were under Tauri.
    root: ".",
    // The packaged window is loaded from `file://`, so emitted asset URLs have
    // to be relative to the document rather than to a server root.
    base: "./",
    resolve: { alias },
    plugins: [
      tanstackRouter({ target: "react", autoCodeSplitting: true }),
      tailwindcss(),
      react(),
    ],
    build: {
      outDir: "out/renderer",
      rollupOptions: { input: resolvePath("./index.html") },
    },
  },
});
