<script lang="ts">
  // Home-screen backdrop: rows of the user's cover art drifting slowly in opposite
  // directions, blurred and dimmed behind the logo, fading out above the filters.
  import { artSrc, type BackdropItem } from "./api";
  import ArtImage from "./ArtImage.svelte";
  import { app, loadBackdrop } from "./state.svelte";

  interface Props {
    /** Freeze the drift (e.g. while the reveal covers the screen). */
    paused: boolean;
  }

  let { paused }: Props = $props();

  // Two rows fill the area behind the logo and buttons; a third would sit in the fade-out.
  const COUNT = 28;
  const ROWS = 2;
  /** Seconds per full loop, per row; slow on purpose. */
  const LOOP_SECONDS = [95, 120];
  /** Minimum tiles per row before doubling, so short libraries still fill the width. */
  const MIN_TILES = 10;
  const FADE_IN_MAX_MS = 1500;

  let ready = $state(false);

  // Wait for the library scan, then fetch covers once per session.
  $effect(() => {
    if (app.view && app.backdrop === null) loadBackdrop(COUNT);
  });

  /** Each row's items, repeated to MIN_TILES, then doubled for a seamless loop. */
  const rows = $derived.by(() => {
    const items = app.backdrop ?? [];
    if (items.length === 0) return [];
    return Array.from({ length: ROWS }, (_, r) => {
      let row = items.filter((_, i) => i % ROWS === r);
      if (row.length === 0) row = items;
      const base: BackdropItem[] = [];
      while (base.length < MIN_TILES) base.push(...row);
      return [...base, ...base];
    });
  });

  // Fade in once the first covers have loaded (or after a short cap), never half-empty.
  $effect(() => {
    const items = app.backdrop;
    if (!items || items.length === 0 || ready) return;
    const first = items.slice(0, 12).flatMap((b) => (b.portrait.length ? [artSrc(b.portrait[0])] : []));
    let done = false;
    const show = () => {
      if (!done) ready = true;
      done = true;
    };
    const timer = setTimeout(show, FADE_IN_MAX_MS);
    Promise.all(
      first.map(
        (s) =>
          new Promise<void>((resolve) => {
            const img = new Image();
            img.onload = img.onerror = () => resolve();
            img.src = s;
          }),
      ),
    ).then(show);
    return () => clearTimeout(timer);
  });
</script>

<div class="backdrop" class:ready class:paused aria-hidden="true">
  <div class="wall">
    {#each rows as row, r}
      <div class="row">
        <div class="track" class:reverse={r % 2 === 1} style="--loop: {LOOP_SECONDS[r % LOOP_SECONDS.length]}s">
          {#each row as item, i (i)}
            <div class="tile">
              <ArtImage sources={item.portrait} alt="" class="cover" />
            </div>
          {/each}
        </div>
      </div>
    {/each}
  </div>
  <div class="scrim"></div>
</div>

<style>
  .backdrop {
    position: absolute;
    inset: 0;
    z-index: 0;
    overflow: hidden;
    pointer-events: none;
  }

  .wall {
    position: absolute;
    top: -24px;
    left: 0;
    right: 0;
    display: flex;
    flex-direction: column;
    gap: 16px;
    opacity: 0;
    transition: opacity 1.2s var(--ease);
    /* Fade out before the filter chips so their small text stays clean. */
    mask-image: linear-gradient(to bottom, black 0%, black 42%, transparent 70%);
  }

  .ready .wall {
    opacity: 0.35;
  }

  .row {
    overflow: hidden;
  }

  .track {
    display: flex;
    gap: 16px;
    /* Trailing gap so -50% lands exactly on the repeated half. */
    padding-right: 16px;
    width: max-content;
    animation: drift var(--loop) linear infinite;
    /* Own layer: the blurred tiles rasterize once and only the layer moves. */
    will-change: transform;
  }

  .track.reverse {
    animation-direction: reverse;
  }

  .paused .track {
    animation-play-state: paused;
  }

  .tile {
    flex: 0 0 auto;
    width: 140px;
    aspect-ratio: 2 / 3;
    border-radius: 10px;
    overflow: hidden;
    background: rgba(255, 255, 255, 0.03);
  }

  .tile :global(.cover) {
    width: 100%;
    height: 100%;
    object-fit: cover;
    filter: blur(5px) saturate(1.15);
    transform: scale(1.06); /* hide the blur's soft edge */
  }

  /* Darker pool behind the logo and buttons for readability. */
  .scrim {
    position: absolute;
    inset: 0;
    background: radial-gradient(
      ellipse 60% 55% at 50% 42%,
      rgba(23, 26, 33, 0.85) 0%,
      rgba(23, 26, 33, 0.55) 45%,
      transparent 80%
    );
  }

  @keyframes drift {
    from {
      transform: translateX(0);
    }
    to {
      transform: translateX(-50%);
    }
  }

  @media (prefers-reduced-motion: reduce) {
    .track {
      animation: none;
    }
  }
</style>
