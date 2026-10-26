<script lang="ts">
  // An <img> that walks a list of candidate sources until one loads.
  import { artSrc, type ArtSource } from "./api";

  interface Props {
    sources: ArtSource[];
    alt: string;
    class?: string;
    /** Called once every candidate has failed. */
    onfail?: () => void;
  }

  let { sources, alt, class: cls = "", onfail }: Props = $props();

  let index = $state(0);
  let loaded = $state(false);

  // New candidate list (new game) -> start over.
  $effect(() => {
    sources;
    index = 0;
    loaded = false;
  });

  function next() {
    loaded = false;
    if (index < sources.length - 1) {
      index += 1;
    } else {
      index = sources.length;
      onfail?.();
    }
  }
</script>

{#if index < sources.length}
  <img
    class={cls}
    class:loaded
    src={artSrc(sources[index])}
    {alt}
    draggable="false"
    onload={() => (loaded = true)}
    onerror={next}
  />
{/if}

<style>
  img {
    opacity: 0;
    transition: opacity var(--t-med) var(--ease);
  }

  img.loaded {
    opacity: 1;
  }
</style>
