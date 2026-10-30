<script lang="ts">
  import { ChevronDown, ChevronRight, Download, RefreshCw, Search, Sparkles, Trash } from "@lucide/svelte";
  import Badge from "$lib/components/Badge.svelte";
  import Button from "$lib/components/Button.svelte";
  import Section from "$lib/components/Section.svelte";
  import Switch from "$lib/components/Switch.svelte";
  import { app } from "$lib/state.svelte";
  import { t } from "$lib/i18n/store.svelte";
  import { TOOL_LABEL, humanBytes, shortTime, sourceLabel, sourceName } from "$lib/utils";
  import type { AvailableVersion, VersionLine } from "$lib/types";

  /**
   * 卡片右上角的色标：先说风险（停更）再说身份（LTS），每行都写清含义，不做"只在变化时标一次"。
   */
  function cornerOf(line: VersionLine): { text: string; cls: string; hint: string } {
    if (line.eol === true) {
      return {
        text: t("detail.eol"),
        cls: "bg-destructive",
        hint: line.eolDate ? t("tool.eolHintDated", { date: line.eolDate }) : t("tool.eolHint"),
      };
    }
    if (line.isLts && line.ltsActive === false) {
      return { text: t("tool.ltsExpired"), cls: "bg-warning", hint: t("tool.ltsExpiredHint") };
    }
    if (line.isLts) return { text: t("tool.ltsActive"), cls: "bg-success", hint: t("tool.ltsActiveHint") };
    if (line.installed) return { text: t("tool.installedHere"), cls: "bg-primary", hint: t("tool.lineInstalledHint") };
    return { text: t("tool.regular"), cls: "bg-muted-foreground/60", hint: t("tool.regularHint") };
  }

  /** 卡片正文里的状态条：把 LTS / EOL 的具体日期摊开写。 */
  function tagsOf(line: VersionLine): { tone: "ok" | "warn" | "bad" | "neutral"; text: string }[] {
    const out: { tone: "ok" | "warn" | "bad" | "neutral"; text: string }[] = [];
    if (line.isLts) out.push({ tone: line.ltsActive === false ? "warn" : "ok", text: "LTS" });
    if (line.ltsCodename) out.push({ tone: "neutral", text: line.ltsCodename });
    if (line.eol === true) out.push({ tone: "bad", text: line.eolDate ? t("detail.eolDated", { date: line.eolDate }) : t("tool.eolShort") });
    else if (line.eol === false && line.eolDate) out.push({ tone: "neutral", text: t("tool.maintainedUntil", { date: line.eolDate }) });
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

  function toggle(key: string): void {
    expanded[key] = !expanded[key];
  }

  function members(line: VersionLine): AvailableVersion[] {
    return [...line.members].sort((a, b) => (a.version < b.version ? 1 : -1));
  }
</script>

<div class="flex h-full flex-col">
  <header class="flex items-center gap-3 border-b border-border px-6 py-4">
    <div class="flex min-w-0 flex-1 flex-wrap items-baseline gap-x-2 gap-y-1">
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
    <div class="relative ml-auto w-56">
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

  {#if app.page?.staleNote}
    <div class="border-b border-warning/30 bg-warning/8 px-5 py-2 text-xs text-warning">
      {app.page.staleNote}
    </div>
  {/if}

  <!-- 点空白处不放详情；只有点到具体卡片才在右边展开。 -->
  <div
    class="min-h-0 flex-1 overflow-y-auto px-6 py-5"
    role="presentation"
    onclick={() => app.select(null)}
  >
    {#if app.loadingVersions && !app.page}
      <ul class="grid grid-cols-2 gap-4 xl:grid-cols-3">
        {#each Array(6) as _, i}
          <li class="h-32 animate-pulse rounded-lg border border-border bg-card" aria-hidden="true" style="animation-delay: {i * 60}ms"></li>
        {/each}
      </ul>
    {:else if lines.length === 0}
      <p class="py-10 text-center text-sm text-muted-foreground">
        {t("tool.noMatch", { q: app.filter })}{#if app.page && app.page.flat.length}{t("tool.noMatchMore", { n: app.page.flat.length })}{/if}
      </p>
    {:else}
      <ul class="grid grid-cols-2 gap-4 xl:grid-cols-3">
        {#each lines as line (line.major)}
          {@const corner = cornerOf(line)}
          {@const key = `${app.tool}-${line.major}`}
          {@const using = activeVersion === line.newest}
          {@const open = !!expanded[key]}
          <!-- 整张卡片都可以点出详情；键盘走上面那个标题按钮，所以这里不挂额外按键。 -->
          <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_noninteractive_element_interactions -->
          <li
            class="relative cursor-default rounded-lg border p-4 text-left transition-all
              {selected === line.newest
                ? 'border-primary bg-accent/50'
                : 'border-border bg-card elevate hover:border-primary/40'}"
            onclick={() => app.select(line.newest)}
          >
            <span
              class="absolute top-0 right-0 rounded-bl-md px-1.5 py-0.5 text-xs font-medium text-white {corner.cls}"
              title={corner.hint}
            >{corner.text}</span>

            <!-- 卡片整体也能点，但这个标题才是键盘可达的入口：按回车同样展开右侧详情。 -->
            <button
              type="button"
              class="flex items-baseline gap-1.5 pr-24 text-left focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/60"
              aria-label={t("tool.lineAria", { tool: TOOL_LABEL[app.tool], major: line.major })}
              aria-pressed={selected === line.newest}
            >
              <span class="text-lg leading-none font-semibold">{TOOL_LABEL[app.tool]}</span>
              <span class="mono text-lg leading-none font-semibold">{line.major}</span>
            </button>
            <p class="mono mt-1 text-xs text-muted-foreground">
              {t("tool.newest", { v: line.newest })}
              {#if line.released}<span>{t("tool.released", { date: line.released })}</span>{/if}
            </p>

            {#if tagsOf(line).length}
              <div class="mt-1.5 flex flex-wrap gap-1">
                {#each tagsOf(line) as tag (tag.text)}
                  <Badge tone={tag.tone}>{tag.text}</Badge>
                {/each}
              </div>
            {/if}

            <dl class="mt-2 space-y-0.5 text-xs text-muted-foreground">
              <div class="flex justify-between"><dt>{t("tool.diskUsed")}</dt><dd class="mono">{line.installed ? humanBytes(line.sizeBytes) : t("tool.notInstalledWord")}</dd></div>
              <div class="flex justify-between"><dt>{t("tool.memberCount")}</dt><dd class="mono">{t("tool.memberCountValue", { n: line.memberCount })}</dd></div>
              <div class="flex justify-between">
                <dt>{t("tool.status")}</dt>
                <dd>
                  {#if line.isGlobalDefault}
                    <Badge tone="ok">{t("common.globalDefault")}</Badge>
                  {:else if line.installed}
                    <Badge tone="ok">{t("tool.installedBadge")}</Badge>
                  {:else}
                    <Badge tone="outline">{t("tool.notInstalledBadge")}</Badge>
                  {/if}
                </dd>
              </div>
            </dl>

            <div class="mt-3 flex flex-wrap items-center gap-2" role="presentation" onclick={(e) => e.stopPropagation()}>
              {#if line.installed}
                <span class="flex items-center gap-1.5">
                  <Switch
                    label={t("tool.useSwitch", { tool: TOOL_LABEL[app.tool], v: line.newest })}
                    title={using ? t("tool.alreadyUsing") : t("tool.turnOnToSwitch")}
                    checked={using}
                    onchange={(next) => {
                      if (next && !using) void app.switchTo(line.newest);
                    }}
                  />
                  <span class="text-xs {using ? 'font-medium text-primary' : 'text-muted-foreground'}">
                    {using ? t("common.inUse") : t("tool.switchToUse")}
                  </span>
                </span>
                <Button size="sm" variant="danger" title={t("tool.uninstallTip", { v: line.newest })} onclick={() => void app.uninstall(line.newest, true)}>
                  <Trash class="size-3.5" />
                  {t("tool.uninstall")}
                </Button>
              {:else}
                <Button size="sm" onclick={() => void app.install(line.newest)}>
                  <Download class="size-3.5" />
                  {t("detail.installBtn", { version: line.newest })}
                </Button>
              {/if}
              {#if line.memberCount > 1}
                <button
                  type="button"
                  class="ml-auto inline-flex items-center gap-1 rounded-full border border-input bg-card px-2 py-0.5 text-xs text-muted-foreground transition-colors hover:border-primary/50 hover:bg-accent hover:text-accent-foreground"
                  aria-expanded={open}
                  onclick={() => toggle(key)}
                >
                  {#if open}
                    <ChevronDown class="size-3" />
                    {t("tool.collapseN", { n: line.memberCount - 1 })}
                  {:else}
                    <ChevronRight class="size-3" />
                    {t("tool.expandN", { n: line.memberCount - 1 })}
                  {/if}
                </button>
              {/if}
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
