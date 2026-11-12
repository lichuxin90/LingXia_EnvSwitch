<script lang="ts">
  import { Minus, Square, X } from "@lucide/svelte";
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import { t } from "$lib/i18n/store.svelte";
  import { app } from "$lib/state.svelte";

  async function action(kind: "minimize" | "toggleMaximize" | "close"): Promise<void> {
    try { await getCurrentWindow()[kind](); } catch (e) { app.fail(e); }
  }
  async function drag(e: MouseEvent): Promise<void> {
    if (e.button !== 0 || e.detail > 1) return;
    try { await getCurrentWindow().startDragging(); } catch (e) { app.fail(e); }
  }
</script>

<div class="window-bar">
  <button type="button" class="window-drag" tabindex="-1" aria-label={t("window.drag")} onmousedown={drag} ondblclick={() => void action("toggleMaximize")}>
    <span class="window-title">灵匣 <span>/</span> EnvSwitch</span>
  </button>
  <button type="button" class="window-control" aria-label={t("window.minimize")} title={t("window.minimize")} onclick={() => void action("minimize")}><Minus size={14} /></button>
  <button type="button" class="window-control" aria-label={t("window.maximize")} title={t("window.maximize")} onclick={() => void action("toggleMaximize")}><Square size={11} /></button>
  <button type="button" class="window-control window-close" aria-label={t("window.close")} title={t("window.close")} onclick={() => void action("close")}><X size={16} /></button>
</div>

<style>
  .window-bar { position: fixed; inset: 0 0 auto; height: 36px; z-index: 100; display: flex; align-items: stretch; background: var(--background); color: var(--muted-foreground); user-select: none; }
  .window-drag { flex: 1; min-width: 0; text-align: left; padding-left: 22px; cursor: default; }
  .window-title { font-size: 10px; letter-spacing: .5px; opacity: .8; }
  .window-title span { margin: 0 7px; opacity: .4; }
  .window-control { width: 46px; display: grid; place-items: center; transition: background .15s, color .15s; }
  .window-control:hover { background: var(--muted); color: var(--foreground); }
  .window-close:hover { background: #df4454; color: white; }
</style>
