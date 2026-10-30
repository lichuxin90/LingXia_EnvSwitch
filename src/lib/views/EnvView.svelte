<script lang="ts">
  import { ChevronDown, RotateCcw, ScanSearch, ShieldAlert, Sparkles, TriangleAlert, Wand } from "@lucide/svelte";
  import Badge from "$lib/components/Badge.svelte";
  import Button from "$lib/components/Button.svelte";
  import Field from "$lib/components/Field.svelte";
  import Hint from "$lib/components/Hint.svelte";
  import Section from "$lib/components/Section.svelte";
  import Segmented from "$lib/components/Segmented.svelte";
  import Switch from "$lib/components/Switch.svelte";
  import { api } from "$lib/ipc";
  import { t, type MsgKey } from "$lib/i18n/store.svelte";
  import { app } from "$lib/state.svelte";
  import { humanBytes, scopeLabel, TOOL_LABEL, shortTime } from "$lib/utils";
  import type { PathScope, ToolId } from "$lib/types";

  /** PATH 读自哪里由后端给标识，界面翻成界面语言；认不出的照原文显示。 */
  const FROM_KEY: Record<string, MsgKey> = {
    registry: "env.fromRegistry",
    session: "env.fromSession",
  };
  const fromLabel = (from: string): string => {
    const key = FROM_KEY[from];
    return key ? t(key) : from;
  };

  let backups = $state<Awaited<ReturnType<typeof api.pathBackups>>>([]);
  let view = $state<Awaited<ReturnType<typeof api.pathView>> | null>(null);
  let preview = $state<Awaited<ReturnType<typeof api.resetPreview>> | null>(null);
  let showPreview = $state(false);
  let busy = $state(false);
  /** 备份列表默认只露最近 3 条，其余点「查看更多」再展开。 */
  let showAllBackups = $state(false);

  const shownBackups = $derived(showAllBackups ? backups : backups.slice(0, 3));

  async function reload(): Promise<void> {
    busy = true;
    try {
      await app.refreshEnv();
      backups = await api.pathBackups();
      view = await api.pathView();
    } catch (e) {
      app.fail(e, t("env.reloadFailed"));
    } finally {
      busy = false;
    }
  }

  $effect(() => {
    if (app.view === "env") void reload();
  });

  const userStatus = $derived(app.paths.find((p) => p.scope === "user") ?? null);
  const systemStatus = $derived(app.paths.find((p) => p.scope === "system") ?? null);
  const activeScope = $derived(app.settings?.pathScope ?? "user");

  async function adopt(exe: string, tool: ToolId, version: string | null, copy: boolean): Promise<void> {
    await app.adopt(exe, tool, version, copy);
  }

  async function openPreview(): Promise<void> {
    try {
      preview = await api.resetPreview();
      showPreview = true;
    } catch (e) {
      app.fail(e, t("env.previewFailed"));
    }
  }

  async function doReset(keepInstalls: boolean): Promise<void> {
    showPreview = false;
    await app.resetNow(keepInstalls);
    await reload();
  }
</script>

