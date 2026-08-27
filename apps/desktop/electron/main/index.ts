import { fileURLToPath } from "node:url";
import * as path from "node:path";
import { BrowserWindow, app, nativeImage, nativeTheme, shell } from "electron";
import { registerIpcHandlers } from "./ipc";
import { runtime } from "./runtime";

const currentDir = path.dirname(fileURLToPath(import.meta.url));

/** electron-vite sets this only while `electron-vite dev` is serving. */
const rendererUrl = process.env.ELECTRON_RENDERER_URL;

/**
 * Painting the window up front avoids a flash of the wrong appearance before
 * React mounts. These are Intent UI's `--bg` in each scheme (see
 * `src/index.css`), picked by OS appearance to match the renderer's default
 * "system" theme — someone who has pinned the other one in-app still gets a
 * single frame of this before the ThemeProvider catches up.
 */
const WINDOW_BACKGROUND = { light: "#ffffff", dark: "#111114" } as const;

function createWindow(): BrowserWindow {
  const window = new BrowserWindow({
    width: 1100,
    height: 760,
    minWidth: 720,
    minHeight: 480,
    title: "onlydiffs",
    backgroundColor: nativeTheme.shouldUseDarkColors
      ? WINDOW_BACKGROUND.dark
      : WINDOW_BACKGROUND.light,
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

/**
 * macOS takes the Dock icon from the running bundle, and in development that
 * bundle is the stock `Electron.app` — so the Dock shows Electron's logo no
 * matter what `electron-builder.yml` says. Pointing the Dock at our own icon
 * makes development match the packaged app. A packaged build already has the
 * right icon in its bundle, and `build/` is not shipped inside the asar, so
 * this only runs unpackaged.
 */
function applyDevelopmentDockIcon(): void {
  if (process.platform !== "darwin" || app.isPackaged) return;
  const icon = nativeImage.createFromPath(
    path.join(currentDir, "../../build/icon.png"),
  );
  if (!icon.isEmpty()) app.dock?.setIcon(icon);
}

app.whenReady().then(() => {
  app.setAppUserModelId("dev.hosenur.onlydiffs");
  applyDevelopmentDockIcon();
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
