<script lang="ts">
  // The pick: a slot-machine spin through cover art, then the game's hero art fills
  // the window with a 5-second countdown to auto-launch.
  import { onDestroy } from "svelte";
  import {
    artSrc,
    formatPlaytime,
    formatSize,
    type ArtSource,
    type GameArt,
    type LibraryGame,
    type ReelItem,
  } from "./api";
  import ArtImage from "./ArtImage.svelte";
  import { branding } from "./branding";

  interface Props {
    game: LibraryGame;
    art: GameArt;
    reel: ReelItem[];
    /** Changes for every new pick (first pick, reroll, blacklist replacement): replays the spin. */
    spinKey: number;
    /** null = unlimited. */
    rerollsLeft: number | null;
    /** Snarky line after a reroll. */
    line: string;
    status: string;
    error: string;
    busy: boolean;
    /** Launch requested; stops the countdown for good. */
    launched: boolean;
    onLaunch: () => void;
    onReroll: () => void;
    onCancel: () => void;
    onBlacklist: () => void;
    onFavorite: () => void;
  }

  let {
    game,
    art,
    reel,
    spinKey,
    rerollsLeft,
    line,
    status,
    error,
    busy,
    launched,
    onLaunch,
    onReroll,
    onCancel,
    onBlacklist,
    onFavorite,
  }: Props = $props();

  const c = branding.copy;
  const SPIN_MS = 1500;
  const COUNTDOWN_MS = 5000;
  const PRELOAD_MAX_MS = 800;

  const reducedMotion = window.matchMedia("(prefers-reduced-motion: reduce)").matches;

  let stage = $state<"spin" | "reveal">("spin");
  let heroFailed = $state(false);
  let coverFailed = $state(false);

  const badge = $derived(
    game.installed
      ? c.installed
      : game.sizeOnDisk
        ? `${c.notInstalled} (≈ ${formatSize(game.sizeOnDisk)})`
        : c.notInstalled,
  );
  const blurb = $derived([...game.genres, ...game.tags].filter((v, i, a) => a.indexOf(v) === i).slice(0, 4));

  /** Resolves when the images have loaded (or failed), or after `ms`, whichever comes first. */
  function preload(srcs: string[], ms: number): Promise<void> {
    const all = Promise.all(
      srcs.map(
        (s) =>
          new Promise<void>((resolve) => {
            const img = new Image();
            img.onload = img.onerror = () => resolve();
            img.src = s;
          }),
      ),
    ).then(() => {});
    return Promise.race([all, new Promise<void>((r) => setTimeout(r, ms))]);
  }

  let spinTimer: ReturnType<typeof setTimeout> | undefined;
  // The reel only starts moving once its art had a moment to load.
  let spinning = $state(false);

  const first = (s: ArtSource[]) => (s.length ? [artSrc(s[0])] : []);

  // Every new pick: preload art, spin (unless reduced motion or nothing to spin), then reveal.
  $effect(() => {
    const key = spinKey;
    heroFailed = false;
    coverFailed = false;
    clearTimeout(spinTimer);
    const srcs = [...first(art.portrait), ...first(art.hero), ...reel.flatMap((r) => first(r.portrait))];

    if (reducedMotion || reel.length === 0) {
      stage = "reveal";
      preload(srcs, PRELOAD_MAX_MS);
      return;
    }
    stage = "spin";
    spinning = false;
    preload(srcs, PRELOAD_MAX_MS).then(() => {
      if (key !== spinKey) return;
      spinning = true;
      spinTimer = setTimeout(() => {
        if (key === spinKey) stage = "reveal";
      }, SPIN_MS + 150);
    });
  });

  // Countdown: runs while the reveal is showing and nothing else is going on.
  let remaining = $state(COUNTDOWN_MS);
  $effect(() => {
    if (stage !== "reveal" || busy || error || launched) return;
    const deadline = performance.now() + COUNTDOWN_MS;
    remaining = COUNTDOWN_MS;
    const id = setInterval(() => {
      remaining = Math.max(0, deadline - performance.now());
      if (remaining === 0) {
        clearInterval(id);
        onLaunch();
      }
    }, 50);
    return () => clearInterval(id);
  });

  const seconds = $derived(Math.ceil(remaining / 1000));
  const RING = 2 * Math.PI * 22;
  const counting = $derived(stage === "reveal" && !busy && !error && !launched);

  onDestroy(() => clearTimeout(spinTimer));

  function onKey(e: KeyboardEvent) {
    if (e.key === "Escape" && !launched) onCancel();
    if (stage !== "reveal" || busy || launched) return;
    if (e.key === "Enter") onLaunch();
    if ((e.key === "r" || e.key === "R") && rerollsLeft !== 0) onReroll();
  }

  // Tile size for the reel (kept in sync with the CSS below).
  const TILE_H = 330;
  const TILE_GAP = 18;
