// App-wide state shared by the main and settings screens.
import {
  api,
  type ApiKeyStatus,
  type BackdropItem,
  type LibraryView,
  type MetaProgress,
  type Settings,
  type UserGames,
} from "./api";

export const app = $state({
  settings: null as Settings | null,
  view: null as LibraryView | null,
  progress: null as MetaProgress | null,
  keyStatus: null as ApiKeyStatus | null,
  /** Why the library couldn't load (e.g. Steam not found). */
  libraryError: "",
  loading: false,
  /** Home-screen backdrop covers; chosen once per app session. */
  backdrop: null as BackdropItem[] | null,
  /** Tag learning has been running a while: show the "keep the app open" note. */
  learningSlow: false,
  /** Games the user added and their edits to Steam games. */
  userGames: null as UserGames | null,
});

/** Applies the result of a game edit and refreshes the list (no rescan needed). */
export async function applyUserGames(userGames: UserGames) {
  app.userGames = userGames;
  await loadLibrary(false);
}

/** Rescan when the window regains focus, at most this often (catches new purchases/installs). */
const FOCUS_RESCAN_MIN_MS = 60_000;
/** Learning still running after this long gets the "big library" note. */
const SLOW_LEARNING_MS = 8_000;

let lastScan = 0;

let backdropRequested = false;

/** Fetches the backdrop covers once the library is available (once per session). */
export async function loadBackdrop(count: number) {
  if (backdropRequested || !app.view) return;
  backdropRequested = true;
  try {
    app.backdrop = await api.getBackdrop(count);
  } catch {
    // Purely decorative: no backdrop is fine.
    app.backdrop = [];
  }
}

/** True when the Web API supplied owned games, so uninstalled games are known. */
export function hasOwnedGames(view: LibraryView | null): boolean {
  return view?.api.kind === "ok" || view?.api.kind === "cached";
}

/** Effective installed-only toggle: forced on without owned games, else the saved choice (default off). */
export function installedOnly(): boolean {
  if (!hasOwnedGames(app.view)) return true;
  return app.settings?.installedOnly ?? false;
}

export async function loadLibrary(refresh = false) {
  app.loading = true;
  try {
    app.view = await api.getLibrary(refresh);
    app.libraryError = "";
    if (refresh || lastScan === 0) lastScan = Date.now();
  } catch (e) {
    app.libraryError = (e as Error).message;
    if (refresh) app.view = null;
  } finally {
    app.loading = false;
  }
}

/** Quiet background rescan: picks up new games, keeps the current list if it fails. */
async function rescanInBackground() {
  if (app.loading || Date.now() - lastScan < FOCUS_RESCAN_MIN_MS) return;
  lastScan = Date.now();
  try {
    app.view = await api.getLibrary(true);
    app.libraryError = "";
  } catch {
    // Keep showing what we had; the next focus or a manual refresh tries again.
  }
}

/** Saves a settings change; `rescan` reloads the library (Steam folder, account, libraries). */
export async function updateSettings(patch: Partial<Settings>, rescan = false) {
  if (!app.settings) return;
  app.settings = await api.saveSettings({ ...app.settings, ...patch });
  if (rescan) await loadLibrary(true);
}

let started = false;

export async function init() {
  if (started) return;
  started = true;

  // Rebuild the view now and then while tags stream in, and once when done.
  let lastRebuild = 0;
  let slowTimer: ReturnType<typeof setTimeout> | undefined;
  api.onMetadataProgress((p) => {
    const finished = app.progress?.running && !p.running;
    const started = p.running && !app.progress?.running;
    app.progress = p;
    // Most libraries finish in seconds; only a long run gets the "keep it open" note.
    if (started) {
      clearTimeout(slowTimer);
      slowTimer = setTimeout(() => (app.learningSlow = app.progress?.running ?? false), SLOW_LEARNING_MS);
    }
    if (!p.running) {
      clearTimeout(slowTimer);
      app.learningSlow = false;
    }
    if (p.done < lastRebuild) lastRebuild = 0; // a new run started
    if (finished || p.done - lastRebuild >= 25) {
      lastRebuild = p.done;
      loadLibrary(false);
    }
  });

  const [settings, keyStatus, progress, userGames] = await Promise.all([
    api.getSettings(),
    api.getApiKeyStatus(),
    api.getMetadataProgress(),
    api.getUserGames(),
  ]);
  app.settings = settings;
  app.userGames = userGames;
  app.keyStatus = keyStatus;
  app.progress = progress;
  await loadLibrary(false);

  // Back from Steam after buying or installing something? Pick it up.
  window.addEventListener("focus", rescanInBackground);
}
