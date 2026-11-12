<script lang="ts">
  import { onMount } from "svelte";
  import { Download, RefreshCw, Star } from "@lucide/svelte";
  import Button from "$lib/components/Button.svelte";
  import GitHubMark from "$lib/components/GitHubMark.svelte";
  import Modal from "$lib/components/Modal.svelte";
  import { api, messageOf } from "$lib/ipc";
  import { t, type MsgKey } from "$lib/i18n/store.svelte";
  import { app } from "$lib/state.svelte";
  import { REPO_URL } from "$lib/utils";
  import type { UpdateInfo } from "$lib/types";

  let info = $state<UpdateInfo | null>(null);
  let stars = $state<number | null>(null);
  let busy = $state(false);
  let failed = $state("");
  let showNotes = $state(false);
  let copy = $state(Math.floor(Math.random() * 3));
  const messages: [MsgKey, MsgKey][] = [
    ["repo.starTitle", "repo.starHint"],
    ["repo.starTitle2", "repo.starHint2"],
    ["repo.starTitle3", "repo.starHint3"],
  ];

  async function check(manual = true): Promise<void> {
    if (busy) return;
    busy = true;
    failed = "";
    try { info = await api.checkUpdate(); }
    catch (e) { if (manual) failed = messageOf(e); }
    finally { busy = false; }
  }

  onMount(() => {
    let disposed = false;
    void check(false);
    void api.repoStars().then((count) => { if (!disposed) stars = count; }).catch(() => {});
    const timer = window.setInterval(() => {
      if (!document.hidden && !window.matchMedia("(prefers-reduced-motion: reduce)").matches) {
        copy = (copy + 1 + Math.floor(Math.random() * 2)) % messages.length;
      }
    }, 9000);
    return () => { disposed = true; window.clearInterval(timer); };
  });

  const release = $derived(info?.hasUpdate ? info : null);
  const status = $derived(busy ? t("repo.checking") : failed ? t("repo.failed", { reason: failed }) : info
    ? info.hasUpdate ? t("repo.found", { latest: info.latest, current: info.current }) : t("repo.latest", { current: info.current }) : "");
</script>

<div class="repo-card">
  <div class="min-w-0 flex-1">
    {#key copy}
      <div class="star-copy">
        <p class="star-title">{t(messages[copy][0])}</p>
        <p class="mt-1 text-xs leading-relaxed text-muted-foreground">{t(messages[copy][1])}</p>
      </div>
    {/key}
    {#if status}
      <p class="mt-1 max-w-xl break-words text-xs {failed ? 'text-destructive' : release ? 'text-warning' : 'text-muted-foreground'}">{status}</p>
    {/if}
  </div>
  <div class="flex flex-wrap items-center gap-2">
    <Button size="sm" variant="outline" class="star-button" onclick={() => void app.openLink(REPO_URL)}>
      <GitHubMark class="size-3.5" />
      {t("repo.starCta")}
      <span class="star-spark inline-flex"><Star class="size-3.5 text-muted-foreground" /></span>
      {#if stars !== null}<span class="mono border-l border-border pl-2 tabular-nums">{stars.toLocaleString()}</span>{/if}
    </Button>
    <Button size="sm" variant="outline" disabled={busy} onclick={() => void check()}>
      <RefreshCw class="size-3.5 {busy ? 'animate-spin' : ''}" />{t("repo.checkUpdate")}
    </Button>
    {#if release}
      <button type="button" class="text-xs text-primary underline-offset-4 hover:underline" onclick={() => showNotes = true}>{t("repo.changelog")} · {release.latest}</button>
      <Button size="sm" onclick={() => void app.openLink(release.url)}><Download class="size-3.5" />{t("repo.updateTo", { latest: release.latest })}</Button>
    {/if}
  </div>
</div>

{#if showNotes && release}
  <Modal title={`${t("repo.changelog")} · ${release.latest}`} onclose={() => showNotes = false}>
    <p class="whitespace-pre-wrap break-words text-sm">{release.notes || release.title}</p>
    {#snippet footer()}<Button size="sm" onclick={() => void app.openLink(release.url)}>{t("repo.updateTo", { latest: release.latest })}</Button>{/snippet}
  </Modal>
{/if}

<style>
  .repo-card { display: flex; flex-direction: column; gap: 18px; position: relative; z-index: 1; }
  .star-copy { min-height: 65px; }
  .star-title { font-size: clamp(18px,2.1cqw,25px); font-weight: 650; line-height: 1.5; letter-spacing: -.6px; }
  .repo-card :global(.star-button) { position: relative; overflow: hidden; background: var(--foreground); border-color: var(--foreground); color: var(--background); padding: 8px 14px; min-height: 36px; }
  .repo-card :global(.star-button)::after { content: ''; position: absolute; inset: -50% auto -50% -80%; width: 35%; background: linear-gradient(90deg,transparent,#ffffff35,transparent); transform: skewX(-20deg); animation: star-sheen 8s ease-in-out infinite; pointer-events: none; }
  .star-spark :global(svg) { color: #ffd780; fill: #ffd78040; }
  @keyframes star-sheen { 0%,65% { left: -80%; } 85%,100% { left: 150%; } }
  @media(prefers-reduced-motion: reduce) { .repo-card :global(.star-button)::after { animation: none; } }


  .star-copy { animation: copy-enter .55s ease-out; }
  .star-spark { animation: star-hop 12s ease-in-out infinite; transform-origin: center; }
  @keyframes copy-enter { from { opacity: 0; transform: translateY(5px); } to { opacity: 1; transform: translateY(0); } }
  @keyframes star-hop { 0%, 80%, 100% { transform: translateY(0) rotate(0); } 87% { transform: translateY(-3px) rotate(-12deg); } 94% { transform: translateY(0) rotate(10deg); } }
  @media (prefers-reduced-motion: reduce) { .star-copy, .star-spark { animation: none; } }
</style>
