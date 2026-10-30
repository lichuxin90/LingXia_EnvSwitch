<script lang="ts">
  import { Check, FolderOpen, HardDrive } from "@lucide/svelte";
  import Badge from "$lib/components/Badge.svelte";
  import Button from "$lib/components/Button.svelte";
  import { api } from "$lib/ipc";
  import { t, type MsgKey } from "$lib/i18n/store.svelte";
  import { app } from "$lib/state.svelte";
  import type { DataRootCandidate } from "$lib/types";

  const candidates = $derived(app.boot?.candidates ?? []);
  let picked = $state("");

  const ROOT_KEY: Record<string, MsgKey> = {
    exe: "firstRun.rootExe",
    programData: "firstRun.rootProgramData",
    home: "firstRun.rootHome",
  };
  /** 后端只给 id 和中文原名：认得出的翻成界面语言，认不出的照原文显示。 */
  const rootLabel = (c: DataRootCandidate): string => {
    if (c.id === "drive") return t("firstRun.rootDrive", { letter: c.dir.charAt(0) });
    const key = ROOT_KEY[c.id];
    return key ? t(key) : c.label;
  };
  const rootNote = (c: DataRootCandidate): string =>
    c.id === "programData" ? t("firstRun.rootProgramDataNote") : (c.note ?? "");

  $effect(() => {
    if (!picked && candidates.length) picked = candidates[0].dir;
  });

  async function browse(): Promise<void> {
    const dir = await api.pickFolder();
    if (dir) picked = dir;
  }

  async function confirm(): Promise<void> {
    if (!picked) return;
    await app.chooseDataRoot(picked);
  }
</script>

<div class="flex min-h-dvh items-center justify-center bg-background px-6">
  <div class="w-full max-w-2xl">
    <div class="flex items-center gap-3">
      <div class="flex size-10 items-center justify-center rounded-xl bg-primary/12 text-primary">
        <HardDrive class="size-5" />
      </div>
      <div>
        <h1 class="text-lg font-semibold">{t("firstRun.title")}</h1>
        <p class="text-xs text-muted-foreground">{t("firstRun.subtitle")}</p>
      </div>
    </div>

    <p class="mt-4 text-sm leading-relaxed text-muted-foreground">{t("firstRun.body")}</p>

    <ul class="mt-4 space-y-2">
      {#each candidates as c (c.dir)}
        <li>
          <button
            type="button"
            class="flex w-full items-center gap-3 rounded-xl border px-3 py-2.5 text-left transition-colors
              {picked === c.dir ? 'border-primary bg-accent/50' : 'border-border bg-card hover:border-primary/40'}"
            onclick={() => (picked = c.dir)}
          >
            <Check class="size-4 shrink-0 {picked === c.dir ? 'text-primary' : 'text-transparent'}" />
            <div class="min-w-0 flex-1">
              <p class="text-sm font-medium">{rootLabel(c)}</p>
              <p class="mono mt-0.5 truncate text-xs text-muted-foreground" title={c.dir}>{c.dir}</p>
              {#if rootNote(c)}
                <p class="mt-0.5 text-xs text-muted-foreground">{rootNote(c)}</p>
              {/if}
            </div>
            {#if c.rank === 0}
              <Badge tone="ok">{t("firstRun.recommended")}</Badge>
            {:else if /C:/.test(c.dir)}
              <Badge tone="warn">{t("firstRun.cDrive")}</Badge>
            {/if}
          </button>
        </li>
      {/each}
    </ul>

    <div class="mt-4 flex items-center gap-2">
      <Button variant="outline" size="sm" onclick={browse}>
        <FolderOpen class="size-3.5" />
        {t("firstRun.browse")}
      </Button>
      <span class="mono min-w-0 flex-1 truncate text-xs text-muted-foreground" title={picked}>{picked || t("firstRun.nonePicked")}</span>
      <Button size="sm" disabled={!picked} onclick={() => void confirm()}>{t("firstRun.confirm")}</Button>
    </div>
  </div>
</div>
