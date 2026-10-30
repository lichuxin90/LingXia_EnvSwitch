<script lang="ts">
  import { ExternalLink, FolderOpen, X } from "@lucide/svelte";
  import Badge from "$lib/components/Badge.svelte";
  import Button from "$lib/components/Button.svelte";
  import { api } from "$lib/ipc";
  import { t } from "$lib/i18n/store.svelte";
  import { app } from "$lib/state.svelte";
  import { humanBytes, resolutionLabel, scopeLabel, shortTime, sourceLabel, TOOL_LABEL } from "$lib/utils";

  const sel = $derived(app.selected?.tool === app.tool ? app.selected.version : null);
  const rec = $derived((app.installed?.versions ?? []).find((v) => v.version === sel) ?? null);
  const avail = $derived((app.page?.flat ?? []).find((v) => v.version === sel) ?? null);
  const line = $derived((app.page?.lines ?? []).find((l) => l.members.some((m) => m.version === sel)) ?? null);
  const res = $derived(
    app.dirContext?.resolutions.find((r) => r.tool === app.tool) ?? app.installed?.resolution ?? null,
  );
  const isDefault = $derived(app.page?.defaultVersion === sel);
  const isActive = $derived(!!sel && res?.version === sel);

  const pathNote = $derived.by(() => {
    const hit = app.paths.filter((p) => p.installed);
    if (!hit.length) return t("detail.shimMissing");
    const best = hit.reduce((a, b) => ((a.position ?? 999) <= (b.position ?? 999) ? a : b));
    return t("detail.pathNote", {
      scope: scopeLabel(best.scope),
      where: t("detail.position", { n: (best.position ?? 0) + 1 }),
    });
  });
</script>

