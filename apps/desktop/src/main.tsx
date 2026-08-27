import React from "react";
import ReactDOM from "react-dom/client";
import { HotkeysProvider } from "@tanstack/react-hotkeys";
import { RouterProvider, createHashHistory, createRouter } from "@tanstack/react-router";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { ThemeProvider } from "@/components/theme-provider";
import { routeTree } from "./routeTree.gen";
import "./index.css";

// Hash history: the window is served from a custom protocol with no server
// behind it, so a pushState route would 404 on reload. Back/forward still work.
const router = createRouter({
  routeTree,
  history: createHashHistory(),
  defaultPreload: false,
});

declare module "@tanstack/react-router" {
  interface Register {
    router: typeof router;
  }
}

/*
 * StrictMode's development-only double-mount blanks the first diff in `dev`:
 * `@pierre/diffs` writes into the `diffs-container` shadow root imperatively,
 * the simulated unmount tears that down, and nothing re-applies it until the
 * next navigation hands the container a fresh diff. Release builds never
 * double-invoke, so packaged runs render the first diff either way.
 */
// SAFETY: `index.html` ships the #root div, and this module is the only thing
// the page loads, so nothing can have removed it before this runs.
ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <ThemeProvider>
      <HotkeysProvider>
        <RouterProvider router={router} />
      </HotkeysProvider>
    </ThemeProvider>
  </React.StrictMode>,
);

/*
 * The window is built hidden so no one sees a half-drawn frame. Showing it from
 * here rather than on the Rust side is what makes that exact: the second frame
 * after the first render is the earliest point the page is actually painted.
 */
requestAnimationFrame(() => {
  requestAnimationFrame(() => {
    void getCurrentWindow()
      .show()
      .catch(() => {
        // Running outside a Tauri window (a plain `vite` server) — nothing to show.
      });
  });
});
