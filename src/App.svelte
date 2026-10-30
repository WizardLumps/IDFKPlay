<script lang="ts">
  import TitleBar from "./lib/TitleBar.svelte";
  import MainScreen from "./screens/MainScreen.svelte";
  import SettingsScreen from "./screens/SettingsScreen.svelte";
  import DebugLibrary from "./screens/DebugLibrary.svelte";
  import GamesScreen from "./screens/GamesScreen.svelte";
  import { init } from "./lib/state.svelte";

  type Screen = "main" | "settings" | "debug" | "games";
  let screen = $state<Screen>("main");

  // Scan at startup so tag learning starts in the background right away.
  init();

  function toggleSettings() {
    screen = screen === "main" ? "settings" : "main";
  }
</script>

<div class="shell">
  <TitleBar onSettings={toggleSettings} settingsActive={screen !== "main"} />
  <main>
    {#key screen}
      <div class="screen">
        {#if screen === "main"}
          <MainScreen onSettings={() => (screen = "settings")} />
        {:else if screen === "debug"}
          <DebugLibrary onBack={() => (screen = "settings")} />
        {:else if screen === "games"}
          <GamesScreen onBack={() => (screen = "settings")} />
        {:else}
          <SettingsScreen
            onBack={() => (screen = "main")}
            onDebug={() => (screen = "debug")}
            onGames={() => (screen = "games")}
          />
        {/if}
      </div>
    {/key}
  </main>
</div>

<style>
  .shell {
    height: 100%;
    display: flex;
    flex-direction: column;
  }

  main {
    flex: 1;
    min-height: 0;
    position: relative;
  }

  .screen {
    position: absolute;
    inset: 0;
    animation: fade-in var(--t-med) var(--ease);
  }

  @keyframes fade-in {
    from {
      opacity: 0;
      transform: translateY(6px);
    }
  }
</style>
