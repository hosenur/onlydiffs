import { fileURLToPath } from "node:url";
import * as path from "node:path";
import { BrowserWindow, app, shell } from "electron";
import { registerIpcHandlers } from "./ipc";
import { runtime } from "./runtime";

const currentDir = path.dirname(fileURLToPath(import.meta.url));

/** electron-vite sets this only while `electron-vite dev` is serving. */
const rendererUrl = process.env.ELECTRON_RENDERER_URL;

/**
 * The app is dark in both appearances (see `src/index.css`), so painting the
 * window with the theme background avoids a white flash before React mounts.
 */
const WINDOW_BACKGROUND = "#131419";

function createWindow(): BrowserWindow {
  const window = new BrowserWindow({
    width: 1100,
    height: 760,
    minWidth: 720,
    minHeight: 480,
    title: "onlydiffs",
    backgroundColor: WINDOW_BACKGROUND,
    show: false,
    webPreferences: {
      preload: path.join(currentDir, "../preload/index.mjs"),
      // An ES-module preload only loads with the sandbox off; context isolation
      // still keeps the renderer off Node, and the bridge stays the only way in.
      sandbox: false,
      contextIsolation: true,
      nodeIntegration: false,
    },
  });

  window.once("ready-to-show", () => window.show());

  // Nothing in this app should open a second window or navigate away from the
  // bundle; anything that tries is handed to the real browser instead.
  window.webContents.setWindowOpenHandler(({ url }) => {
    void shell.openExternal(url);
    return { action: "deny" };
  });
  window.webContents.on("will-navigate", (event, url) => {
    if (rendererUrl !== undefined && url.startsWith(rendererUrl)) return;
    event.preventDefault();
    void shell.openExternal(url);
  });

  if (rendererUrl !== undefined) {
    void window.loadURL(rendererUrl);
  } else {
    void window.loadFile(path.join(currentDir, "../renderer/index.html"));
  }

  return window;
}

app.whenReady().then(() => {
  app.setAppUserModelId("dev.hosenur.onlydiffs");
  registerIpcHandlers();
  createWindow();

  app.on("activate", () => {
    if (BrowserWindow.getAllWindows().length === 0) createWindow();
  });
});

app.on("window-all-closed", () => {
  if (process.platform !== "darwin") app.quit();
});

/** Never let a stuck finalizer keep the window on screen after Cmd+Q. */
const DISPOSE_TIMEOUT_MS = 2000;

// Tears down the Effect runtime — open scopes, the HTTP agent — before exit.
// `app.exit` skips `will-quit`, so preventing the default here runs once.
app.on("will-quit", (event) => {
  event.preventDefault();
  void Promise.race([
    runtime.dispose(),
    new Promise((resolve) => setTimeout(resolve, DISPOSE_TIMEOUT_MS)),
  ]).finally(() => app.exit(0));
});
