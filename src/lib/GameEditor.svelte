<script lang="ts" module>
  import type { CustomGame as CG, GameOverride as GO, LibraryGame as LG } from "./api";

  export type EditTarget =
    | { kind: "new" }
    | { kind: "custom"; game: CG; favorite: boolean }
    | { kind: "steam"; game: LG; override: GO | null };
</script>

<script lang="ts">
  // Add or edit a game: a game you added (any launcher) or edits to a Steam game.
  import { untrack } from "svelte";
  import { open } from "@tauri-apps/plugin-dialog";
  import { api, type GameArt, type ImageChange, type Mood } from "./api";
  import ArtImage from "./ArtImage.svelte";
  import { branding } from "./branding";
  import { applyUserGames } from "./state.svelte";

  interface Props {
    target: EditTarget;
    onClose: () => void;
  }

  let { target, onClose }: Props = $props();

  const c = branding.copy;
  const MOODS: Mood[] = ["action", "adventure", "puzzle", "rpg", "strategy", "multiplayer", "controller", "vr"];
  const IMAGE_EXTS = ["jpg", "jpeg", "png", "webp", "gif", "bmp"];
  const BATTLE_NET = "C:\\Program Files (x86)\\Battle.net\\Battle.net.exe";

  // The form is filled once from the game it opened with (the parent recreates the editor per game).
  const initial = untrack(() => target);
  const isSteam = initial.kind === "steam";
  const custom = initial.kind === "custom" ? initial.game : null;
  const steamGame = initial.kind === "steam" ? initial.game : null;
  const override = initial.kind === "steam" ? initial.override : null;
  const appId = initial.kind === "new" ? null : initial.kind === "custom" ? initial.game.id : initial.game.appId;

  // ---- form state (initialised once from the target) ----
  let name = $state(custom?.name ?? override?.name ?? "");
  let launchKind = $state<"program" | "link">(custom?.launch.kind ?? "program");
  let path = $state(custom?.launch.kind === "program" ? custom.launch.path : "");
  let args = $state(custom?.launch.kind === "program" ? custom.launch.args : "");
  let url = $state(custom?.launch.kind === "link" ? custom.launch.url : "");
  let moods = $state<Mood[]>(custom?.moods ?? override?.moods ?? []);
  let ownMoods = $state(!isSteam || override?.moods != null);
  let favorite = $state(
    initial.kind === "custom" ? initial.favorite : initial.kind === "steam" ? initial.game.favorite : false,
  );
  let hours = $state(custom?.playtimeMinutes != null ? String(+(custom.playtimeMinutes / 60).toFixed(2)) : "");

  let image = $state<ImageChange>({ kind: "keep" });
  let imageUrlDraft = $state("");
  let showUrl = $state(false);
  let art = $state<GameArt | null>(null);
  let error = $state("");
  let busy = $state(false);
  let confirmDelete = $state(false);

  // Current art for the preview (existing games only).
  if (appId !== null) {
    api.getArt(appId).then((a) => (art = a)).catch(() => {});
  }

  const hasOwnImage = $derived(custom?.image != null || override?.image != null);
  const imageLabel = $derived(
    image.kind === "file"
      ? `New image: ${image.path.split(/[\\/]/).pop()}`
      : image.kind === "url"
        ? `New image link: ${image.url}`
        : image.kind === "clear"
          ? isSteam
            ? "Back to Steam's art"
            : "No image"
          : "",
  );

  function toggleMood(m: Mood) {
    moods = moods.includes(m) ? moods.filter((x) => x !== m) : [...moods, m];
  }

  async function browseProgram() {
    const picked = await open({
      multiple: false,
      directory: false,
      filters: [{ name: "Programs and shortcuts", extensions: ["exe", "lnk", "url", "bat", "cmd"] }],
    });
    if (typeof picked === "string") path = picked;
  }

  async function chooseImage() {
    const picked = await open({ multiple: false, directory: false, filters: [{ name: "Images", extensions: IMAGE_EXTS }] });
    if (typeof picked === "string") image = { kind: "file", path: picked };
  }

  function useImageUrl() {
    const u = imageUrlDraft.trim();
    if (!u.startsWith("https://")) {
      error = "Image links must start with https://";
      return;
    }
    error = "";
    image = { kind: "url", url: u };
    showUrl = false;
  }

  function useBattleNet() {
    launchKind = "program";
    path = BATTLE_NET;
    if (!args.trim()) args = '--exec="launch WoW"';
  }

  async function save() {
    error = "";
    busy = true;
    try {
      if (steamGame) {
        let result = await api.saveGameOverride(steamGame.appId, {
          name: name.trim() || null,
          image,
          moods: ownMoods ? moods : null,
          // Favorite is set separately so it's compared against Steam's own Favorites.
          favorite: override?.favorite ?? null,
        });
        if (favorite !== steamGame.favorite) result = await api.setFavorite(steamGame.appId, favorite);
        await applyUserGames(result);
      } else {
        const h = hours.trim() === "" ? null : Number(hours);
        if (h !== null && (!Number.isFinite(h) || h < 0)) throw new Error("Hours played must be a number, or left blank.");
        const result = await api.saveCustomGame({
          id: custom?.id ?? null,
          name,
          launch: launchKind === "program" ? { kind: "program", path, args } : { kind: "link", url },
          image,
          moods,
          favorite,
          playtimeHours: h,
        });
        await applyUserGames(result);
      }
      onClose();
    } catch (e) {
      error = (e as Error).message;
    } finally {
      busy = false;
    }
  }

  async function reset() {
    if (!steamGame) return;
    busy = true;
    try {
      await applyUserGames(await api.resetGameOverride(steamGame.appId));
      onClose();
    } catch (e) {
      error = (e as Error).message;
    } finally {
      busy = false;
    }
  }

  async function remove() {
    if (!custom) return;
    if (!confirmDelete) {
      confirmDelete = true;
      return;
    }
    busy = true;
    try {
      await applyUserGames(await api.deleteCustomGame(custom.id));
      onClose();
    } catch (e) {
      error = (e as Error).message;
    } finally {
      busy = false;
    }
  }

  function onKey(e: KeyboardEvent) {
    if (e.key === "Escape" && !busy) onClose();
  }
