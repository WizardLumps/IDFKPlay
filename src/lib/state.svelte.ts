// App-wide state shared by the main and settings screens.
import { api, type ApiKeyStatus, type BackdropItem, type LibraryView, type MetaProgress, type Settings } from "./api";

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
});

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
  } catch (e) {
    app.libraryError = (e as Error).message;
    if (refresh) app.view = null;
  } finally {
    app.loading = false;
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
  api.onMetadataProgress((p) => {
    const finished = app.progress?.running && !p.running;
    app.progress = p;
    if (p.done < lastRebuild) lastRebuild = 0; // a new run started
    if (finished || p.done - lastRebuild >= 25) {
      lastRebuild = p.done;
      loadLibrary(false);
    }
  });

  const [settings, keyStatus, progress] = await Promise.all([
    api.getSettings(),
    api.getApiKeyStatus(),
    api.getMetadataProgress(),
  ]);
  app.settings = settings;
  app.keyStatus = keyStatus;
  app.progress = progress;
  await loadLibrary(false);
}
