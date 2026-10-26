<script lang="ts">
  // Debug view proving the data pipeline works. Replaced by the real UI in Phase 5.
  import { onDestroy } from "svelte";
  import { openUrl } from "@tauri-apps/plugin-opener";
  import {
    api,
    API_KEY_URL,
    formatPlaytime,
    formatSize,
    type ApiKeyStatus,
    type LibraryView,
    type MetaProgress,
  } from "../lib/api";
  import { branding } from "../lib/branding";

  interface Props {
    onBack: () => void;
  }

  let { onBack }: Props = $props();

  let view = $state<LibraryView | null>(null);
  let error = $state("");
  let loading = $state(false);
  let query = $state("");
  let showHidden = $state(false);
  let progress = $state<MetaProgress | null>(null);
  let keyStatus = $state<ApiKeyStatus | null>(null);
  let keyInput = $state("");
  let keyMessage = $state("");

  const games = $derived(
    (view?.games ?? []).filter((g) => g.name.toLowerCase().includes(query.trim().toLowerCase())),
  );
  const installedCount = $derived(view?.games.filter((g) => g.installed).length ?? 0);

  async function load(refresh: boolean) {
    loading = true;
    error = "";
    try {
      view = await api.getLibrary(refresh);
    } catch (e) {
      error = (e as Error).message;
    } finally {
      loading = false;
    }
  }

  // Rebuild the view now and then while metadata streams in, and once at the end.
  let lastRebuild = 0;
  const unlisten = api.onMetadataProgress((p) => {
    progress = p;
    if (!p.running || p.done - lastRebuild >= 25) {
      lastRebuild = p.done;
      load(false);
    }
  });
  onDestroy(() => {
    unlisten.then((f) => f());
  });

  async function saveKey(key: string | null) {
    keyMessage = "";
    try {
      keyStatus = await api.setApiKey(key);
      keyInput = "";
      keyMessage = key
        ? `Saved to ${keyStatus.storage === "credentialManager" ? "Windows Credential Manager" : "the app's config folder"}.`
        : "Key removed.";
      await load(true);
    } catch (e) {
      keyMessage = (e as Error).message;
    }
  }

  async function rebuild() {
    await api.rebuildMetadata();
    lastRebuild = 0;
    await load(false);
  }

  load(false);
  api.getMetadataProgress().then((p) => (progress = p));
  api.getApiKeyStatus().then((s) => (keyStatus = s));
</script>

