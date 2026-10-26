<script lang="ts">
  import { openUrl } from "@tauri-apps/plugin-opener";
  import { api, API_KEY_URL, MAX_REROLLS } from "../lib/api";
  import { branding } from "../lib/branding";
  import { app, loadLibrary, updateSettings } from "../lib/state.svelte";

  interface Props {
    onBack: () => void;
    onDebug: () => void;
  }

  let { onBack, onDebug }: Props = $props();

  // Drafts for text fields; saved on explicit action so half-typed paths don't trigger rescans.
  let steamPathDraft = $state(app.settings?.steamPath ?? "");
  let newLibrary = $state("");
  let keyDraft = $state("");
  let hoursDraft = $state(String(app.settings?.somethingNewHours ?? 2));

  // Reroll slider: 0..MAX_REROLLS, and one step past the end means unlimited (saved as null).
  let rerollSlider = $state(
    app.settings?.rerollsPerSession === null ? MAX_REROLLS + 1 : (app.settings?.rerollsPerSession ?? 2),
  );
  const rerollLabel = $derived(
    rerollSlider > MAX_REROLLS ? "Unlimited" : `${rerollSlider} ${rerollSlider === 1 ? "reroll" : "rerolls"}`,
  );

  function saveRerolls() {
    const n = Number(rerollSlider);
    updateSettings({ rerollsPerSession: n > MAX_REROLLS ? null : n });
  }

  // Per-section feedback.
  let note = $state<Record<string, { text: string; bad?: boolean }>>({});

  function say(section: string, text: string, bad = false) {
    note[section] = { text, bad };
  }

  async function run(section: string, fn: () => Promise<void>, ok = "") {
    say(section, "");
    try {
      await fn();
      if (ok) say(section, ok);
    } catch (e) {
      say(section, (e as Error).message, true);
    }
  }

  const steam = $derived(app.view?.steam ?? null);
  const sourceLabel = $derived(
    steam?.steamPathSource === "settings"
      ? "set here"
      : steam?.steamPathSource === "registry"
        ? "auto-detected"
        : steam
          ? "default location"
          : "",
  );

  const names = $derived.by(() => {
    const m = new Map<number, string>();
    for (const g of app.view?.games ?? []) m.set(g.appId, g.name);
    for (const g of app.view?.hidden ?? []) m.set(g.appId, g.name);
    return m;
  });

  const apiLine = $derived.by(() => {
    const a = app.view?.api;
    if (!a) return { text: "", bad: false };
    switch (a.kind) {
      case "ok":
        return { text: `Working: ${a.count} owned games found.`, bad: false };
      case "cached":
        return { text: `Using the last saved list (${a.count} games). ${a.message}`, bad: true };
      case "error":
        return { text: a.message, bad: true };
      case "noAccount":
        return { text: "Key saved, but no Steam account was found on this PC.", bad: true };
      default:
        return { text: "No key: only installed games are included.", bad: false };
    }
  });

  function saveSteamPath(value: string | null) {
    steamPathDraft = value ?? "";
    run("steam", () => updateSettings({ steamPath: value }, true), value ? "Saved. Library rescanned." : "Auto-detecting.");
  }

  function addLibrary() {
    const path = newLibrary.trim();
    if (!path || !app.settings) return;
    run("steam", async () => {
      await updateSettings({ extraLibraries: [...app.settings!.extraLibraries, path] }, true);
      newLibrary = "";
    }, "Library added.");
  }

  function removeLibrary(path: string) {
    if (!app.settings) return;
    run("steam", () => updateSettings({ extraLibraries: app.settings!.extraLibraries.filter((p) => p !== path) }, true));
  }

  function chooseAccount(e: Event) {
    const v = (e.currentTarget as HTMLSelectElement).value;
    run("steam", () => updateSettings({ steamId: v || null }, true));
  }

  function saveKey(key: string | null) {
    run("api", async () => {
      app.keyStatus = await api.setApiKey(key);
      keyDraft = "";
      await loadLibrary(true);
    }, key ? "Key saved." : "Key removed.");
  }

  function saveHours() {
    const h = Number(hoursDraft);
    if (h === app.settings?.somethingNewHours) return;
    if (!Number.isFinite(h) || h <= 0) {
      say("new", "Enter a number of hours above 0.", true);
      return;
    }
    run("new", async () => {
      await updateSettings({ somethingNewHours: h });
      hoursDraft = String(app.settings?.somethingNewHours ?? h);
    }, "Saved.");
  }

  function unblock(appId: number) {
    run("blacklist", async () => {
      app.settings = await api.setBlacklisted(appId, false);
    });
  }

  function rebuildTags() {
    run("library", () => api.rebuildMetadata(), "Rebuilding tags in the background.");
  }