</script>

<svelte:window onkeydown={onKey} />

<div class="reveal" role="dialog" aria-modal="true" aria-labelledby="reveal-title">
  {#if stage === "spin"}
    <div class="spin-stage">
      <div class="window">
        {#key spinKey}
          <div
            class="strip"
            class:go={spinning}
            style="--dist: -{reel.length * (TILE_H + TILE_GAP)}px; --spin: {SPIN_MS}ms"
          >
            {#each reel as r, i (i)}
              <div class="tile">
                <span class="tile-name">{r.name}</span>
                <ArtImage sources={r.portrait} alt="" class="tile-img" />
              </div>
            {/each}
            <div class="tile">
              <span class="tile-name">{game.name}</span>
              <ArtImage sources={art.portrait} alt="" class="tile-img" />
            </div>
          </div>
        {/key}
      </div>
      <button class="skip" onclick={onCancel}>{c.cancel}</button>
    </div>
  {:else}
    {#key spinKey}
      <div class="hero-stage">
        <!-- Background: blurred cover always; sharp hero on top when available. -->
        <div class="bg" aria-hidden="true">
          <ArtImage sources={art.portrait} alt="" class="bg-blur" />
          {#if !heroFailed}
            <ArtImage sources={art.hero} alt="" class="bg-hero" onfail={() => (heroFailed = true)} />
          {/if}
          <div class="shade"></div>
        </div>

        <div class="content">
          <div class="cover">
            {#if !coverFailed}
              <ArtImage sources={art.portrait} alt="{game.name} cover" class="cover-img" onfail={() => (coverFailed = true)} />
            {:else}
              <ArtImage sources={art.header} alt="{game.name} banner" class="header-img" />
            {/if}
          </div>

          <div class="info">
            <div class="badges">
              <span class="badge" class:off={!game.installed}>{badge}</span>
              <button
                class="fav"
                class:on={game.favorite}
                aria-pressed={game.favorite}
                title={game.favorite ? "Remove from Favorites" : "Add to Favorites"}
                onclick={onFavorite}
                disabled={busy}>★</button
              >
            </div>
            <h2 id="reveal-title">{game.name}</h2>
            <p class="meta">
              {game.playtimeUnknown ? "Playtime unknown" : formatPlaytime(game.playtimeMinutes)}
              {#if blurb.length}<span class="dot">·</span>{blurb.join(", ")}{/if}
            </p>

            <p class="line" aria-live="polite">
              {#if error}<span class="err">{error}</span>{:else if status}{status}{:else}{line}{/if}
            </p>

            <div class="actions">
              <button class="primary" onclick={onLaunch} disabled={busy || launched}>
                <svg class="ring" viewBox="0 0 50 50" aria-hidden="true">
                  <circle cx="25" cy="25" r="22" class="ring-track" />
                  <circle
                    cx="25"
                    cy="25"
                    r="22"
                    class="ring-fill"
                    style="stroke-dasharray: {RING}; stroke-dashoffset: {counting ? RING * (1 - remaining / COUNTDOWN_MS) : 0}"
                  />
                  <text x="25" y="26" text-anchor="middle" dominant-baseline="middle">{counting ? seconds : "▶"}</text>
                </svg>
                {c.launchNow}
              </button>
              <button
                onclick={onReroll}
                disabled={busy || launched || rerollsLeft === 0}
                title={rerollsLeft === 0 ? c.outOfRerolls : "R"}
              >
                {rerollsLeft === 0 ? c.outOfRerolls : rerollsLeft === null ? c.reroll : `${c.reroll} (${rerollsLeft})`}
              </button>
              <button onclick={onCancel} disabled={launched}>{c.cancel}</button>
            </div>
            <button class="ban" onclick={onBlacklist} disabled={busy || launched}>{c.blacklist}</button>
          </div>
        </div>
      </div>
    {/key}
  {/if}
</div>

<style>
  .reveal {
    position: absolute;
    inset: 0;
    z-index: 5;
    overflow: hidden;
    background: var(--bg-0);
    animation: fade var(--t-med) var(--ease);
  }

  /* ---------- Spin ---------- */

  .spin-stage {
    position: absolute;
    inset: 0;
    display: grid;
    place-items: center;
    background: radial-gradient(600px 400px at 50% 50%, rgba(102, 192, 244, 0.12), transparent 70%);
  }

  .window {
    position: relative;
    width: 220px; /* TILE_H * 2/3 */
    height: 560px;
    overflow: hidden;
    /* Fade neighbours above/below the centre slot. */
    mask-image: linear-gradient(to bottom, transparent 0%, black 30%, black 70%, transparent 100%);
  }

  .strip {
    position: absolute;
    left: 0;
    right: 0;
    top: calc(50% - 165px); /* centre the first tile: TILE_H / 2 */
    display: flex;
    flex-direction: column;
    gap: 18px; /* TILE_GAP */
    will-change: transform;
  }

  .strip.go {
    animation: roll var(--spin) cubic-bezier(0.15, 0.6, 0.2, 1) forwards;
  }

  @keyframes roll {
    0% {
      transform: translateY(0);
      filter: blur(0);
    }
    20% {
      filter: blur(2px);
    }
    80% {
      filter: blur(0.5px);
    }
    100% {
      transform: translateY(var(--dist));
      filter: blur(0);
    }
  }

  .tile {
    position: relative;
    height: 330px; /* TILE_H */
    flex-shrink: 0;
    border-radius: 14px;
    overflow: hidden;
    background: linear-gradient(160deg, var(--bg-2), var(--bg-1));
    box-shadow: 0 16px 40px rgba(0, 0, 0, 0.5);
    display: grid;
    place-items: center;
  }

  .tile-name {
    padding: 16px;
    text-align: center;
    font-weight: 700;
    color: var(--text-dim);
  }

  .tile :global(.tile-img) {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
    object-fit: cover;
  }

  .skip {
    position: absolute;
    bottom: 28px;
    left: 50%;
    transform: translateX(-50%);
    padding: 8px 16px;
    border-radius: 999px;
    color: var(--text-dim);
    background: var(--surface);
    border: 1px solid var(--border);
  }

  .skip:hover {
    color: var(--text);
  }

  /* ---------- Hero reveal ---------- */

  .hero-stage {
    position: absolute;
    inset: 0;
    animation: hero-in 450ms var(--ease);
  }

  .bg {
    position: absolute;
    inset: 0;
  }

  .bg :global(.bg-blur) {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
    object-fit: cover;
    filter: blur(40px) saturate(1.3) brightness(0.7);
    transform: scale(1.2);
  }

  .bg :global(.bg-hero) {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
    object-fit: cover;
    object-position: center 30%;
  }

  /* Darken bottom-left so text stays readable on any art. */
  .shade {
    position: absolute;
    inset: 0;
    background:
      linear-gradient(to top, rgba(14, 17, 23, 0.95) 0%, rgba(14, 17, 23, 0.6) 40%, rgba(14, 17, 23, 0.1) 75%),
      linear-gradient(to right, rgba(14, 17, 23, 0.7) 0%, transparent 60%);
  }

  .content {
    position: absolute;
    left: 0;
    right: 0;
    bottom: 0;
    display: flex;
    align-items: flex-end;
    gap: 32px;
    padding: 0 48px 40px;
  }

  .cover {
    flex: 0 0 180px;
    aspect-ratio: 2 / 3;
    border-radius: 12px;
    overflow: hidden;
    background: rgba(0, 0, 0, 0.35);
    box-shadow: 0 20px 50px rgba(0, 0, 0, 0.6);
    display: grid;
    place-items: center;
    animation: rise 500ms var(--ease) both;
  }

  .cover :global(.cover-img) {
    width: 100%;
    height: 100%;
    object-fit: cover;
  }

  .cover :global(.header-img) {
    width: 100%;
    object-fit: contain;
  }

  .info {
    min-width: 0;
    flex: 1;
    animation: rise 500ms 80ms var(--ease) both;
  }

  .badge {
    display: inline-block;
    font-size: 12px;
    font-weight: 650;
    padding: 4px 11px;
    border-radius: 999px;
    background: rgba(102, 192, 244, 0.2);
    color: var(--accent);
    backdrop-filter: blur(6px);
  }

  .badges {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .fav {
    width: 30px;
    height: 30px;
    border-radius: 50%;
    font-size: 17px;
    line-height: 1;
    color: rgba(230, 237, 243, 0.55);
    background: rgba(255, 255, 255, 0.08);
    border: 1px solid rgba(255, 255, 255, 0.12);
    backdrop-filter: blur(6px);
    transition:
      color var(--t-fast) var(--ease),
      transform var(--t-fast) var(--ease);
  }

  .fav:hover:not(:disabled) {
    color: #f5c451;
    transform: scale(1.08);
  }

  .fav.on {
    color: #f5c451;
    border-color: rgba(245, 196, 81, 0.5);
  }

  .badge.off {
    background: rgba(255, 255, 255, 0.12);
    color: var(--text);
  }

  h2 {
    margin: 12px 0 6px;
    font-size: clamp(32px, 5vw, 56px);
    font-weight: 800;
    letter-spacing: -0.025em;
    line-height: 1.05;
    text-shadow: 0 2px 20px rgba(0, 0, 0, 0.5);
    overflow-wrap: anywhere;
  }

  .meta {
    margin: 0;
    color: rgba(230, 237, 243, 0.75);
  }

  .dot {
    margin: 0 8px;
  }

  .line {
    min-height: 1.5em;
    margin: 16px 0 14px;
    color: var(--accent);
    font-weight: 650;
  }

  .err {
    color: #ff8a8e;
  }

  .actions {
    display: flex;
    gap: 10px;
    flex-wrap: wrap;
    align-items: center;
  }

  .actions button {
    display: inline-flex;
    align-items: center;
    gap: 10px;
    padding: 12px 18px;
    border-radius: 12px;
    background: rgba(255, 255, 255, 0.08);
    border: 1px solid rgba(255, 255, 255, 0.12);
    backdrop-filter: blur(10px);
    font-weight: 650;
    transition:
      background var(--t-fast) var(--ease),
      transform var(--t-fast) var(--ease);
  }

  .actions button:hover:not(:disabled) {
    background: rgba(255, 255, 255, 0.14);
    transform: translateY(-1px);
  }

  .actions .primary {
    padding: 8px 20px 8px 8px;
    font-size: 17px;
    background: linear-gradient(135deg, #7fd0ff 0%, var(--accent-strong) 100%);
    color: #0e1a26;
    border-color: transparent;
    box-shadow: 0 10px 30px rgba(102, 192, 244, 0.35);
  }

  .actions .primary:hover:not(:disabled) {
    background: linear-gradient(135deg, #95d9ff 0%, #4aa7e3 100%);
  }

  .ring {
    width: 38px;
    height: 38px;
    transform: rotate(-90deg);
  }

  .ring-track {
    fill: rgba(14, 26, 38, 0.15);
    stroke: rgba(14, 26, 38, 0.2);
    stroke-width: 4;
  }

  .ring-fill {
    fill: none;
    stroke: #0e1a26;
    stroke-width: 4;
    stroke-linecap: round;
    transition: stroke-dashoffset 60ms linear;
  }

  .ring text {
    transform: rotate(90deg);
    transform-origin: 25px 25px;
    font-size: 20px;
    font-weight: 800;
    fill: #0e1a26;
  }

  button:disabled {
    opacity: 0.55;
    cursor: default;
  }

  .ban {
    margin-top: 14px;
    font-size: 13px;
    color: rgba(230, 237, 243, 0.6);
    text-decoration: underline;
    text-underline-offset: 3px;
  }

  .ban:hover:not(:disabled) {
    color: #ff8a8e;
  }

  @media (max-width: 980px) {
    .content {
      padding: 0 32px 32px;
      gap: 24px;
    }

    .cover {
      flex-basis: 140px;
    }
  }

  @keyframes fade {
    from {
      opacity: 0;
    }
  }

  @keyframes hero-in {
    from {
      opacity: 0;
      transform: scale(1.03);
    }
  }

  @keyframes rise {
    from {
      opacity: 0;
      transform: translateY(16px);
    }
  }
</style>
