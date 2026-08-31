import React from "react";
import ReactDOM from "react-dom/client";
import { flushSync } from "react-dom";
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
 * How long the first navigation gets to resolve before the window is shown
 * anyway. Well past a warm launch, and far short of the backstop in `lib.rs`.
 */
const FIRST_RESOLVE_BUDGET = 800;

let shown = false;

function show() {
  if (shown) return;
  shown = true;
  void getCurrentWindow()
    .show()
    .catch(() => {
      // Running outside a Tauri window (a plain `vite` server) — nothing to show.
    });
}

/*
 * The window is built hidden so no one sees a half-drawn frame, and it is shown
 * from here because only the renderer knows when there is a frame worth showing.
 * `onResolved` is that moment: the first navigation's loaders have returned and
 * their data is committed, so the window opens on the diff rather than on the
 * empty shell that precedes it.
 *
 * This waited on two nested animation frames until it was profiled. A hidden
 * WebKit window is not being composited, so it serves no animation frames at
 * all and that callback never ran — every launch instead sat out the
 * three-second safety net in `lib.rs`, which was almost all of a 4.6s startup.
 *
 * Subscribed before the render below, so a first navigation that resolves
 * quickly cannot land before anything is listening.
 */
const unsubscribe = router.subscribe("onResolved", () => {
  unsubscribe();
  show();
});

// A first navigation that never settles must not leave the window hidden for
// the full three seconds; an empty shell is a better answer than nothing.
setTimeout(show, FIRST_RESOLVE_BUDGET);

/*
 * StrictMode's development-only double-mount blanks the first diff in `dev`:
 * `@pierre/diffs` writes into the `diffs-container` shadow root imperatively,
 * the simulated unmount tears that down, and nothing re-applies it until the
 * next navigation hands the container a fresh diff. Release builds never
 * double-invoke, so packaged runs render the first diff either way.
 */
// SAFETY: `index.html` ships the #root div, and this module is the only thing
// the page loads, so nothing can have removed it before this runs.
const root = ReactDOM.createRoot(document.getElementById("root") as HTMLElement);

/*
 * `flushSync` so the tree is committed in this task rather than at React's
 * leisure a tick later, which is what starts the first navigation — and so the
 * `onResolved` above — as early as there is anything to start it with.
 */
flushSync(() => {
  root.render(
    <React.StrictMode>
      <ThemeProvider>
        <HotkeysProvider>
          <RouterProvider router={router} />
        </HotkeysProvider>
      </ThemeProvider>
    </React.StrictMode>,
  );
});
