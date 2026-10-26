// Typed wrappers around the Rust commands. Shapes mirror the serde structs in src-tauri.
import { convertFileSrc, invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

export interface Settings {
  steamPath: string | null;
  extraLibraries: string[];
  steamId: string | null;
  somethingNewHours: number;
  afterLaunch: "close" | "stayOpen";
  installedOnly: boolean | null;
  blacklist: number[];
  hideApiBanner: boolean;
  skipSteamHidden: boolean;
}

export interface CollectionInfo {
  id: string;
  name: string;
  /** Members in the picker's game list. */
  count: number;
  /** Rule-based in Steam: uses the list Steam stored. */
  dynamic: boolean;
}

export type Mode = "justPlay" | "somethingNew";
export type Mood = "action" | "adventure" | "puzzle" | "rpg" | "strategy" | "multiplayer" | "controller" | "vr";

export interface PickRequest {
  mode: Mode;
  moods: Mood[];
  installedOnly: boolean;
  exclude: number[];
  collections: string[];
}

export interface ReelItem {
  appId: number;
  name: string;
  portrait: ArtSource[];
}

export interface PickOutcome {
  game: LibraryGame | null;
  art: GameArt | null;
  reel: ReelItem[];
  eligible: number;
  rerollsLeft: number;
  rerollDenied: boolean;
}

export interface Game {
  source: string;
  appId: number;
  name: string;
  installed: boolean;
  installPath: string | null;
  sizeOnDisk: number | null;
  playtimeMinutes: number;
  lastPlayed: number | null;
}

export type MetaState = "pending" | "known" | "unknown";

export interface LibraryGame extends Game {
  meta: MetaState;
  genres: string[];
  categories: string[];
  tags: string[];
}

export interface LibraryInfo {
  path: string;
  origin: "steam" | "settings";
  reachable: boolean;
  gameCount: number;
}

export interface SteamAccount {
  steamId: string;
  accountId: number;
  personaName: string;
  accountName: string;
  mostRecent: boolean;
  timestamp: number;
}

export interface SteamInfo {
  steamPath: string;
  steamPathSource: "settings" | "registry" | "default";
  libraries: LibraryInfo[];
  accounts: SteamAccount[];
  selectedAccount: string | null;
}

export type ApiStatus =
  | { kind: "noKey" }
  | { kind: "noAccount" }
  | { kind: "ok"; count: number }
  | { kind: "cached"; count: number; fetchedAt: number; message: string }
  | { kind: "error"; message: string };

export interface HiddenGame {
  appId: number;
  name: string;
  reason: string;
}

export interface LibraryView {
  games: LibraryGame[];
  hidden: HiddenGame[];
  collections: CollectionInfo[];
  steamHidden: number[];
  steam: SteamInfo;
  api: ApiStatus;
  warnings: string[];
}

export interface MetaProgress {
  done: number;
  total: number;
  running: boolean;
  note: string | null;
  error: string | null;
}

export type ArtSource = { kind: "file"; value: string } | { kind: "url"; value: string };

/** Candidates per slot, best first. */
export interface GameArt {
  portrait: ArtSource[];
  header: ArtSource[];
  hero: ArtSource[];
}

/** Local files go through Tauri's asset protocol; URLs are used as-is. */
export function artSrc(s: ArtSource): string {
  return s.kind === "file" ? convertFileSrc(s.value) : s.value;
}

export interface ApiKeyStatus {
  hasKey: boolean;
  storage: "credentialManager" | "file" | null;
}

/** Rust commands reject with a user-facing message string. */
function message(e: unknown): string {
  return typeof e === "string" ? e : e instanceof Error ? e.message : String(e);
}

async function call<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  try {
    return await invoke<T>(cmd, args);
  } catch (e) {
    throw new Error(message(e));
  }
}

export const api = {
  getSettings: () => call<Settings>("get_settings"),
  saveSettings: (settings: Settings) => call<Settings>("save_settings", { settings }),
  getLibrary: (refresh = false) => call<LibraryView>("get_library", { refresh }),
  pickGame: (request: PickRequest, reroll: boolean) => call<PickOutcome>("pick_game", { request, reroll }),
  getArt: (appId: number) => call<GameArt>("get_art", { appId }),
  launchGame: (appId: number) => call<{ closing: boolean }>("launch_game", { appId }),
  setBlacklisted: (appId: number, blacklisted: boolean) =>
    call<Settings>("set_blacklisted", { appId, blacklisted }),
  rerollsLeft: () => call<number>("rerolls_left"),
  getMetadataProgress: () => call<MetaProgress>("get_metadata_progress"),
  rebuildMetadata: () => call<void>("rebuild_metadata"),
  getApiKeyStatus: () => call<ApiKeyStatus>("get_api_key_status"),
  setApiKey: (key: string | null) => call<ApiKeyStatus>("set_api_key", { key }),
  onMetadataProgress: (cb: (p: MetaProgress) => void): Promise<UnlistenFn> =>
    listen<MetaProgress>("metadata-progress", (e) => cb(e.payload)),
};

export const API_KEY_URL = "https://steamcommunity.com/dev/apikey";

export function formatPlaytime(minutes: number): string {
  if (minutes <= 0) return "Never played";
  if (minutes < 60) return `${minutes} min`;
  const h = minutes / 60;
  return `${h < 10 ? h.toFixed(1) : Math.round(h)} h`;
}

export function formatSize(bytes: number | null): string {
  if (!bytes) return "—";
  const gb = bytes / 1024 ** 3;
  return gb >= 1 ? `${gb.toFixed(1)} GB` : `${Math.round(bytes / 1024 ** 2)} MB`;
}
