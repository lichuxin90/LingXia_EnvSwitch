<script lang="ts">
  import { Download, FolderInput, RefreshCw, ScanSearch } from "@lucide/svelte";
  import Button from "$lib/components/Button.svelte";
  import Modal from "$lib/components/Modal.svelte";
  import { app } from "$lib/state.svelte";
  import { api } from "$lib/ipc";
  import { t } from "$lib/i18n/store.svelte";
  import type { ForeignRuntime } from "$lib/types";

  let pending = $state<string | null>(null);
  let reinstall = $state<ForeignRuntime | null>(null);
  async function adopt(f: ForeignRuntime): Promise<void> {
    if (!f.tool || pending) return;
    if (!await app.askDanger(t("external.adoptTitle"), t("external.adoptBody", { path: f.root }), t("env.takeover"), "default")) return;
    pending = f.exe;
    try { await app.adopt(f.exe, f.tool, f.version, false); }
    finally { pending = null; }
  }
  async function refresh(): Promise<void> {
    if (await app.refreshEnv()) app.toast(t("external.refreshed", { n: app.scan?.foreign.length ?? 0 }), "ok");
  }
  async function uninstall(): Promise<void> {
    try { await api.openSystemUninstall(); } catch (e) { app.fail(e); }
  }
</script>

{#if app.scan?.foreign.length}
  <section class="external-card" aria-label={t("external.title")}>
    <div class="external-heading">
      <span class="external-icon"><ScanSearch size={19} /></span>
      <div class="external-heading-text"><h2>{t("external.title")} <span>{app.scan.foreign.length}</span></h2><p>{t("external.hint")}</p></div>
      <Button size="sm" variant="outline" disabled={app.scanning || pending !== null} onclick={() => void refresh()}><RefreshCw size={14} class={app.scanning ? "animate-spin" : ""} />{app.scanning ? t("external.refreshing") : t("external.refresh")}</Button>
    </div>
    <ul>
      {#each app.scan.foreign as f (f.exe)}
        <li>
          <div class="external-info"><strong>{f.name} <span class="mono">{f.version ?? "—"}</span></strong><p class="mono" title={f.exe}>{f.exe}</p>{#if !f.takeable}<p>{f.whyNot ?? t("env.notTakeable")}</p>{/if}</div>
          <div class="external-actions">
            {#if f.takeable && f.tool}<Button size="sm" disabled={pending !== null} onclick={() => void adopt(f)}><FolderInput size={14} />{pending === f.exe ? t("external.adopting") : t("env.takeover")}</Button>{/if}
            <Button size="sm" variant="outline" onclick={() => reinstall = f}><Download size={14} />{t("external.reinstall")}</Button>
          </div>
        </li>
      {/each}
    </ul>
  </section>
{:else if app.scanning && !app.scan}
  <p class="external-scanning"><ScanSearch size={15} />{t("external.scanning")}</p>
{/if}

{#if reinstall}
  <Modal title={t("external.reinstallTitle")} onclose={() => reinstall = null}>
    <p class="text-sm leading-relaxed">{t("external.reinstallBody", { name: reinstall.name })}</p>
    <p class="mono mt-3 break-all rounded-lg bg-muted p-3 text-xs">{reinstall.exe}</p>
    {#snippet footer()}
      <Button variant="outline" onclick={() => void uninstall()}>{t("external.systemApps")}</Button>
      {#if reinstall?.tool}<Button onclick={() => { const tool = reinstall?.tool; reinstall = null; if (tool) void app.setTool(tool); }}>{t("external.download")}</Button>{/if}
    {/snippet}
  </Modal>
{/if}

<style>
  .external-card { border: 1px solid color-mix(in srgb, var(--primary) 26%, var(--border)); border-radius: 18px; background: var(--card); padding: 20px 24px; }
  .external-heading { display: flex; flex-wrap: wrap; gap: 12px; align-items: center; }
  .external-heading-text { flex: 1; min-width: 180px; }
  .external-icon { color: var(--primary); padding: 10px; border-radius: 12px; background: var(--accent); }
  h2 { font-size: 14px; font-weight: 600; } h2 > span { font-size: 11px; margin-left: 5px; padding: 2px 6px; border-radius: 5px; background: var(--accent); color: var(--primary); }
  p { font-size: 11px; color: var(--muted-foreground); margin-top: 5px; line-height: 1.6; }
  ul { margin-top: 15px; } li { display: flex; flex-wrap: wrap; align-items: center; gap: 12px; border-top: 1px solid var(--border); padding: 13px 0; } li:last-child { padding-bottom: 0; }
  .external-info { flex: 1; min-width: 180px; } .external-info strong { font-size: 13px; font-weight: 600; } .external-info strong span { margin-left: 8px; font-size: 11px; font-weight: 400; color: var(--muted-foreground); }
  .external-info p { overflow-wrap: anywhere; } .external-actions { display: flex; flex-wrap: wrap; gap: 8px; }
  .external-scanning { display: flex; gap: 8px; align-items: center; padding: 12px 3px; }
</style>
