<script lang="ts">
  // Every game in one list: add your own, edit any game, hide or un-hide.
  import { api, isCustom, type LibraryGame } from "../lib/api";
  import { branding } from "../lib/branding";
  import GameEditor, { type EditTarget } from "../lib/GameEditor.svelte";
  import { app, loadLibrary } from "../lib/state.svelte";

  interface Props {
    onBack: () => void;
  }

  let { onBack }: Props = $props();

  type Filter = "all" | "mine" | "edited" | "hidden";
  let filter = $state<Filter>("all");
  let query = $state("");
  let editing = $state<EditTarget | null>(null);
  let error = $state("");

  const blacklist = $derived(new Set(app.settings?.blacklist ?? []));

  const counts = $derived.by(() => {
    const games = app.view?.games ?? [];
    return {
      all: games.length,
      mine: games.filter((g) => isCustom(g.appId)).length,
      edited: games.filter((g) => g.edited).length,
      hidden: games.filter((g) => blacklist.has(g.appId)).length,
    };
  });

  const games = $derived.by(() => {
    const q = query.trim().toLowerCase();
    return (app.view?.games ?? []).filter((g) => {
      if (q && !g.name.toLowerCase().includes(q)) return false;
      if (filter === "mine") return isCustom(g.appId);
      if (filter === "edited") return g.edited;
      if (filter === "hidden") return blacklist.has(g.appId);
      return true;
    });
  });

  function edit(g: LibraryGame) {
    if (isCustom(g.appId)) {
      const game = app.userGames?.custom.find((c) => c.id === g.appId);
      if (game) editing = { kind: "custom", game, favorite: g.favorite };
    } else {
      editing = { kind: "steam", game: g, override: app.userGames?.overrides[String(g.appId)] ?? null };
    }
  }

  async function setHidden(g: LibraryGame, hidden: boolean) {
    error = "";
    try {
      app.settings = await api.setBlacklisted(g.appId, hidden);
    } catch (e) {
      error = (e as Error).message;
    }
  }

  async function closeEditor() {
    editing = null;
    await loadLibrary(false);
  }

  const FILTERS: { id: Filter; label: string }[] = [
    { id: "all", label: "All" },
    { id: "mine", label: "Added by you" },
    { id: "edited", label: "Edited" },
    { id: "hidden", label: "Hidden" },
  ];
</script>

