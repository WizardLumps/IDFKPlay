<script lang="ts">
  import { api, type GameArt, type LibraryGame, type Mode, type Mood, type PickOutcome, type ReelItem } from "../lib/api";
  import { branding } from "../lib/branding";
  import Reveal from "../lib/Reveal.svelte";
  import Backdrop from "../lib/Backdrop.svelte";
  import { app, hasOwnedGames, installedOnly, updateSettings } from "../lib/state.svelte";

  interface Props {
    onSettings: () => void;
  }

  let { onSettings }: Props = $props();

  const c = branding.copy;
  const MOODS: Mood[] = ["action", "adventure", "puzzle", "rpg", "strategy", "multiplayer", "controller", "vr"];

  /** Line under a pick: snark that escalates with rerolls spent, or the lock message. */
  function pickLine(out: PickOutcome): string {
    if (out.locked) return c.lockedPick;
    if (!out.charged) return "";
    // 1st reroll -> rerolls[0], 2nd -> rerolls[1], then the last line repeats.
    return c.rerolls[Math.min(Math.max(out.rerollsUsed - 1, 0), c.rerolls.length - 1)] ?? "";
  }

  // Chip selections last for this session only.
  let moods = $state<Mood[]>([]);
  let collections = $state<string[]>([]);
  let message = $state("");
  let flashFilters = $state(false);

  let reveal = $state<{
    game: LibraryGame;
    art: GameArt;
    reel: ReelItem[];
    spinKey: number;
    mode: Mode;
    shown: number[];
    /** null = unlimited. */
    rerollsLeft: number | null;
    line: string;
    status: string;
    error: string;
    busy: boolean;
    launched: boolean;
  } | null>(null);

  let spinCounter = 0;

  /** Shows a new pick in the open reveal (replays the spin). */
  function showPick(r: NonNullable<typeof reveal>, out: PickOutcome) {
    if (!out.game || !out.art) return;
    r.game = out.game;
    r.art = out.art;
    r.reel = out.reel;
    r.spinKey = ++spinCounter;
    r.shown = [...r.shown, out.game.appId];
  }

  const forcedInstalled = $derived(app.view !== null && !hasOwnedGames(app.view));
  const showApiBanner = $derived(
    app.view !== null && app.view.api.kind === "noKey" && app.settings !== null && !app.settings.hideApiBanner,
  );
  const ready = $derived(app.view !== null && app.settings !== null);

  function toggleMood(m: Mood) {
    moods = moods.includes(m) ? moods.filter((x) => x !== m) : [...moods, m];
    message = "";
  }

  function toggleCollection(id: string) {
    collections = collections.includes(id) ? collections.filter((x) => x !== id) : [...collections, id];
    message = "";
  }

  // Only collections that still exist (they change if you switch Steam accounts).
  const activeCollections = $derived(collections.filter((id) => app.view?.collections.some((c) => c.id === id)));

  function request(mode: Mode, exclude: number[] = []) {
    return { mode, moods: [...moods], installedOnly: installedOnly(), exclude, collections: [...activeCollections] };
  }

  async function pick(mode: Mode) {
    message = "";
    try {
      // After Cancel, a fresh pick costs a reroll (or brings the same game back).
      const out = await api.pickGame(request(mode), "fresh");
      if (!out.game || !out.art) {
        const filtered = moods.length > 0 || activeCollections.length > 0 || mode === "somethingNew" || installedOnly();
        message = filtered ? c.emptyResult : c.emptyLibrary;
        flashFilters = filtered;
        setTimeout(() => (flashFilters = false), 1200);
        return;
      }
      reveal = {
        game: out.game,
        art: out.art,
        reel: out.reel,
        spinKey: ++spinCounter,
        mode,
        shown: [out.game.appId],
        rerollsLeft: out.rerollsLeft,
        line: pickLine(out),
        status: "",
        error: "",
        busy: false,
        launched: false,
      };
    } catch (e) {
      message = (e as Error).message;
    }
  }

  async function reroll() {
    if (!reveal) return;
    const r = reveal;
    r.busy = true;
    r.error = "";
    try {
      const out = await api.pickGame(request(r.mode, r.shown), "reroll");
      r.rerollsLeft = out.rerollsLeft;
      if (out.game) {
        showPick(r, out);
        r.line = pickLine(out);
      }
    } catch (e) {
      r.error = (e as Error).message;
    } finally {
      r.busy = false;
    }
  }

  async function blacklist() {
    if (!reveal) return;
    const r = reveal;
    r.busy = true;
    r.error = "";
    try {
      app.settings = await api.setBlacklisted(r.game.appId, true);
      // A free replacement: blacklisting doesn't cost a reroll.
      const out = await api.pickGame(request(r.mode, r.shown), "replace");
      if (out.game) {
        showPick(r, out);
        r.rerollsLeft = out.rerollsLeft;
        r.line = "";
      } else {
        reveal = null;
        message = c.emptyResult;
      }
    } catch (e) {
      r.error = (e as Error).message;
    } finally {
      r.busy = false;
    }
  }

  async function launch() {
    if (!reveal || reveal.launched) return;
    const r = reveal;
    r.launched = true;
    r.error = "";
    r.status = c.launching;
    try {
      const out = await api.launchGame(r.game.appId);
      if (!out.closing) {
        r.status = c.launched;
        setTimeout(() => {
          if (reveal === r) reveal = null;
        }, 1500);
      }
    } catch (e) {
      // Let the user retry by hand; the countdown stays stopped while an error shows.
      r.status = "";
      r.error = (e as Error).message;
      r.launched = false;
    }
  }

  function toggleInstalled() {
    if (forcedInstalled) return;
    updateSettings({ installedOnly: !installedOnly() });
  }