</script>

<section class="settings">
  <header>
    <button class="back" onclick={onBack}>← {branding.copy.back}</button>
    <h2>{branding.copy.settings}</h2>
  </header>

  {#if !app.settings}
    <p class="dim">Loading…</p>
  {:else}
    <!-- Steam -->
    <div class="card">
      <h3>Steam</h3>
      <label class="field">
        <span>Steam folder {#if sourceLabel}<em>({sourceLabel})</em>{/if}</span>
        <div class="row">
          <input
            bind:value={steamPathDraft}
            placeholder={steam?.steamPath ?? "C:\\Program Files (x86)\\Steam"}
            spellcheck="false"
            onkeydown={(e) => e.key === "Enter" && saveSteamPath(steamPathDraft.trim() || null)}
          />
          <button onclick={() => saveSteamPath(steamPathDraft.trim() || null)}>Save</button>
          {#if app.settings.steamPath}
            <button onclick={() => saveSteamPath(null)}>Auto-detect</button>
          {/if}
        </div>
        {#if steam}<small class="dim">Using {steam.steamPath}</small>{/if}
      </label>

      <div class="field">
        <span>Extra library folders <em>(only if Steam doesn't list them itself)</em></span>
        {#each app.settings.extraLibraries as lib}
          <div class="row item">
            <code>{lib}</code>
            <button onclick={() => removeLibrary(lib)}>Remove</button>
          </div>
        {/each}
        <div class="row">
          <input
            bind:value={newLibrary}
            placeholder="D:\SteamLibrary"
            spellcheck="false"
            onkeydown={(e) => e.key === "Enter" && addLibrary()}
          />
          <button onclick={addLibrary} disabled={!newLibrary.trim()}>Add</button>
        </div>
        {#if steam}
          <small class="dim">
            Scanning {steam.libraries.filter((l) => l.reachable).length} of {steam.libraries.length} libraries.
          </small>
        {/if}
      </div>

      <label class="field">
        <span>Steam account</span>
        <select value={app.settings.steamId ?? ""} onchange={chooseAccount} disabled={!steam?.accounts.length}>
          <option value="">Most recent login</option>
          {#each steam?.accounts ?? [] as acc}
            <option value={acc.steamId}>{acc.personaName} ({acc.accountName})</option>
          {/each}
        </select>
      </label>
      {#if note.steam?.text}<p class="note" class:bad={note.steam.bad}>{note.steam.text}</p>{/if}
    </div>

    <!-- API key -->
    <div class="card">
      <h3>Steam Web API key</h3>
      <p class="dim">
        Unlocks games you own but haven't installed. Get a free key at
        <button class="link" onclick={() => openUrl(API_KEY_URL)}>steamcommunity.com/dev/apikey</button>.
        Domain name: type <code>localhost</code>.
      </p>
      <div class="row">
        <input
          type="password"
          bind:value={keyDraft}
          placeholder={app.keyStatus?.hasKey ? "•••••••• (saved)" : "Paste your key"}
          autocomplete="off"
          onkeydown={(e) => e.key === "Enter" && keyDraft.trim() && saveKey(keyDraft)}
        />
        <button onclick={() => saveKey(keyDraft)} disabled={!keyDraft.trim()}>Save</button>
        {#if app.keyStatus?.hasKey}<button onclick={() => saveKey(null)}>Remove</button>{/if}
      </div>
      {#if apiLine.text}<p class="note" class:bad={apiLine.bad}>{apiLine.text}</p>{/if}
      {#if note.api?.text}<p class="note" class:bad={note.api.bad}>{note.api.text}</p>{/if}
    </div>

    <!-- Something New -->
    <div class="card">
      <h3>Something New</h3>
      <label class="field">
        <span>Counts as new if played less than</span>
        <div class="row">
          <input
            class="short"
            type="number"
            min="0.1"
            step="0.5"
            bind:value={hoursDraft}
            onkeydown={(e) => e.key === "Enter" && saveHours()}
            onblur={saveHours}
          />
          <span class="dim">hours</span>
        </div>
      </label>
      {#if note.new?.text}<p class="note" class:bad={note.new.bad}>{note.new.text}</p>{/if}
    </div>

    <!-- Rerolls -->
    <div class="card">
      <h3>Rerolls</h3>
      <p class="dim">
        How many times per session you can ask for a different game. Once they're used up, canceling and picking
        again brings the same game back. Launching a game resets the cycle.
      </p>
      <div class="slider">
        <input
          type="range"
          min="0"
          max={MAX_REROLLS + 1}
          step="1"
          bind:value={rerollSlider}
          onchange={saveRerolls}
          aria-label="Rerolls per session"
          aria-valuetext={rerollLabel}
        />
        <span class="slider-value">{rerollLabel}</span>
      </div>
      <div class="ticks dim">
        <span style="left: 0%">0</span>
        <span style="left: {(MAX_REROLLS / (MAX_REROLLS + 1)) * 100}%">{MAX_REROLLS}</span>
        <span style="left: 100%">∞</span>
      </div>
    </div>

    <!-- After launch -->
    <div class="card">
      <h3>After launching a game</h3>
      <div class="segmented" role="radiogroup">
        <button
          role="radio"
          aria-checked={app.settings.afterLaunch === "close"}
          onclick={() => updateSettings({ afterLaunch: "close" })}>Close {branding.appName}</button
        >
        <button
          role="radio"
          aria-checked={app.settings.afterLaunch === "stayOpen"}
          onclick={() => updateSettings({ afterLaunch: "stayOpen" })}>Stay open</button
        >
      </div>
    </div>

    <!-- Blacklist -->
    <div class="card">
      <h3>Never show again</h3>
      <label class="check">
        <input
          type="checkbox"
          checked={app.settings.skipSteamHidden}
          onchange={(e) => updateSettings({ skipSteamHidden: e.currentTarget.checked })}
        />
        <span>
          Skip games hidden in Steam
          {#if app.view}<em>({app.view.steamHidden.length} hidden)</em>{/if}
        </span>
      </label>
      {#each app.settings.blacklist as id (id)}
        <div class="row item">
          <span>{names.get(id) ?? `App ${id}`}</span>
          <button onclick={() => unblock(id)}>Unblock</button>
        </div>
      {:else}
        <p class="dim">Nothing blocked yet. Use "{branding.copy.blacklist}" on a pick.</p>
      {/each}
      {#if note.blacklist?.text}<p class="note" class:bad={note.blacklist.bad}>{note.blacklist.text}</p>{/if}
    </div>

    <!-- Library -->
    <div class="card">
      <h3>Library</h3>
      <p class="dim">
        {#if app.view}
          {app.view.games.length} games ({app.view.games.filter((g) => g.installed).length} installed),
          {app.view.hidden.length} non-games hidden.
        {/if}
        {#if app.progress?.running}
          {branding.copy.learning} {app.progress.done}/{app.progress.total}
        {:else if app.progress?.error}
          {app.progress.error}
        {/if}
      </p>
      <div class="row">
        <button onclick={() => run("library", () => loadLibrary(true), "Library refreshed.")} disabled={app.loading}>
          {app.loading ? "Refreshing…" : "Refresh library"}
        </button>
        <button onclick={rebuildTags}>Rebuild tag cache</button>
        <button class="link" onclick={onDebug}>Library details</button>
      </div>
      {#if app.libraryError}<p class="note bad">{app.libraryError}</p>{/if}
      {#if note.library?.text}<p class="note" class:bad={note.library.bad}>{note.library.text}</p>{/if}
    </div>
  {/if}
</section>

<style>
  .settings {
    height: 100%;
    overflow-y: auto;
    padding: 32px 48px 48px;
    max-width: 860px;
    margin: 0 auto;
    user-select: text;
  }

  header {
    display: flex;
    align-items: center;
    gap: 16px;
    margin-bottom: 24px;
  }

  h2 {
    margin: 0;
    font-size: 28px;
    font-weight: 750;
    letter-spacing: -0.01em;
  }

  .back {
    padding: 8px 12px;
    border-radius: 8px;
    color: var(--text-dim);
    transition:
      background var(--t-fast) var(--ease),
      color var(--t-fast) var(--ease);
  }

  .back:hover {
    background: var(--surface-hover);
    color: var(--text);
  }

  .card {
    display: grid;
    gap: 12px;
    padding: 20px 22px;
    margin-bottom: 12px;
    border-radius: var(--radius);
    background: var(--surface);
    border: 1px solid var(--border);
  }

  h3 {
    margin: 0;
    font-size: 16px;
    font-weight: 650;
  }

  .field {
    display: grid;
    gap: 6px;
  }

  .field > span {
    font-size: 13px;
    color: var(--text-dim);
  }

  em {
    font-style: normal;
    opacity: 0.75;
  }

  .row {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .row.item {
    justify-content: space-between;
    padding: 6px 10px;
    border-radius: 8px;
    background: rgba(0, 0, 0, 0.15);
  }

  input,
  select {
    flex: 1;
    min-width: 0;
    padding: 9px 12px;
    border-radius: 8px;
    border: 1px solid var(--border);
    background: rgba(0, 0, 0, 0.25);
    color: var(--text);
    font: inherit;
    font-size: 14px;
  }

  input.short {
    flex: 0 0 100px;
  }

  select option {
    background: var(--bg-1);
  }

  .row button,
  .segmented button {
    padding: 9px 14px;
    border-radius: 8px;
    background: var(--surface-hover);
    border: 1px solid var(--border);
    font-size: 14px;
    white-space: nowrap;
    transition: background var(--t-fast) var(--ease);
  }

  .row button:hover:not(:disabled) {
    background: rgba(255, 255, 255, 0.12);
  }

  button:disabled {
    opacity: 0.5;
    cursor: default;
  }

  .row button.link,
  .link {
    background: none;
    border: none;
    padding: 0 4px;
    color: var(--accent);
    text-decoration: underline;
    text-underline-offset: 3px;
    font-size: inherit;
  }

  .slider {
    display: flex;
    align-items: center;
    gap: 16px;
  }

  .slider input {
    flex: 1;
    padding: 0;
    border: none;
    background: none;
    accent-color: var(--accent);
    cursor: pointer;
  }

  .slider-value {
    flex: 0 0 96px;
    text-align: right;
    font-weight: 650;
    font-variant-numeric: tabular-nums;
    color: var(--accent);
  }

  /* Labels under the slider track, each centred on its step. The inset keeps them under
     the thumb's travel (the track minus half a thumb at each end); the right margin skips
     the value label beside the slider. */
  .ticks {
    position: relative;
    height: 16px;
    margin: -4px calc(112px + 8px) 0 8px;
    font-size: 12px;
  }

  .ticks span {
    position: absolute;
    transform: translateX(-50%);
  }

  .check {
    display: flex;
    align-items: center;
    gap: 10px;
    font-size: 14px;
    cursor: pointer;
  }

  .check input {
    flex: 0 0 auto;
    width: 16px;
    height: 16px;
    padding: 0;
    accent-color: var(--accent);
  }

  .segmented {
    display: inline-flex;
    gap: 6px;
  }

  .segmented button[aria-checked="true"] {
    background: rgba(102, 192, 244, 0.2);
    border-color: var(--accent);
    color: var(--accent);
  }

  code {
    font-size: 13px;
    color: var(--text);
  }

  p {
    margin: 0;
    font-size: 14px;
  }

  small {
    font-size: 12px;
  }

  .note {
    font-size: 13px;
    color: var(--accent);
  }

  .note.bad {
    color: #ff8a8e;
  }

  .dim {
    color: var(--text-dim);
  }
</style>