<div class="flex h-full flex-col overflow-y-auto">
  <header class="flex items-center gap-3 border-b border-border px-6 py-4">
    <h2 class="text-base font-semibold">{t("env.title")}</h2>
    <Hint text={t("env.tagline")} />
    <div class="ml-auto">
      <Button variant="outline" size="sm" onclick={() => void reload()}>
        <ScanSearch class="size-3.5 {busy ? 'animate-spin' : ''}" />
        {t("env.recheck")}
      </Button>
    </div>
  </header>

  <div class="space-y-5 px-6 py-5">
    {#if !app.boot?.elevated}
      <p class="flex items-start gap-2 rounded-lg border border-border bg-muted/40 px-3 py-2 text-xs text-muted-foreground">
        <ShieldAlert class="mt-0.5 size-3.5 shrink-0" />
        <span>{t("env.notAdminNote")}</span>
      </p>
    {/if}

    <Section title={t("env.pathStateTitle")} desc={t("env.pathStateDesc")}>
      <ul class="divide-y divide-border">
        {#each [userStatus, systemStatus] as st (st?.scope ?? "none")}
          {#if st}
            <li class="py-2 text-xs">
              <div class="flex flex-wrap items-center gap-2">
                <span class="w-24 shrink-0 font-medium">{scopeLabel(st.scope)}</span>
                <Badge tone={st.installed ? (st.isFirst ? "ok" : "warn") : "outline"}>
                  {st.installed ? (st.isFirst ? t("env.installedFirst") : t("env.installedNotFirst")) : t("env.notInstalled")}
                </Badge>
                <span class="text-muted-foreground">
                  {t("env.entryLine", { pos: st.position === null ? "—" : st.position + 1, n: st.backupCount })}
                  {#if st.latestBackup}{t("env.lastBackup", { time: shortTime(st.latestBackup) })}{/if}
                </span>
                <div class="ml-auto flex gap-1">
                  <Button size="sm" variant="outline" onclick={() => void app.pathApply(st.scope)}>
                    {st.installed ? t("env.writeAgain") : t("env.writeIn")}
                  </Button>
                  <Button size="sm" variant="danger" disabled={!st.installed} onclick={() => void app.pathWithdraw(st.scope)}>
                    {t("env.withdraw")}
                  </Button>
                </div>
              </div>
              {#if st.note}
                <p class="mt-1 pl-24 text-muted-foreground">{st.note}</p>
              {/if}
            </li>
          {:else}
            <li class="py-2 text-xs text-muted-foreground">{t("env.segmentMissing")}</li>
          {/if}
        {/each}
      </ul>

      <div class="mt-2 border-t border-border pt-2">
        <Field label={t("env.priorityField")} hint={t("env.priorityHint")}>
          <Switch
            label={t("env.prioritySwitch")}
            checked={app.settings?.shimPriority ?? true}
            onchange={(next) =>
              void app.patch(
                { shimPriority: next },
                next ? t("env.priorityOn") : t("env.priorityOff"),
              )}
          />
        </Field>
        <div class="pt-3">
          <p class="flex items-center gap-1.5 text-sm font-medium">
            {t("env.scopeTitle")}
            <Hint text={t("env.scopeDesc")} />
          </p>
          <Segmented
            class="mt-3"
            fill
            ariaLabel={t("env.scopeAria")}
            value={activeScope}
            items={[
              { value: "user", label: t("env.scopeUser") },
              { value: "system", label: t("env.scopeSystem"), hint: t("env.scopeSystemHint") },
            ]}
            onchange={(next) => void app.patch({ pathScope: next as PathScope }, t("env.scopeChanged"))}
          />
        </div>
        <Field label={t("env.autoSwitchField")} hint={t("env.autoSwitchHint")}>
          <Switch
            label={t("env.autoSwitchLabel")}
            checked={app.settings?.autoSwitch ?? true}
            onchange={(next) => void app.patch({ autoSwitch: next })}
          />
        </Field>
      </div>
    </Section>

    <Section title={t("env.shimTitle")} desc={t("env.shimDesc")}>
      {#if app.boot}
        <ul class="mb-2 flex flex-wrap gap-1.5">
          {#each app.boot.shims as s (s.name)}
            <li>
              <Badge tone={s.exists ? "ok" : "bad"}>
                {s.name}{#if s.tool}（{TOOL_LABEL[s.tool]}）{/if}
              </Badge>
            </li>
          {/each}
          {#if app.boot.shims.length === 0}
            <li class="text-xs text-muted-foreground">{t("env.shimEmpty")}</li>
          {/if}
        </ul>
      {/if}
      <div class="flex gap-2">
        <Button size="sm" variant="outline" onclick={() => void app.shimSync()}>
          <Wand class="size-3.5" />
          {t("env.shimSync")}
        </Button>
        <Button size="sm" variant="ghost" onclick={() => void app.revealPath(app.boot?.layout?.shims ?? null)}>
          {t("env.openDir")}
        </Button>
      </div>
    </Section>

    <Section title={t("env.hookTitle")} desc={t("env.hookDesc")}>
      <ul class="divide-y divide-border">
        {#each app.hooks as h (h.shell)}
          <li class="flex flex-wrap items-center gap-2 py-2 text-xs">
            <span class="w-28 shrink-0 font-medium">{h.label}</span>
            <Badge tone={h.installed ? "ok" : h.detected ? "outline" : "neutral"}>
              {h.installed ? t("env.hookOn") : h.detected ? t("env.hookAvailable") : t("env.hookAbsent")}
            </Badge>
            {#if h.installed && !h.scriptReady}
              <Badge tone="warn">{t("env.hookScriptMissing")}</Badge>
            {/if}
            <span class="min-w-0 flex-1 truncate text-muted-foreground" title={h.rcFile ?? h.script}>
              {h.rcFile ?? t("env.hookNoRc")}
            </span>
            <Switch
              label={t("env.hookSwitch", { label: h.label })}
              checked={h.installed}
              onchange={(next) => void app.hookToggle(h.shell, next)}
            />
          </li>
          {#if h.note}
            <li class="pb-2 text-xs text-muted-foreground">{h.note}</li>
          {/if}
        {/each}
        {#if app.hooks.length === 0}
          <li class="py-2 text-xs text-muted-foreground">{t("env.hookEmpty")}</li>
        {/if}
      </ul>
    </Section>

    <Section title={t("env.orderTitle")} desc={t("env.orderDesc")}>
      {#if view}
        <p class="mb-2 text-xs text-muted-foreground">{t("env.orderFrom", { from: fromLabel(view.from) })}</p>
        <ol class="mono max-h-56 space-y-0.5 overflow-y-auto text-xs">
          {#each view.entries as e, i}
            <li class={e === view.shimDir ? "rounded bg-primary/12 px-1 text-primary" : "px-1"}>
              {i + 1}. {e}
            </li>
          {/each}
        </ol>
      {:else}
        <p class="text-xs text-muted-foreground">{t("env.orderEmpty")}</p>
      {/if}
    </Section>

    <Section title={t("env.scanTitle")} desc={t("env.scanDesc")}>
      {#if app.scan?.conflicts.length}
        <ul class="mb-3 space-y-2">
          {#each app.scan.conflicts as c (c.name + (c.path ?? ""))}
            <li class="flex items-start gap-2 rounded-lg border border-warning/35 bg-warning/8 px-2.5 py-2 text-xs">
              <TriangleAlert class="mt-0.5 size-3.5 shrink-0 text-warning" />
              <div class="min-w-0">
                <p class="font-medium">{c.name} · {c.detail}</p>
                <p class="mt-0.5 text-muted-foreground">{c.suggestion}</p>
                {#if c.path}<p class="mono mt-0.5 break-all text-muted-foreground">{c.path}</p>{/if}
              </div>
            </li>
          {/each}
        </ul>
      {:else}
        <p class="mb-3 text-xs text-muted-foreground">{t("env.noConflict")}</p>
      {/if}

      {#if app.scan?.foreign.length}
        <ul class="divide-y divide-border">
          {#each app.scan.foreign as f (f.exe)}
            <li class="flex flex-wrap items-center gap-2 py-2 text-xs">
              <span class="w-24 shrink-0 font-medium">{f.name}</span>
              <span class="mono w-20 shrink-0">{f.version ?? t("env.versionUnread")}</span>
              {#if f.manager}<Badge tone="outline">{f.manager}</Badge>{/if}
              <span class="min-w-0 flex-1 truncate text-muted-foreground" title={f.exe}>{f.exe}</span>
              {#if f.takeable && f.tool}
                <Button size="sm" variant="outline" title={t("env.takeoverTip")} onclick={() => void adopt(f.exe, f.tool as ToolId, f.version, false)}>
                  <Sparkles class="size-3.5" />
                  {t("env.takeover")}
                </Button>
                <Button size="sm" variant="ghost" title={t("env.copyInTip")} onclick={() => void adopt(f.exe, f.tool as ToolId, f.version, true)}>
                  {t("env.copyIn")}
                </Button>
              {:else}
                <span class="text-muted-foreground">{f.whyNot ?? t("env.notTakeable")}</span>
              {/if}
            </li>
          {/each}
        </ul>
      {:else}
        <p class="text-xs text-muted-foreground">{t("env.noForeign")}</p>
      {/if}

      {#if app.scan?.notes.length}
        <ul class="mt-3 space-y-1 border-t border-border pt-2 text-xs text-muted-foreground">
          {#each app.scan.notes as n (n)}
            <li>{n}</li>
          {/each}
        </ul>
      {/if}
    </Section>

    <Section title={t("env.backupTitle")} desc={t("env.backupDesc")}>
      {#if backups.length === 0}
        <p class="py-2 text-center text-xs text-muted-foreground">{t("env.backupEmpty")}</p>
      {:else}
        <ul class="divide-y divide-border">
          {#each shownBackups as b, i (b.id)}
            <li class="flex flex-wrap items-center gap-2 py-2 text-xs">
              <span class="mono w-6 shrink-0 text-muted-foreground">{backups.length - i}</span>
              <span class="mono w-40 shrink-0">{b.id}</span>
              <Badge tone="outline">{scopeLabel(b.scope)}</Badge>
              <Badge tone={b.kind === "path" ? "neutral" : "accent"}>
                {b.kind === "path" ? t("env.backupKindPath") : t("env.backupKindConfig")}
              </Badge>
              <span class="w-16 shrink-0 text-muted-foreground">{t("env.backupEntries", { n: b.entryCount })}</span>
              <span class="min-w-0 flex-1 truncate text-muted-foreground">{b.preview.join(" | ")}</span>
              <span class="mono shrink-0 text-muted-foreground">{shortTime(b.createdAt)}</span>
              <Button size="sm" variant="outline" onclick={() => void app.pathRestore(b.id)}>
                <RotateCcw class="size-3.5" />
                {t("env.backupRestore")}
              </Button>
            </li>
          {/each}
        </ul>
        {#if backups.length > 3}
          <div class="mt-2 flex items-center gap-2">
            <Button size="sm" variant="outline" onclick={() => (showAllBackups = !showAllBackups)}>
              <ChevronDown class="size-3.5 {showAllBackups ? 'rotate-180' : ''}" />
              {showAllBackups ? t("env.backupCollapse") : t("env.backupExpand", { n: backups.length, m: backups.length - 3 })}
            </Button>
            <Hint text={t("env.backupFoot")} />
          </div>
        {:else}
          <p class="mt-2 text-xs text-muted-foreground">{t("env.backupAll", { n: backups.length })}</p>
        {/if}
      {/if}
    </Section>

    <Section title={t("env.resetTitle")} desc={t("env.resetDesc")}>
      {#if showPreview && preview}
        <ul class="mb-3 space-y-1 text-xs">
          <li class="flex justify-between"><span class="text-muted-foreground">{t("env.resetShimDir")}</span><span class="mono">{preview.shimDir}</span></li>
          <li class="flex justify-between"><span class="text-muted-foreground">{t("env.resetShims")}</span><span>{t("env.resetShimCount", { n: preview.shims })}</span></li>
          <li class="flex justify-between"><span class="text-muted-foreground">{t("env.resetScope")}</span><span>{scopeLabel(preview.scope)}</span></li>
          <li class="flex justify-between"><span class="text-muted-foreground">{t("env.resetBackups")}</span><span>{t("env.resetBackupCount", { n: preview.backupCount })}</span></li>
          <li class="flex justify-between"><span class="text-muted-foreground">{t("env.resetHooks")}</span><span>{preview.hooks.length ? preview.hooks.join("、") : t("env.resetHooksNone")}</span></li>
          <li class="flex justify-between"><span class="text-muted-foreground">{t("env.resetInstalls")}</span><span>{t("env.resetInstallsValue", { n: preview.installedCount, m: preview.referencedCount })}</span></li>
          <li class="flex justify-between"><span class="text-muted-foreground">{t("env.resetSize")}</span><span class="mono">{humanBytes(preview.installsBytes)}</span></li>
        </ul>
        <div class="flex gap-2">
          <Button size="sm" variant="outline" onclick={() => { showPreview = false; }}>{t("env.resetHold")}</Button>
          <Button size="sm" variant="danger" onclick={() => void doReset(true)}>
            {t("env.resetKeep")}
          </Button>
          <Button size="sm" variant="danger" onclick={() => void doReset(false)}>
            {t("env.resetPurge")}
          </Button>
        </div>
      {:else}
        <div class="flex items-center gap-2">
          <Button size="sm" variant="danger" onclick={() => void openPreview()}>{t("env.resetPreview")}</Button>
          <Hint text={t("env.resetFoot")} />
        </div>
      {/if}
    </Section>
  </div>
</div>
