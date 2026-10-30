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
  /** Rerolls per app session; null = unlimited. */
  rerollsPerSession: number | null;
}

/** Highest finite reroll setting (mirrors settings::MAX_REROLLS). */
export const MAX_REROLLS = 10;

/** fresh = Just Play / Something New, reroll = Reroll button, replace = after "Never show this again". */
export type PickKind = "fresh" | "reroll" | "replace";

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

export interface BackdropItem {
  appId: number;
  portrait: ArtSource[];
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
  /** null = unlimited. */
  rerollsLeft: number | null;
  rerollsUsed: number;
  /** This pick spent a reroll. */
  charged: boolean;
  /** Out of rerolls: same game as before. */
  locked: boolean;
}

export interface Game {
  source: string;
  appId: number;
  name: string;
  installed: boolean;
  installPath: string | null;
  sizeOnDisk: number | null;
  playtimeMinutes: number;
  /** Custom games without hours: never counts as Something New. */
  playtimeUnknown: boolean;
  lastPlayed: number | null;
}

export type MetaState = "pending" | "known" | "unknown";

export interface LibraryGame extends Game {
  meta: MetaState;
  genres: string[];
  categories: string[];
  tags: string[];
  favorite: boolean;
  /** A Steam game the user edited. */
  edited: boolean;
}

// ---- Games the user adds, and edits to Steam games (mirrors usergames.rs) ----

/** IDs at or above this are games the user added. */
export const CUSTOM_ID_BASE = 2_000_000_000;
export const isCustom = (id: number) => id >= CUSTOM_ID_BASE;

export type Launch = { kind: "program"; path: string; args: string } | { kind: "link"; url: string };

export interface CustomGame {
  id: number;
  name: string;
  launch: Launch;
  /** File name in IDFKPlay's art folder, or an https URL. */
  image: string | null;
  moods: Mood[];
  favorite: boolean;
  /** null = unknown. */
  playtimeMinutes: number | null;
}

export interface GameOverride {
  name: string | null;
  image: string | null;
  moods: Mood[] | null;
  favorite: boolean | null;
}

export interface UserGames {
  custom: CustomGame[];
  overrides: Record<string, GameOverride>;
}

export type ImageChange =
  | { kind: "keep" }
  | { kind: "file"; path: string }
  | { kind: "url"; url: string }
  | { kind: "clear" };

export interface CustomGameInput {
  /** null = add a new game. */
  id: number | null;
  name: string;
  launch: Launch;
  image: ImageChange;
  moods: Mood[];
  favorite: boolean;
  /** null = unknown. */
  playtimeHours: number | null;
}

/** Edits to a Steam game; null keeps Steam's value. */
export interface OverrideInput {
  name: string | null;
  image: ImageChange;
  moods: Mood[] | null;
  favorite: boolean | null;
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
  pickGame: (request: PickRequest, kind: PickKind) => call<PickOutcome>("pick_game", { request, kind }),
  getArt: (appId: number) => call<GameArt>("get_art", { appId }),
  getBackdrop: (count: number) => call<BackdropItem[]>("get_backdrop", { count }),
  launchGame: (appId: number) => call<{ closing: boolean }>("launch_game", { appId }),
  setBlacklisted: (appId: number, blacklisted: boolean) =>
    call<Settings>("set_blacklisted", { appId, blacklisted }),
  rerollsLeft: () => call<number | null>("rerolls_left"),
  getUserGames: () => call<UserGames>("get_user_games"),
  saveCustomGame: (game: CustomGameInput) => call<UserGames>("save_custom_game", { game }),
  deleteCustomGame: (appId: number) => call<UserGames>("delete_custom_game", { appId }),
  saveGameOverride: (appId: number, edits: OverrideInput) => call<UserGames>("save_game_override", { appId, edits }),
  resetGameOverride: (appId: number) => call<UserGames>("reset_game_override", { appId }),
  setFavorite: (appId: number, favorite: boolean) => call<UserGames>("set_favorite", { appId, favorite }),
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