</script>

<section class="main">
  <Backdrop paused={reveal !== null} />

  <div class="intro">
    <h1>{branding.appName}</h1>
    <p class="tagline">{branding.tagline}</p>
  </div>

  {#if app.libraryError}
    <div class="banner error">
      <span>{app.libraryError}</span>
      <button onclick={onSettings}>{c.apiBannerAction}</button>
    </div>
  {:else if showApiBanner}
    <div class="banner">
      <span>{c.apiBanner}</span>
      <button onclick={onSettings}>{c.apiBannerAction}</button>
      <button class="x" aria-label={c.dismiss} title={c.dismiss} onclick={() => updateSettings({ hideApiBanner: true })}>×</button>
    </div>
  {/if}

  <div class="actions">
    <button class="big primary" onclick={() => pick("justPlay")} disabled={!ready}>
      <span class="label">{c.justPlay}</span>
      <span class="sub">{c.justPlaySub}</span>
    </button>
    <button class="big secondary" onclick={() => pick("somethingNew")} disabled={!ready}>
      <span class="label">{c.somethingNew}</span>
      <span class="sub">
        {c.somethingNewSub}
        {#if app.settings}<span class="nowrap">(under {app.settings.somethingNewHours}&nbsp;h)</span>{/if}
      </span>
    </button>
  </div>

  <div class="filters" class:flash={flashFilters}>
    <button
      class="switch"
      role="switch"
      aria-checked={installedOnly()}
      disabled={forcedInstalled}
      title={forcedInstalled ? c.installedOnlyForced : ""}
      onclick={toggleInstalled}
    >
      <span class="track"><span class="thumb"></span></span>
      {c.installedOnly}
    </button>
    <span class="sep"></span>
    {#each MOODS as m}
      <button class="chip" class:on={moods.includes(m)} aria-pressed={moods.includes(m)} onclick={() => toggleMood(m)}>
        {c.moods[m]}
      </button>
    {/each}
  </div>

  {#if app.view?.collections.length}
    <div class="filters collections" class:flash={flashFilters}>
      <span class="row-label">{c.collections}</span>
      {#each app.view.collections as col (col.id)}
        <button
          class="chip coll"
          class:on={activeCollections.includes(col.id)}
          aria-pressed={activeCollections.includes(col.id)}
          title={col.dynamic ? "Dynamic collection in Steam: uses the games Steam saved with it" : ""}
          onclick={() => toggleCollection(col.id)}
        >
          {#if col.id === "favorite"}<span class="star" aria-hidden="true">★</span>{/if}
          {col.id === "favorite" ? c.favorites : col.name}
          <span class="count">{col.count}</span>
        </button>
      {/each}
    </div>
  {/if}

  <p class="status" aria-live="polite">
    {#if message}
      <span class="msg">{message}</span>
    {:else if app.loading && !app.view}
      {c.loading}
    {:else if app.progress?.running}
      {c.learning} {app.progress.done}/{app.progress.total}
      {#if app.progress.note}<br /><span class="dim">{app.progress.note}</span>
      {:else if app.learningSlow}<br /><span class="dim">{c.learningSlow}</span>{/if}
    {:else if forcedInstalled && app.view}
      <span class="dim">{c.installedOnlyForced}</span>
    {/if}
  </p>

  {#if reveal}
    <Reveal
      game={reveal.game}
      art={reveal.art}
      reel={reveal.reel}
      spinKey={reveal.spinKey}
      rerollsLeft={reveal.rerollsLeft}
      line={reveal.line}
      status={reveal.status}
      error={reveal.error}
      busy={reveal.busy}
      launched={reveal.launched}
      onLaunch={launch}
      onReroll={reroll}
      onCancel={() => (reveal = null)}
      onBlacklist={blacklist}
    />
  {/if}
</section>

<style>
  .main {
    position: relative;
    height: 100%;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 28px;
    padding: 40px 48px;
  }

  /* Content sits above the backdrop (which is absolutely positioned at z-index 0). */
  .intro,
  .banner,
  .actions,
  .filters,
  .status {
    position: relative;
    z-index: 1;
  }

  .intro {
    text-align: center;
  }

  h1 {
    margin: 0;
    font-size: clamp(44px, 6.5vw, 76px);
    font-weight: 800;
    letter-spacing: -0.03em;
    line-height: 1;
    background: linear-gradient(180deg, #ffffff 0%, var(--accent) 140%);
    -webkit-background-clip: text;
    background-clip: text;
    color: transparent;
  }

  .tagline {
    margin: 12px 0 0;
    color: var(--text-dim);
    font-size: 16px;
  }

  .banner {
    display: flex;
    align-items: center;
    gap: 12px;
    max-width: 760px;
    width: 100%;
    padding: 10px 12px 10px 16px;
    border-radius: 12px;
    background: rgba(102, 192, 244, 0.08);
    border: 1px solid rgba(102, 192, 244, 0.25);
    font-size: 13px;
  }

  .banner span {
    flex: 1;
  }

  .banner.error {
    background: rgba(229, 72, 77, 0.1);
    border-color: rgba(229, 72, 77, 0.4);
  }

  .banner button {
    padding: 6px 10px;
    border-radius: 8px;
    background: var(--surface-hover);
    white-space: nowrap;
  }

  .banner .x {
    padding: 2px 8px;
    font-size: 18px;
    line-height: 1;
    background: none;
    color: var(--text-dim);
  }

  .actions {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 20px;
    width: min(760px, 100%);
  }

  .big {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 6px;
    padding: 30px 32px;
    border-radius: var(--radius);
    text-align: left;
    transition:
      transform var(--t-fast) var(--ease),
      box-shadow var(--t-med) var(--ease),
      background var(--t-med) var(--ease);
  }

  .big:hover:not(:disabled) {
    transform: translateY(-2px);
  }

  .big:active:not(:disabled) {
    transform: translateY(0) scale(0.99);
  }

  .big:disabled {
    opacity: 0.6;
    cursor: progress;
  }

  .label {
    font-size: 28px;
    font-weight: 750;
    letter-spacing: -0.01em;
  }

  .sub {
    font-size: 14px;
    opacity: 0.8;
  }

  .nowrap {
    white-space: nowrap;
  }

  .collections {
    margin-top: -14px; /* sits close under the mood row */
  }

  .row-label {
    font-size: 12px;
    font-weight: 600;
    letter-spacing: 0.06em;
    text-transform: uppercase;
    color: var(--text-dim);
    margin-right: 4px;
  }

  .chip.coll {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    background: transparent;
    border-style: dashed;
  }

  .chip.coll.on {
    border-style: solid;
  }

  .star {
    color: #f5c451;
  }

  .chip.coll.on .star {
    color: #0e1a26;
  }

  .count {
    font-size: 11px;
    opacity: 0.65;
    font-variant-numeric: tabular-nums;
  }

  /* Pressed chips already have a solid border; keep the keyboard ring from doubling it. */
  .chip:focus-visible {
    outline-offset: 3px;
  }

  .primary {
    background: linear-gradient(135deg, #7fd0ff 0%, var(--accent-strong) 100%);
    color: #0e1a26;
    box-shadow: 0 10px 30px rgba(102, 192, 244, 0.25);
  }

  .primary:hover:not(:disabled) {
    box-shadow: 0 14px 40px rgba(102, 192, 244, 0.4);
  }

  .secondary {
    background: var(--surface);
    border: 1px solid var(--border);
    backdrop-filter: blur(8px);
  }

  .secondary:hover:not(:disabled) {
    background: var(--surface-hover);
    border-color: rgba(102, 192, 244, 0.4);
  }

  .filters {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    justify-content: center;
    gap: 8px;
    max-width: 860px;
  }

  .sep {
    width: 1px;
    height: 22px;
    margin: 0 6px;
    background: var(--border);
  }

  .chip {
    padding: 7px 14px;
    border-radius: 999px;
    font-size: 13px;
    font-weight: 550;
    background: var(--surface);
    border: 1px solid var(--border);
    color: var(--text-dim);
    transition:
      background var(--t-fast) var(--ease),
      color var(--t-fast) var(--ease),
      border-color var(--t-fast) var(--ease),
      box-shadow var(--t-med) var(--ease);
  }

  .chip:hover {
    color: var(--text);
    background: var(--surface-hover);
  }

  .chip.on {
    color: #0e1a26;
    background: var(--accent);
    border-color: var(--accent);
  }

  /* Empty result: pulse the active filters so it's obvious what to loosen. */
  .flash .chip.on,
  .flash .switch[aria-checked="true"] {
    animation: pulse 0.4s var(--ease) 3;
  }

  .switch {
    display: inline-flex;
    align-items: center;
    gap: 8px;
    font-size: 13px;
    font-weight: 550;
    padding: 6px 4px;
  }

  .switch:disabled {
    opacity: 0.6;
    cursor: not-allowed;
  }

  .track {
    position: relative;
    width: 34px;
    height: 20px;
    border-radius: 999px;
    background: var(--surface-hover);
    border: 1px solid var(--border);
    transition: background var(--t-fast) var(--ease);
  }

  .thumb {
    position: absolute;
    top: 2px;
    left: 2px;
    width: 14px;
    height: 14px;
    border-radius: 50%;
    background: var(--text-dim);
    transition:
      transform var(--t-fast) var(--ease),
      background var(--t-fast) var(--ease);
  }

  .switch[aria-checked="true"] .track {
    background: rgba(102, 192, 244, 0.35);
    border-color: var(--accent);
  }

  .switch[aria-checked="true"] .thumb {
    transform: translateX(14px);
    background: var(--accent);
  }

  .status {
    margin: 0;
    min-height: 1.5em;
    font-size: 13px;
    line-height: 1.6;
    text-align: center;
    color: var(--text-dim);
  }

  .msg {
    color: var(--text);
    font-size: 15px;
    font-weight: 600;
  }

  .dim {
    opacity: 0.8;
  }

  @keyframes pulse {
    50% {
      box-shadow: 0 0 0 4px rgba(102, 192, 244, 0.45);
    }
  }
</style>