</script>

<svelte:window onkeydown={onKey} />

<div class="backdrop" role="presentation" onclick={() => !busy && onClose()}>
  <div
    class="dialog"
    role="dialog"
    aria-modal="true"
    aria-labelledby="editor-title"
    tabindex="-1"
    onclick={(e) => e.stopPropagation()}
    onkeydown={(e) => e.stopPropagation()}
  >
    <header>
      <h3 id="editor-title">
        {initial.kind === "new" ? "Add a game" : `Edit ${steamGame?.name ?? custom?.name}`}
      </h3>
      {#if isSteam}<span class="badge">Steam</span>{:else if custom}<span class="badge mine">Added by you</span>{/if}
    </header>

    <div class="body">
      <!-- Cover -->
      <div class="cover-col">
        <div class="cover">
          {#if image.kind === "url"}
            <img src={image.url} alt="" />
          {:else if image.kind === "keep" && art && art.portrait.length}
            <ArtImage sources={art.portrait} alt="" class="cover-img" />
          {:else}
            <span class="initial">{(name || steamGame?.name || "?").trim().charAt(0).toUpperCase()}</span>
          {/if}
        </div>
        <button class="btn small" onclick={chooseImage} disabled={busy}>Choose image…</button>
        <button class="btn small" onclick={() => (showUrl = !showUrl)} disabled={busy}>Use an image link…</button>
        {#if hasOwnImage || image.kind === "file" || image.kind === "url"}
          <button class="btn small ghost" onclick={() => (image = { kind: "clear" })} disabled={busy}>
            {isSteam ? "Use Steam's art" : "Remove image"}
          </button>
        {/if}
        {#if showUrl}
          <input class="url" placeholder="https://…" bind:value={imageUrlDraft} onkeydown={(e) => e.key === "Enter" && useImageUrl()} />
          <button class="btn small" onclick={useImageUrl}>Use link</button>
        {/if}
        {#if imageLabel}<small class="dim wrap">{imageLabel}</small>{/if}
      </div>

      <!-- Fields -->
      <div class="fields">
        <label class="field">
          <span>Name</span>
          <input bind:value={name} placeholder={isSteam ? steamGame?.name : "World of Warcraft"} spellcheck="false" />
          {#if isSteam}<small class="dim">Leave blank to use Steam's name.</small>{/if}
        </label>

        {#if !isSteam}
          <div class="field">
            <span>How it launches</span>
            <div class="segmented" role="radiogroup">
              <button role="radio" aria-checked={launchKind === "program"} onclick={() => (launchKind = "program")}>Program</button>
              <button role="radio" aria-checked={launchKind === "link"} onclick={() => (launchKind = "link")}>Launcher link</button>
            </div>
            {#if launchKind === "program"}
              <div class="row">
                <input bind:value={path} placeholder="C:\Games\MyGame\Game.exe" spellcheck="false" />
                <button class="btn" onclick={browseProgram} disabled={busy}>Browse…</button>
              </div>
              <input bind:value={args} placeholder="Arguments (optional)" spellcheck="false" />
              <small class="dim">
                Battle.net game? Launch it through Battle.net so you're signed in automatically:
                <button class="link" onclick={useBattleNet}>use Battle.net</button>, then put the game's code in
                <code>--exec="launch WoW"</code>.
              </small>
            {:else}
              <input bind:value={url} placeholder="com.epicgames.launcher://apps/…?action=launch" spellcheck="false" />
            {/if}
          </div>
        {/if}

        <div class="field">
          <span>Moods</span>
          {#if isSteam}
            <label class="check">
              <input type="checkbox" bind:checked={ownMoods} />
              <span>Use my own moods instead of Steam's tags</span>
            </label>
          {/if}
          {#if ownMoods}
            <div class="chips">
              {#each MOODS as m}
                <button class="chip" class:on={moods.includes(m)} aria-pressed={moods.includes(m)} onclick={() => toggleMood(m)}>
                  {c.moods[m]}
                </button>
              {/each}
            </div>
          {/if}
        </div>

        <div class="row split">
          <label class="check">
            <input type="checkbox" bind:checked={favorite} />
            <span>★ Favorite</span>
          </label>
          {#if !isSteam}
            <label class="hours">
              <span>Hours played</span>
              <input type="number" min="0" step="0.5" bind:value={hours} placeholder="Unknown" />
            </label>
          {/if}
        </div>
        {#if !isSteam}
          <small class="dim">Leave hours blank if you don't know: the game can still come up in Just Play, but not in Something New.</small>
        {/if}
      </div>
    </div>

    {#if error}<p class="error">{error}</p>{/if}

    <footer>
      {#if isSteam && (override || steamGame?.edited)}
        <button class="btn ghost" onclick={reset} disabled={busy}>Reset to Steam's data</button>
      {:else if custom}
        <button class="btn ghost danger" onclick={remove} disabled={busy}>
          {confirmDelete ? `Really delete ${custom.name}?` : "Delete"}
        </button>
      {/if}
      <span class="spacer"></span>
      <button class="btn" onclick={onClose} disabled={busy}>{c.cancel}</button>
      <button class="btn primary" onclick={save} disabled={busy}>{busy ? "Saving…" : "Save"}</button>
    </footer>
  </div>
</div>

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    z-index: 20;
    display: grid;
    place-items: center;
    padding: 24px;
    background: rgba(10, 12, 16, 0.65);
    backdrop-filter: blur(8px);
    animation: fade var(--t-med) var(--ease);
  }

  .dialog {
    width: min(760px, 100%);
    max-height: calc(100vh - 80px);
    overflow-y: auto;
    padding: 22px 24px;
    border-radius: 16px;
    background: linear-gradient(160deg, var(--bg-2), var(--bg-1));
    border: 1px solid var(--border);
    box-shadow: 0 30px 80px rgba(0, 0, 0, 0.5);
    outline: none;
    user-select: text;
  }

  header {
    display: flex;
    align-items: center;
    gap: 10px;
    margin-bottom: 16px;
  }

  h3 {
    margin: 0;
    font-size: 20px;
    font-weight: 750;
    overflow-wrap: anywhere;
  }

  .badge {
    font-size: 11px;
    font-weight: 650;
    padding: 2px 8px;
    border-radius: 999px;
    background: var(--surface-hover);
    color: var(--text-dim);
    white-space: nowrap;
  }

  .badge.mine {
    color: var(--accent);
    background: rgba(102, 192, 244, 0.15);
  }

  .body {
    display: grid;
    grid-template-columns: 170px 1fr;
    gap: 22px;
  }

  .cover-col {
    display: grid;
    align-content: start;
    gap: 6px;
  }

  .cover {
    aspect-ratio: 2 / 3;
    border-radius: 10px;
    overflow: hidden;
    background: rgba(0, 0, 0, 0.3);
    display: grid;
    place-items: center;
    margin-bottom: 4px;
  }

  .cover img,
  .cover :global(.cover-img) {
    width: 100%;
    height: 100%;
    object-fit: cover;
  }

  .initial {
    font-size: 56px;
    font-weight: 800;
    color: var(--text-dim);
  }

  .fields {
    display: grid;
    align-content: start;
    gap: 14px;
    min-width: 0;
  }

  .field {
    display: grid;
    gap: 6px;
  }

  .field > span,
  .hours > span {
    font-size: 13px;
    color: var(--text-dim);
  }

  .row {
    display: flex;
    gap: 8px;
    align-items: center;
  }

  .row.split {
    justify-content: space-between;
    flex-wrap: wrap;
  }

  input {
    width: 100%;
    min-width: 0;
    padding: 8px 11px;
    border-radius: 8px;
    border: 1px solid var(--border);
    background: rgba(0, 0, 0, 0.25);
    color: var(--text);
    font: inherit;
    font-size: 14px;
  }

  .hours {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .hours input {
    width: 110px;
  }

  .check {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 14px;
    cursor: pointer;
  }

  .check input {
    width: 16px;
    height: 16px;
    padding: 0;
    accent-color: var(--accent);
  }

  .chips {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
  }

  .chip {
    padding: 6px 12px;
    border-radius: 999px;
    font-size: 13px;
    background: var(--surface);
    border: 1px solid var(--border);
    color: var(--text-dim);
  }

  .chip.on {
    color: #0e1a26;
    background: var(--accent);
    border-color: var(--accent);
  }

  .segmented {
    display: inline-flex;
    gap: 6px;
  }

  .segmented button,
  .btn {
    padding: 8px 13px;
    border-radius: 8px;
    background: var(--surface-hover);
    border: 1px solid var(--border);
    font-size: 14px;
    white-space: nowrap;
  }

  .segmented button[aria-checked="true"] {
    background: rgba(102, 192, 244, 0.2);
    border-color: var(--accent);
    color: var(--accent);
  }

  .btn.small {
    padding: 6px 10px;
    font-size: 13px;
  }

  .btn.ghost {
    background: none;
  }

  .btn.danger {
    color: #ff8a8e;
    border-color: rgba(229, 72, 77, 0.45);
  }

  .btn.primary {
    background: linear-gradient(135deg, #7fd0ff 0%, var(--accent-strong) 100%);
    color: #0e1a26;
    border-color: transparent;
    font-weight: 650;
  }

  button:disabled {
    opacity: 0.55;
    cursor: default;
  }

  .link {
    padding: 0;
    color: var(--accent);
    text-decoration: underline;
    text-underline-offset: 2px;
    font-size: inherit;
  }

  code {
    font-size: 12px;
    color: var(--text);
  }

  small {
    font-size: 12px;
    line-height: 1.5;
  }

  .wrap {
    overflow-wrap: anywhere;
  }

  .dim {
    color: var(--text-dim);
  }

  .error {
    margin: 14px 0 0;
    font-size: 13px;
    color: #ff8a8e;
  }

  footer {
    display: flex;
    align-items: center;
    gap: 8px;
    margin-top: 18px;
  }

  .spacer {
    flex: 1;
  }

  @media (max-width: 640px) {
    .body {
      grid-template-columns: 1fr;
    }

    .cover {
      width: 140px;
    }
  }

  @keyframes fade {
    from {
      opacity: 0;
    }
  }
</style>