<section class="debug">
  <header>
    <button class="btn" onclick={onBack}>← {branding.copy.back}</button>
    <h2>Library debug</h2>
    <button class="btn" onclick={rebuild} disabled={loading}>Rebuild tag cache</button>
    <button class="btn" onclick={() => load(true)} disabled={loading}>{loading ? "Scanning…" : "Refresh library"}</button>
  </header>

  <div class="panel">
    <strong>Steam Web API key</strong>
    <span class="dim">
      {keyStatus?.hasKey ? "A key is saved." : "No key: only installed games are shown."}
      Get one at
      <button class="link" onclick={() => openUrl(API_KEY_URL)}>steamcommunity.com/dev/apikey</button>
      (Domain name: type <code>localhost</code>).
    </span>
    <div class="row">
      <input type="password" placeholder="Paste your key" bind:value={keyInput} autocomplete="off" />
      <button class="btn" onclick={() => saveKey(keyInput)} disabled={!keyInput.trim()}>Save</button>
      {#if keyStatus?.hasKey}<button class="btn" onclick={() => saveKey(null)}>Remove</button>{/if}
    </div>
    {#if keyMessage}<span class="dim">{keyMessage}</span>{/if}
  </div>

  {#if progress && (progress.running || progress.error)}
    <p class="progress">
      {#if progress.running}
        Learning your library… {progress.done}/{progress.total}
        {#if progress.note}<span class="dim"> {progress.note}</span>{/if}
      {:else}
        {progress.error}
      {/if}
    </p>
  {/if}

  {#if error}
    <p class="error">{error}</p>
  {/if}

  {#if view}
    <dl class="meta">
      <dt>Steam</dt>
      <dd>{view.steam.steamPath} <span class="tag">{view.steam.steamPathSource}</span></dd>
      <dt>Libraries</dt>
      <dd>
        {#each view.steam.libraries as lib}
          <div class:dim={!lib.reachable}>
            {lib.path} — {lib.reachable ? `${lib.gameCount} manifests` : "unreachable"}
            {#if lib.origin === "settings"}<span class="tag">settings</span>{/if}
          </div>
        {/each}
      </dd>
      <dt>Account</dt>
      <dd>
        {#each view.steam.accounts as acc}
          <div>
            {acc.personaName} ({acc.steamId})
            {#if acc.steamId === view.steam.selectedAccount}<span class="tag accent">selected</span>{/if}
          </div>
        {:else}
          <div class="dim">None found</div>
        {/each}
      </dd>
      <dt>Web API</dt>
      <dd>
        {#if view.api.kind === "ok"}{view.api.count} owned games
        {:else if view.api.kind === "cached"}{view.api.count} owned games (cached) — {view.api.message}
        {:else if view.api.kind === "error"}<span class="bad">{view.api.message}</span>
        {:else if view.api.kind === "noAccount"}Key set, but no Steam account found
        {:else}Not set up{/if}
      </dd>
      <dt>Games</dt>
      <dd>
        {view.games.length} games, {installedCount} installed,
        <button class="link" onclick={() => (showHidden = !showHidden)}>{view.hidden.length} hidden</button>
      </dd>
    </dl>

    {#if showHidden}
      <ul class="hidden-list">
        {#each view.hidden as h}<li>{h.name} <span class="dim">({h.appId}) — {h.reason}</span></li>{/each}
      </ul>
    {/if}

    {#if view.warnings.length}
      <ul class="warnings">
        {#each view.warnings as w}<li>{w}</li>{/each}
      </ul>
    {/if}

    <input class="search" placeholder="Filter by name…" bind:value={query} />

    <table>
      <thead>
        <tr><th>App ID</th><th>Name</th><th>Status</th><th>Size</th><th>Playtime</th><th>Meta</th><th>Genres</th><th>Tags</th></tr>
      </thead>
      <tbody>
        {#each games as g (g.appId)}
          <tr>
            <td class="num">{g.appId}</td>
            <td>{g.name}</td>
            <td class:dim={!g.installed}>{g.installed ? "Installed" : "Not installed"}</td>
            <td class="num">{formatSize(g.sizeOnDisk)}</td>
            <td class="num">{formatPlaytime(g.playtimeMinutes)}</td>
            <td class:dim={g.meta !== "known"}>{g.meta}</td>
            <td class="small">{g.genres.join(", ")}</td>
            <td class="small">{g.tags.slice(0, 4).join(", ")}</td>
          </tr>
        {/each}
      </tbody>
    </table>
  {:else if loading}
    <p class="dim">Scanning your Steam libraries…</p>
  {/if}
</section>

<style>
  .debug {
    height: 100%;
    overflow-y: auto;
    padding: 24px 32px 40px;
    user-select: text;
  }

  header {
    display: flex;
    align-items: center;
    gap: 12px;
    margin-bottom: 16px;
  }

  h2 {
    margin: 0;
    flex: 1;
    font-size: 24px;
  }

  .btn {
    padding: 8px 12px;
    border-radius: 8px;
    background: var(--surface);
    border: 1px solid var(--border);
    white-space: nowrap;
  }

  .btn:hover:not(:disabled) {
    background: var(--surface-hover);
  }

  .btn:disabled {
    opacity: 0.5;
    cursor: default;
  }

  .link {
    color: var(--accent);
    text-decoration: underline;
    text-underline-offset: 2px;
    padding: 0;
  }

  .panel {
    display: grid;
    gap: 8px;
    padding: 14px 16px;
    margin-bottom: 16px;
    border-radius: 10px;
    background: var(--surface);
    border: 1px solid var(--border);
    font-size: 13px;
  }

  .row {
    display: flex;
    gap: 8px;
  }

  input {
    padding: 8px 12px;
    border-radius: 8px;
    border: 1px solid var(--border);
    background: rgba(0, 0, 0, 0.2);
    color: var(--text);
    font: inherit;
  }

  .row input {
    flex: 1;
    max-width: 360px;
  }

  code {
    color: var(--text);
  }

  .progress {
    font-size: 13px;
    color: var(--accent);
  }

  .error {
    padding: 12px 16px;
    border-radius: 10px;
    background: rgba(229, 72, 77, 0.12);
    border: 1px solid rgba(229, 72, 77, 0.4);
  }

  .bad {
    color: #ff8a8e;
  }

  .meta {
    display: grid;
    grid-template-columns: 100px 1fr;
    gap: 6px 16px;
    margin: 0 0 16px;
    font-size: 13px;
  }

  dt {
    color: var(--text-dim);
  }

  dd {
    margin: 0;
  }

  .tag {
    font-size: 11px;
    padding: 1px 6px;
    border-radius: 4px;
    background: var(--surface-hover);
    color: var(--text-dim);
    margin-left: 4px;
  }

  .tag.accent {
    color: var(--accent);
  }

  .hidden-list,
  .warnings {
    margin: 0 0 16px;
    padding: 10px 14px 10px 30px;
    border-radius: 10px;
    font-size: 13px;
  }

  .hidden-list {
    background: var(--surface);
    border: 1px solid var(--border);
  }

  .warnings {
    background: rgba(245, 180, 60, 0.08);
    border: 1px solid rgba(245, 180, 60, 0.3);
  }

  .search {
    width: 100%;
    max-width: 360px;
    margin-bottom: 12px;
    background: var(--surface);
  }

  table {
    width: 100%;
    border-collapse: collapse;
    font-size: 13px;
  }

  th {
    text-align: left;
    font-weight: 600;
    color: var(--text-dim);
    padding: 6px 8px;
    border-bottom: 1px solid var(--border);
    position: sticky;
    top: -24px;
    background: var(--bg-1);
  }

  td {
    padding: 5px 8px;
    border-bottom: 1px solid rgba(255, 255, 255, 0.03);
  }

  .num {
    font-variant-numeric: tabular-nums;
    white-space: nowrap;
  }

  .small {
    font-size: 12px;
    color: var(--text-dim);
  }

  .dim {
    color: var(--text-dim);
  }
</style>
