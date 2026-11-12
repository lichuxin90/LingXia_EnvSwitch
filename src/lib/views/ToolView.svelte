<script lang="ts">
  import { ChevronDown, Download, RefreshCw, LoaderCircle, Search, Sparkles, Trash, ChevronUp } from "@lucide/svelte";
  import Badge from "$lib/components/Badge.svelte";
  import Button from "$lib/components/Button.svelte";
  import Section from "$lib/components/Section.svelte";
  import Switch from "$lib/components/Switch.svelte";
  import { app } from "$lib/state.svelte";
  import { t } from "$lib/i18n/store.svelte";
  import { TOOL_LABEL, humanBytes, shortTime, sourceLabel, sourceName } from "$lib/utils";
  import type { AvailableVersion, VersionLine } from "$lib/types";

  /**
   * 版本行的状态标签：优先显示停更风险，然后显示 LTS 与安装状态。
   */
  function cornerOf(line: VersionLine): { text: string; cls: string; hint: string } {
    if (line.eol === true) {
      return {
        text: t("detail.eol"),
        cls: "bg-destructive/8 text-destructive",
        hint: line.eolDate ? t("tool.eolHintDated", { date: line.eolDate }) : t("tool.eolHint"),
      };
    }
    if (line.isLts && line.ltsActive === false) {
      return { text: t("tool.ltsExpired"), cls: "bg-warning/10 text-warning", hint: t("tool.ltsExpiredHint") };
    }
    if (line.isLts) return { text: t("tool.ltsActive"), cls: "bg-success/10 text-success", hint: t("tool.ltsActiveHint") };
    if (line.installed) return { text: t("tool.installedHere"), cls: "bg-accent text-accent-foreground", hint: t("tool.lineInstalledHint") };
    return { text: t("tool.regular"), cls: "bg-muted text-muted-foreground", hint: t("tool.regularHint") };
  }

  /** 版本行的补充信息。 */
  function tagsOf(line: VersionLine): { tone: "ok" | "warn" | "bad" | "neutral"; text: string }[] {
    const out: { tone: "ok" | "warn" | "bad" | "neutral"; text: string }[] = [];
    if (line.ltsCodename) out.push({ tone: "neutral", text: line.ltsCodename });
    if (line.eol === false && line.eolDate) out.push({ tone: "neutral", text: t("tool.maintainedUntil", { date: line.eolDate }) });
    return out;
  }

  const lines = $derived.by(() => {
    const q = app.filter.trim().toLowerCase();
    const all = app.page?.lines ?? [];
    if (!q) return all;
    return all.filter(
      (l) =>
        l.major.toLowerCase().includes(q) ||
        l.newest.toLowerCase().includes(q) ||
        (l.ltsCodename ?? "").toLowerCase().includes(q),
    );
  });

  const installed = $derived(app.installed?.versions ?? []);
  const activeVersion = $derived(app.installed?.resolution.version ?? null);
  const selected = $derived(app.selected?.tool === app.tool ? app.selected.version : null);

  const expanded = $state<Record<string, boolean>>({});
  let switching = $state(true);
  // Only animate the transition; cached lists remain available immediately.
  $effect(() => {
    const tool = app.tool;
    switching = true;
    const timer = setTimeout(() => { if (app.tool === tool) switching = false; }, 260);
    return () => clearTimeout(timer);
  });

  function toggle(key: string): void {
    expanded[key] = !expanded[key];
  }

  function members(line: VersionLine): AvailableVersion[] {
    return [...line.members].sort((a, b) => (a.version < b.version ? 1 : -1));
  }
</script>