<aside class="flex h-full flex-col overflow-y-auto border-l border-border bg-detail">
  <div class="flex items-start gap-2 border-b border-border px-4 py-3">
    <div class="min-w-0 flex-1">
      <h3 class="text-sm font-semibold">{t("detail.title", { tool: TOOL_LABEL[app.tool] })}</h3>
      <p class="mt-0.5 text-xs text-muted-foreground">
        {#if app.dirContext}
          {t("detail.dirPrefix")} {app.dirContext.dir}
        {:else}
          {t("detail.dirNone")}
        {/if}
      </p>
    </div>
    <Button size="sm" variant="ghost" title={t("detail.close")} aria-label={t("detail.close")} onclick={() => app.select(null)}>
      <X class="size-3.5" />
    </Button>
  </div>

  {#if sel}
    <div class="space-y-4 px-4 py-4">
      <div>
        <div class="flex flex-wrap items-center gap-2">
          <span class="mono text-xl font-semibold">{sel}</span>
          {#if isActive}
            <Badge tone="accent">{t("common.inUse")}</Badge>
          {:else if isDefault}
            <Badge tone="ok">{t("common.globalDefault")}</Badge>
          {:else if rec}
            <Badge tone="neutral">{t("detail.installedIdle")}</Badge>
          {:else}
            <Badge tone="outline">{t("detail.notInstalled")}</Badge>
          {/if}
          {#if line?.isLts}
            <Badge tone={line.ltsActive === false ? "warn" : "ok"}>LTS</Badge>
          {/if}
          {#if line?.eol === true}
            <Badge tone="bad">{line.eolDate ? t("detail.eolDated", { date: line.eolDate }) : t("detail.eol")}</Badge>
          {/if}
        </div>
        {#if res}
          <p class="mt-1 text-xs leading-relaxed text-muted-foreground">
            {t("detail.nowUses", {
              tool: TOOL_LABEL[app.tool],
              version: res.version ?? t("detail.originalBinary"),
              why: resolutionLabel(res.source),
            })}
            {#if res.foundIn}<span class="mono">（{res.foundIn}）</span>{/if}
          </p>
        {/if}
        {#if res?.note}
          <p class="mt-1 rounded-md bg-warning/10 px-2 py-1 text-xs text-warning">{res.note}</p>
        {/if}
        {#if avail?.unavailableReason}
          <p class="mt-1 rounded-md bg-muted px-2 py-1 text-xs text-muted-foreground">{avail.unavailableReason}</p>
        {/if}
      </div>

      {#if rec}
        <dl class="space-y-2 text-xs">
          <div>
            <dt class="text-muted-foreground">{t("detail.exePath")}</dt>
            <dd class="mono mt-0.5 flex items-start gap-1">
              <span class="min-w-0 flex-1 break-all">{rec.execPath}</span>
              <button
                type="button"
                class="shrink-0 text-muted-foreground hover:text-foreground"
                title={t("detail.revealHere")}
                aria-label={t("detail.revealHere")}
                onclick={() => void api.revealPath(rec.execPath)}
              ><FolderOpen class="size-3.5" /></button>
            </dd>
          </div>
          <div class="flex justify-between"><dt class="text-muted-foreground">{t("detail.versionDir")}</dt><dd class="mono truncate max-w-[11rem]" title={rec.dir}>{rec.dir}</dd></div>
          <div class="flex justify-between"><dt class="text-muted-foreground">{t("detail.source")}</dt><dd>{sourceLabel(rec.source)}</dd></div>
          <div class="flex justify-between"><dt class="text-muted-foreground">{t("detail.diskUsed")}</dt><dd class="mono">{humanBytes(rec.sizeBytes)}</dd></div>
          <div class="flex justify-between"><dt class="text-muted-foreground">{t("detail.installedAt")}</dt><dd class="mono">{shortTime(rec.installedAt)}</dd></div>
          {#if rec.sha256}
            <div>
              <dt class="text-muted-foreground">{t("detail.checksum")}</dt>
              <dd class="mono mt-0.5 break-all">{rec.sha256}</dd>
            </div>
          {/if}
          {#if rec.origin}
            <div>
              <dt class="text-muted-foreground">{t("detail.origin")}</dt>
              <dd class="mono mt-0.5 break-all">{rec.origin}</dd>
            </div>
          {/if}
          {#if !rec.usable}
            <div class="rounded-md border border-destructive/35 bg-destructive/8 px-2 py-1.5">
              <p class="text-destructive">{t("detail.unusable", { reason: rec.unusableReason ?? t("detail.unusableDefault") })}</p>
            </div>
          {/if}
        </dl>
      {:else}
        <dl class="space-y-2 text-xs">
          <div class="flex justify-between"><dt class="text-muted-foreground">{t("detail.downloadSize")}</dt><dd class="mono">{humanBytes(avail?.sizeHint)}</dd></div>
          <div class="flex justify-between"><dt class="text-muted-foreground">{t("detail.released")}</dt><dd class="mono">{avail?.released ?? line?.released ?? t("detail.noRelease")}</dd></div>
          <div class="flex justify-between"><dt class="text-muted-foreground">{t("detail.lineCount")}</dt><dd class="mono">{t("detail.lineCountValue", { n: line?.memberCount ?? 1 })}</dd></div>
          {#if avail?.originTag}
            <div class="flex justify-between"><dt class="text-muted-foreground">{t("detail.originTag")}</dt><dd>{avail.originTag}</dd></div>
          {/if}
          <div class="flex justify-between"><dt class="text-muted-foreground">{t("detail.installPath")}</dt><dd class="mono truncate max-w-[11rem]" title={app.boot?.layout?.installs}>{app.boot?.layout?.installs ?? t("detail.dataDirUnset")}</dd></div>
        </dl>
      {/if}

      {#if rec}
        <div>
          <h4 class="mb-1 text-sm font-semibold">{t("detail.envTitle")}</h4>
          <ul class="divide-y divide-border rounded-md border border-border">
            <li class="flex items-start justify-between gap-2 px-2 py-1.5 text-xs">
              <span class="shrink-0 text-muted-foreground">{t("detail.envAssign")}</span>
              <span class="mono min-w-0 flex-1 break-all text-right">PATH</span>
            </li>
            <li class="flex items-start justify-between gap-2 px-2 py-1.5 text-xs">
              <span class="shrink-0 text-muted-foreground">{t("detail.envEffective")}</span>
              <span class="mono min-w-0 flex-1 break-all text-right">{rec.dir}</span>
            </li>
            <li class="flex items-start justify-between gap-2 px-2 py-1.5 text-xs">
              <span class="shrink-0 text-muted-foreground">{t("detail.shimLocation")}</span>
              <span class="mono min-w-0 flex-1 break-all text-right">{pathNote}</span>
            </li>
          </ul>
        </div>
      {/if}

      {#if rec && rec.companions.length}
        <div>
          <h4 class="mb-1 text-sm font-semibold">{t("detail.companionsTitle")}</h4>
          <ul class="divide-y divide-border rounded-md border border-border">
            {#each rec.companions as c (c.name)}
              <li class="flex items-center gap-2 px-2 py-1.5 text-xs">
                <span class="mono w-16 shrink-0">{c.name}</span>
                <span class="mono min-w-0 flex-1 truncate">{c.version ?? t("detail.companionUnknown")}</span>
                {#if c.note}<span class="text-muted-foreground">{c.note}</span>{/if}
              </li>
            {/each}
          </ul>
        </div>
      {/if}

      <div>
        <h4 class="mb-1 text-sm font-semibold">{t("detail.projectsTitle")}</h4>
        {#if rec && rec.boundProjects.length}
          <ul class="space-y-1">
            {#each rec.boundProjects as p (p)}
              <li class="flex items-center gap-1.5 rounded-md border border-border px-2 py-1 text-xs">
                <FolderOpen class="size-3.5 shrink-0 text-muted-foreground" />
                <span class="mono min-w-0 flex-1 truncate" title={p}>{p}</span>
                <button
                  type="button"
                  class="shrink-0 text-muted-foreground hover:text-foreground"
                  title={t("detail.revealHere")}
                  aria-label={t("detail.revealHere")}
                  onclick={() => void api.revealPath(p)}
                ><ExternalLink class="size-3.5" /></button>
              </li>
            {/each}
          </ul>
        {:else}
          <p class="text-xs text-muted-foreground">{t("detail.noProjects")}</p>
        {/if}
      </div>

      <div class="flex flex-wrap gap-2 border-t border-border pt-3">
        {#if rec}
          <Button size="sm" variant="outline" disabled={isDefault || !rec.usable} onclick={() => void app.switchTo(rec.version)}>
            {isDefault ? t("detail.alreadyDefault") : t("detail.setAsDefault")}
          </Button>
          <Button size="sm" variant="danger" onclick={() => void app.uninstall(rec.version, true)}>{t("label.kind.uninstall")}</Button>
        {:else}
          <Button size="sm" disabled={!!avail?.unavailableReason} onclick={() => void app.install(sel)}>
            {t("detail.installBtn", { version: sel })}
          </Button>
        {/if}
        <Button size="sm" variant="ghost" onclick={() => app.select(null)}>{t("common.close")}</Button>
      </div>
    </div>
  {:else}
    <p class="px-4 py-6 text-center text-xs text-muted-foreground">{t("detail.pickCardHint")}</p>
  {/if}
</aside>
