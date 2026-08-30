import React from "react";
import ReactDOM from "react-dom/client";
import { HotkeysProvider } from "@tanstack/react-hotkeys";
import { RouterProvider, createMemoryHistory, createRouter } from "@tanstack/react-router";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { ThemeProvider } from "@/components/theme-provider";
import { currentProject } from "@/lib/ipc";
import { routeTree } from "./routeTree.gen";
import "./index.css";

/*
 * `/` is the project picker, so a launch that restored a repository would show
 * it anyway. Deciding the entry here rather than redirecting out of `/` keeps
 * the picker reachable: the sidebar's "Switch project" link still points at `/`
 * and still lands there, because nothing bounces off it once the app is up.
 *
 * A failure means the bridge is missing (a plain `vite` server), where the
 * picker is the honest thing to show.
 */
const opened = await currentProject().catch(() => null);
const initialPath = opened === null ? "/" : "/diff";

/*
 * Keep route history inside the router. WebKit binds Backspace to its own page
 * history even while a text input is focused; hash routes gave it entries to
 * traverse, so deleting palette text could leave the repository. A desktop
 * window has no URL to restore: every launch starts here and chooses its route
 * from the persisted project above.
 */
const router = createRouter({
  routeTree,
  history: createMemoryHistory({ initialEntries: [initialPath] }),
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