<div class="flex h-full flex-col">
  <header class="tool-header flex items-center gap-3 border-b border-border px-6 py-4">
    <div class="tool-heading flex min-w-0 flex-1 flex-wrap items-baseline gap-x-2 gap-y-1">
      <h2 class="shrink-0 text-base font-semibold">{TOOL_LABEL[app.tool]}</h2>
      <span class="text-xs text-muted-foreground">
        {#if app.page}
          {t("tool.curSource", { src: sourceName(app.tool, app.page.sourceId, app.page.sourceLabel) })}
          {#if app.page.fetchedAt}
            {t("tool.updatedAt", { time: shortTime(app.page.fetchedAt) })}
          {/if}
        {:else}
          {t("tool.fetching")}
        {/if}
      </span>
    </div>
    <div class="tool-search relative ml-auto w-56 shrink-0">
      <Search class="pointer-events-none absolute top-1/2 left-2 size-3.5 -translate-y-1/2 text-muted-foreground" />
      <input
        type="search"
        class="h-8 w-full rounded-md border border-input bg-card pr-2 pl-7 text-sm focus:outline-none focus:ring-2 focus:ring-ring/50"
        placeholder={t("tool.searchPlaceholder")}
        aria-label={t("tool.searchAria")}
        bind:value={app.filter}
      />
    </div>
    <Button variant="outline" size="sm" title={t("tool.refreshTip")} onclick={() => void app.refreshTool(true)}>
      <RefreshCw class="size-3.5 {app.loadingVersions ? 'animate-spin' : ''}" />
      {t("tool.refresh")}
    </Button>
  </header>

  {#if app.tool === "php"}
    <p class="border-b border-border bg-muted/40 px-6 py-3 text-xs leading-relaxed text-muted-foreground">{t("tool.phpNote")}</p>
  {/if}

  {#if app.page?.staleNote}
    <div class="border-b border-warning/30 bg-warning/8 px-5 py-2 text-xs text-warning">
      {app.page.staleNote}
    </div>
  {/if}

  <!-- 点击列表空白处关闭详情，保留版本行与控件的点击操作。 -->
  <div
    class="relative min-h-0 flex-1 overflow-y-auto px-6 py-5"
    aria-busy={switching || app.loadingVersions}
    role="presentation"
    onclick={(e) => {
      if (e.target instanceof Element && !e.target.closest("li, button, input")) app.select(null);
    }}
  >
    {#if app.page && (switching || app.loadingVersions)}
      <div class="runtime-loading-float" role="status"><LoaderCircle class="size-4 animate-spin" />{t("tool.loading", { tool: TOOL_LABEL[app.tool] })}</div>
    {/if}
    {#if app.loadingVersions && !app.page}
      <div class="runtime-loading" role="status"><LoaderCircle class="size-7 animate-spin" /><span>{t("tool.loading", { tool: TOOL_LABEL[app.tool] })}</span></div>
    {:else if lines.length === 0}
      <p class="py-10 text-center text-sm text-muted-foreground">
        {t("tool.noMatch", { q: app.filter })}{#if app.page && app.page.flat.length}{t("tool.noMatchMore", { n: app.page.flat.length })}{/if}
      </p>
    {:else}
      <ul class="version-list overflow-hidden rounded-lg border border-border bg-card">
        {#each lines as line (line.major)}
          {@const corner = cornerOf(line)}
          {@const key = `${app.tool}-${line.major}`}
          {@const using = activeVersion === line.newest}
          {@const open = !!expanded[key]}
          <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_noninteractive_element_interactions -->
          <li class="version-row border-b border-border px-4 py-4 text-left transition-colors last:border-b-0 {selected === line.newest ? 'bg-accent/40' : 'hover:bg-detail'}"
            onclick={() => app.select(line.newest)}>
            <div class="version-row-main">
              <div class="min-w-0">
                <div class="flex flex-wrap items-center gap-2">
                  <button type="button" class="mono text-base font-semibold tracking-tight focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/60"
                    aria-label={t("tool.lineAria", { tool: TOOL_LABEL[app.tool], major: line.major })} aria-pressed={selected === line.newest}>{line.major}<span class="ml-1 text-muted-foreground/40">.x</span></button>
                  <span class="rounded px-1.5 py-0.5 text-[11px] font-medium {corner.cls}" title={corner.hint}>{corner.text}</span>
                  {#if line.isGlobalDefault}<Badge tone="accent">{t("common.globalDefault")}</Badge>{/if}
                </div>
                <p class="mt-1.5 text-xs text-muted-foreground"><span class="mono">{t("tool.newest", { v: line.newest })}</span>{#if line.released}<span class="ml-2">{t("tool.released", { date: line.released })}</span>{/if}</p>
                {#if tagsOf(line).length}<div class="mt-2 flex flex-wrap gap-1">{#each tagsOf(line) as tag (tag.text)}<Badge tone={tag.tone}>{tag.text}</Badge>{/each}</div>{/if}
              </div>
              <div class="version-row-meta text-xs text-muted-foreground">
                <span class="mono">{line.installed ? humanBytes(line.sizeBytes) : '—'}</span>
                <span>{t("tool.memberCountValue", { n: line.memberCount })}</span>
              </div>
              <div class="flex flex-wrap items-center justify-end gap-2" role="presentation" onclick={(e) => e.stopPropagation()}>
                {#if line.installed}
                  <span class="flex items-center gap-1.5">
                    <Switch label={t("tool.useSwitch", { tool: TOOL_LABEL[app.tool], v: line.newest })}
                      title={using ? t("tool.alreadyUsing") : t("tool.turnOnToSwitch")} checked={using}
                      onchange={(next) => { if (next && !using) void app.switchTo(line.newest); }} />
                    <span class="text-xs {using ? 'text-primary' : 'text-muted-foreground'}">{using ? t("common.inUse") : t("tool.switchToUse")}</span>
                  </span>
                  <Button size="sm" variant="ghost" class="hover:text-destructive" title={t("tool.uninstallTip", { v: line.newest })}
                    aria-label={t("tool.uninstallTip", { v: line.newest })} onclick={() => void app.uninstall(line.newest, true)}><Trash class="size-3.5" /></Button>
                {:else}
                  <Button size="sm" variant="outline" onclick={() => void app.install(line.newest)}><Download class="size-3.5" />{t("tool.installShort")}</Button>
                {/if}
                {#if line.memberCount > 1}
                  <button type="button" class="inline-flex h-8 items-center gap-1 rounded-md px-2 text-xs text-muted-foreground transition-colors hover:bg-muted hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
                    aria-expanded={open} onclick={() => toggle(key)}>
                    {#if open}<ChevronUp class="size-3.5" />{:else}<ChevronDown class="size-3.5" />{/if}
                    {open ? t("tool.collapseN", { n: line.memberCount - 1 }) : t("tool.expandN", { n: line.memberCount - 1 })}
                  </button>
                {/if}
              </div>
            </div>

            {#if open}
              <ul class="mt-2 divide-y divide-border overflow-hidden rounded-md border border-border bg-detail">
                {#each members(line) as v (v.version)}
                  <li class="flex items-center gap-2 px-2 py-1.5 text-xs">
                    <span class="mono w-24 shrink-0">{v.version}</span>
                    <span class="w-20 shrink-0 text-muted-foreground">{humanBytes(v.sizeHint)}</span>
                    <span class="min-w-0 flex-1 truncate text-muted-foreground">
                      {v.unavailableReason ?? (v.installed ? t("tool.installedHere") : t("tool.installable"))}
                    </span>
                    {#if v.installed}
                      <Button size="sm" variant="secondary" onclick={() => void app.switchTo(v.version)}>{t("tool.switchShort")}</Button>
                    {:else}
                      <Button
                        size="sm"
                        variant="outline"
                        disabled={!!v.unavailableReason}
                        onclick={() => void app.install(v.version)}
                      >{t("tool.installShort")}</Button>
                    {/if}
                  </li>
                {/each}
              </ul>
            {/if}
          </li>
        {/each}
      </ul>
    {/if}

    <Section title={t("tool.installedTitle")} desc={t("tool.installedDesc")} class="mt-5">
      {#if installed.length === 0}
        <p class="py-4 text-center text-xs text-muted-foreground">
          {t("tool.noneManaged", { tool: TOOL_LABEL[app.tool] })}
        </p>
      {:else}
        <ul class="divide-y divide-border">
          {#each installed as v (v.version)}
            <!-- 同上：整行点得出详情，键盘入口是行首那个版本号按钮。 -->
            <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_noninteractive_element_interactions -->
            <li
              class="flex flex-wrap items-center gap-x-3 gap-y-1 py-2 text-xs transition-colors cursor-default
                {selected === v.version ? 'bg-accent/40' : 'hover:bg-muted/40'}"
              onclick={() => app.select(v.version)}
            >
              <button
                type="button"
                class="mono w-24 shrink-0 text-left text-sm font-medium focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/60"
                aria-label={t("tool.rowAria", { tool: TOOL_LABEL[app.tool], v: v.version })}
                aria-pressed={selected === v.version}
              >{v.version}</button>
              <Badge tone={v.isActive ? "accent" : v.isGlobalDefault ? "ok" : "neutral"}>
                {v.isActive ? t("common.inUse") : v.isGlobalDefault ? t("common.globalDefault") : t("tool.spare")}
              </Badge>
              <Badge tone="outline">{sourceLabel(v.source)}</Badge>
              <span class="w-20 shrink-0 text-muted-foreground">{humanBytes(v.sizeBytes)}</span>
              <span class="w-28 shrink-0 text-muted-foreground">{t("tool.installedOn", { time: shortTime(v.installedAt) })}</span>
              <span class="min-w-0 flex-1 truncate text-muted-foreground" title={v.execPath}>{v.execPath}</span>
              {#if v.boundProjects.length}
                <button
                  type="button"
                  class="shrink-0 text-primary underline-offset-2 hover:underline"
                  title={t("tool.boundTip", { n: v.boundProjects.length })}
                  onclick={(e) => {
                    e.stopPropagation();
                    app.setView("project");
                  }}
                >{t("tool.boundValue", { n: v.boundProjects.length })}</button>
              {/if}
              {#if v.usable}
                <span role="presentation" onclick={(e) => e.stopPropagation()} class="flex shrink-0 items-center gap-2">
                  <Button size="sm" variant="outline" disabled={v.isGlobalDefault} onclick={() => void app.switchTo(v.version)}>
                    {v.isGlobalDefault ? t("tool.alreadyDefault") : t("tool.setAsDefault")}
                  </Button>
                  <Button size="sm" variant="danger" onclick={() => void app.uninstall(v.version, true)}>{t("tool.uninstall")}</Button>
                </span>
              {:else}
                <Badge tone="bad">{t("proj.incomplete")}</Badge>
                <span class="text-muted-foreground">{v.unusableReason}</span>
                <Button size="sm" variant="danger" onclick={() => void app.uninstall(v.version, false)}>{t("tool.removeRecord")}</Button>
              {/if}
            </li>
          {/each}
        </ul>
      {/if}
    </Section>

    {#if app.guesses.length}
      <Section title={t("tool.offlineTitle")} desc={t("tool.offlineDesc")} class="mt-5">
        <ul class="divide-y divide-border">
          {#each app.guesses as g (g.file)}
            <li class="flex flex-wrap items-center gap-x-3 gap-y-1 py-2 text-xs">
              <span class="min-w-0 flex-1 truncate" title={g.file}>{g.fileName}</span>
              <span class="w-20 shrink-0 text-muted-foreground">{humanBytes(g.sizeBytes)}</span>
              <span class="w-24 shrink-0">{g.tool ? TOOL_LABEL[g.tool] : t("tool.unknownTool")}</span>
              <span class="mono w-20 shrink-0">{g.version ?? t("tool.versionPending")}</span>
              {#if g.sidecarSha256}
                <Badge tone="ok">{t("tool.withChecksum")}</Badge>
              {:else}
                <Badge tone="warn">{t("tool.noChecksum")}</Badge>
              {/if}
              {#if g.platformNote}
                <span class="text-warning">{g.platformNote}</span>
              {/if}
              {#if g.ok}
                <Button size="sm" onclick={() => void app.importOffline([g.file])}>
                  <Sparkles class="size-3.5" />
                  {t("tool.installThis")}
                </Button>
              {:else}
                <Badge tone="bad">{g.whyNot ?? t("tool.whyNot")}</Badge>
              {/if}
            </li>
          {/each}
        </ul>
        <div class="mt-2 flex gap-2">
          <Button size="sm" variant="outline" disabled={!app.guesses.some((g) => g.ok)} onclick={() => void app.importOffline(app.guesses.filter((g) => g.ok).map((g) => g.file))}>
            {t("tool.importAll")}
          </Button>
          <Button size="sm" variant="ghost" onclick={() => (app.guesses = [])}>{t("tool.notNow")}</Button>
        </div>
      </Section>
    {/if}
  </div>
</div>

<style>
  .runtime-loading { min-height: 240px; display: flex; flex-direction: column; align-items: center; justify-content: center; gap: 14px; color: var(--primary); font-size: 12px; }
  .runtime-loading-float { position: sticky; top: 8px; z-index: 5; display: flex; align-items: center; justify-content: center; gap: 8px; width: max-content; max-width: 100%; height: 34px; margin: 0 auto -34px; padding: 0 16px; border: 1px solid var(--border); border-radius: 999px; background: var(--card); box-shadow: 0 6px 24px #0000000a; color: var(--primary); font-size: 12px; pointer-events: none; }

  @container (max-width: 760px) {
    .tool-header { flex-wrap: wrap; }
    .tool-heading { flex: 0 0 100%; }
    .tool-search { flex: 1; width: auto; }
  }
  .version-row-main { display: grid; grid-template-columns: minmax(180px, 1fr) auto; align-items: center; gap: 16px; }
  .version-row-meta { display: none; }
  @container (min-width: 880px) {
    .version-row-main { grid-template-columns: minmax(220px, 1fr) minmax(100px, .35fr) auto; }
    .version-row-meta { display: flex; flex-direction: column; gap: 6px; }
  }
  @container (max-width: 540px) {
    .version-row-main { grid-template-columns: minmax(0, 1fr); }
    .version-row-main > :last-child { justify-content: flex-start; }
  }
</style>
