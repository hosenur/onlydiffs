import React from "react";
import ReactDOM from "react-dom/client";
import { HotkeysProvider } from "@tanstack/react-hotkeys";
import { RouterProvider, createHashHistory, createRouter } from "@tanstack/react-router";
import { ThemeProvider } from "@/components/theme-provider";
import { routeTree } from "./routeTree.gen";
import "./index.css";

// Hash history: the packaged app is loaded from `file://`, where pushState
// routing has no server to fall back on. Back/forward still work.
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

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <ThemeProvider>
      <HotkeysProvider>
        <RouterProvider router={router} />
      </HotkeysProvider>
    </ThemeProvider>
  </React.StrictMode>,
);