<section class="games">
  <header>
    <button class="back" onclick={onBack}>← {branding.copy.back}</button>
    <h2>Games</h2>
    <button class="btn primary" onclick={() => (editing = { kind: "new" })}>+ Add a game</button>
  </header>
  <p class="dim intro">
    Add games from any launcher (Battle.net, Epic, GOG…), fix missing art, set moods and favorites, or hide games
    you never want picked.
  </p>

  <div class="toolbar">
    <input class="search" placeholder="Search your games…" bind:value={query} spellcheck="false" />
    <div class="filters" role="radiogroup">
      {#each FILTERS as f}
        <button role="radio" aria-checked={filter === f.id} onclick={() => (filter = f.id)}>
          {f.label} <span class="count">{counts[f.id]}</span>
        </button>
      {/each}
    </div>
  </div>

  {#if error}<p class="error">{error}</p>{/if}

  {#if !app.view}
    <p class="dim">{branding.copy.loading}</p>
  {:else if games.length === 0}
    <p class="dim empty">
      {filter === "mine" && !query ? "You haven't added any games yet." : "No games match."}
    </p>
  {:else}
    <ul class="list">
      {#each games as g (g.appId)}
        {@const hidden = blacklist.has(g.appId)}
        <li class:hidden>
          <span class="name">
            {#if g.favorite}<span class="star" title="Favorite">★</span>{/if}
            {g.name}
          </span>
          <span class="badges">
            {#if isCustom(g.appId)}<span class="badge mine">Added by you</span>{/if}
            {#if g.edited}<span class="badge">Edited</span>{/if}
            {#if hidden}<span class="badge warn">Hidden</span>{/if}
          </span>
          <span class="actions">
            <button class="btn small" onclick={() => edit(g)}>Edit</button>
            <button class="btn small ghost" onclick={() => setHidden(g, !hidden)}>{hidden ? "Unhide" : "Hide"}</button>
          </span>
        </li>
      {/each}
    </ul>
  {/if}
</section>

{#if editing}
  {#key editing}
    <GameEditor target={editing} onClose={closeEditor} />
  {/key}
{/if}

<style>
  .games {
    height: 100%;
    overflow-y: auto;
    padding: 32px 48px 48px;
    max-width: 920px;
    margin: 0 auto;
  }

  header {
    display: flex;
    align-items: center;
    gap: 16px;
  }

  h2 {
    flex: 1;
    margin: 0;
    font-size: 28px;
    font-weight: 750;
  }

  .intro {
    margin: 10px 0 18px;
    font-size: 14px;
  }

  .back {
    padding: 8px 12px;
    border-radius: 8px;
    color: var(--text-dim);
  }

  .back:hover {
    background: var(--surface-hover);
    color: var(--text);
  }

  .toolbar {
    display: flex;
    flex-wrap: wrap;
    gap: 10px;
    align-items: center;
    margin-bottom: 14px;
    position: sticky;
    top: -32px;
    padding: 8px 0;
    background: var(--bg-1);
    z-index: 1;
  }

  .search {
    flex: 1;
    min-width: 200px;
    padding: 9px 12px;
    border-radius: 8px;
    border: 1px solid var(--border);
    background: rgba(0, 0, 0, 0.25);
    color: var(--text);
    font: inherit;
    font-size: 14px;
  }

  .filters {
    display: inline-flex;
    gap: 6px;
  }

  .filters button {
    padding: 7px 11px;
    border-radius: 999px;
    font-size: 13px;
    background: var(--surface);
    border: 1px solid var(--border);
    color: var(--text-dim);
    white-space: nowrap;
  }

  .filters button[aria-checked="true"] {
    color: var(--accent);
    border-color: var(--accent);
    background: rgba(102, 192, 244, 0.12);
  }

  .count {
    font-size: 11px;
    opacity: 0.7;
    font-variant-numeric: tabular-nums;
  }

  .list {
    list-style: none;
    margin: 0;
    padding: 0;
    display: grid;
    gap: 4px;
  }

  li {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 8px 10px 8px 14px;
    border-radius: 10px;
    background: var(--surface);
    border: 1px solid transparent;
  }

  li:hover {
    border-color: var(--border);
  }

  li.hidden .name {
    opacity: 0.55;
  }

  .name {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: 14px;
  }

  .star {
    color: #f5c451;
    margin-right: 4px;
  }

  .badges {
    display: inline-flex;
    gap: 6px;
  }

  .badge {
    font-size: 11px;
    font-weight: 600;
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

  .badge.warn {
    color: #ffb86b;
    background: rgba(245, 160, 60, 0.12);
  }

  .actions {
    display: inline-flex;
    gap: 6px;
  }

  .btn {
    padding: 8px 13px;
    border-radius: 8px;
    background: var(--surface-hover);
    border: 1px solid var(--border);
    font-size: 14px;
    white-space: nowrap;
  }

  .btn.small {
    padding: 5px 10px;
    font-size: 13px;
  }

  .btn.ghost {
    background: none;
  }

  .btn.primary {
    background: linear-gradient(135deg, #7fd0ff 0%, var(--accent-strong) 100%);
    color: #0e1a26;
    border-color: transparent;
    font-weight: 650;
  }

  .dim {
    color: var(--text-dim);
  }

  .empty {
    padding: 24px 0;
    text-align: center;
  }

  .error {
    color: #ff8a8e;
    font-size: 13px;
  }
</style>
