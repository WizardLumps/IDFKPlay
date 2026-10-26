<script lang="ts">
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import { branding } from "./branding";

  interface Props {
    onSettings: () => void;
    settingsActive: boolean;
  }

  let { onSettings, settingsActive }: Props = $props();

  const win = getCurrentWindow();
</script>

<header class="titlebar" data-tauri-drag-region>
  <div class="brand" data-tauri-drag-region>
    <span class="dot" data-tauri-drag-region></span>
    <span data-tauri-drag-region>{branding.appName}</span>
  </div>

  <div class="controls">
    <button
      class="ctl"
      class:active={settingsActive}
      title={branding.copy.settings}
      aria-label={branding.copy.settings}
      onclick={onSettings}
    >
      <svg viewBox="0 0 16 16" width="15" height="15" aria-hidden="true" fill="none" stroke="currentColor" stroke-width="1.4" stroke-linecap="round">
        <path d="M2 4h12M2 8h12M2 12h12" opacity="0.55" />
        <circle cx="5" cy="4" r="1.8" fill="currentColor" />
        <circle cx="11" cy="8" r="1.8" fill="currentColor" />
        <circle cx="7" cy="12" r="1.8" fill="currentColor" />
      </svg>
    </button>
    <button class="ctl" aria-label="Minimize" onclick={() => win.minimize()}>
      <svg viewBox="0 0 12 12" width="12" height="12" aria-hidden="true">
        <rect x="1" y="5.5" width="10" height="1" fill="currentColor" />
      </svg>
    </button>
    <button class="ctl" aria-label="Maximize" onclick={() => win.toggleMaximize()}>
      <svg viewBox="0 0 12 12" width="12" height="12" aria-hidden="true">
        <rect x="1.5" y="1.5" width="9" height="9" fill="none" stroke="currentColor" />
      </svg>
    </button>
    <button class="ctl close" aria-label="Close" onclick={() => win.close()}>
      <svg viewBox="0 0 12 12" width="12" height="12" aria-hidden="true">
        <path d="M1.5 1.5l9 9M10.5 1.5l-9 9" stroke="currentColor" stroke-width="1.2" />
      </svg>
    </button>
  </div>
</header>

<style>
  .titlebar {
    height: var(--titlebar-h);
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding-left: 14px;
    background: rgba(23, 26, 33, 0.6);
    backdrop-filter: blur(12px);
    border-bottom: 1px solid var(--border);
    flex-shrink: 0;
    position: relative;
    z-index: 10;
  }

  .brand {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 13px;
    font-weight: 600;
    letter-spacing: 0.02em;
    color: var(--text-dim);
  }

  .dot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: var(--accent);
    box-shadow: 0 0 10px var(--accent);
  }

  .controls {
    display: flex;
    height: 100%;
  }

  .ctl {
    width: 46px;
    height: 100%;
    display: grid;
    place-items: center;
    color: var(--text-dim);
    transition:
      background var(--t-fast) var(--ease),
      color var(--t-fast) var(--ease);
  }

  /* Title-bar buttons are flush to the edge: an inset ring instead of an outline. */
  .ctl:focus-visible {
    outline: none;
    box-shadow: inset 0 0 0 2px rgba(102, 192, 244, 0.5);
  }

  .ctl:hover,
  .ctl.active {
    background: var(--surface-hover);
    color: var(--text);
  }

  .ctl.active {
    color: var(--accent);
  }

  .ctl.close:hover {
    background: var(--danger);
    color: white;
  }
</style>
