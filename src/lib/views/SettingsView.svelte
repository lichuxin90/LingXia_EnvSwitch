<script lang="ts">
  import { ChevronRight, Download, FolderOpen, Globe, Info, LogIn, Monitor, Moon, ShieldCheck, Sun, Upload } from "@lucide/svelte";
  import Button from "$lib/components/Button.svelte";
  import Field from "$lib/components/Field.svelte";
  import Hint from "$lib/components/Hint.svelte";
  import Section from "$lib/components/Section.svelte";
  import Segmented from "$lib/components/Segmented.svelte";
  import Switch from "$lib/components/Switch.svelte";
  import { api } from "$lib/ipc";
  import { LOCALE_LABEL, t, type Locale, type MsgKey } from "$lib/i18n/store.svelte";
  import { app, type View } from "$lib/state.svelte";
  import { ALL_TOOLS, TOOL_LABEL, humanBytes, shortTime } from "$lib/utils";
  import type { InstalledVersion, Language, ToolId } from "$lib/types";

  /** 浮层里的分区：左边选，右边只出现选中的那一块，跟主侧栏一个用法。 */
  type Pane = "general" | "dirs" | "share" | "usage";
  const panes: { key: Pane; label: MsgKey }[] = [
    { key: "general", label: "set.paneGeneral" },
    { key: "dirs", label: "set.dirTitle" },
    { key: "share", label: "set.shareTitle" },
    { key: "usage", label: "set.usageTitle" },
  ];

  /** 管理入口不在设置里重复一套配置界面，点一下直接进对应页面；版本清单侧栏本来就有，不再列一遍。 */
  const manage: { view: View; key: MsgKey; icon: typeof Globe }[] = [
    { view: "mirror", key: "nav.mirror", icon: Globe },
    { view: "env", key: "nav.env", icon: ShieldCheck },
    { view: "about", key: "nav.about", icon: Info },
  ];

  let pane = $state<Pane>("general");

  function openManage(view: View): void {
    app.closeSettings();
    app.setView(view);
  }

  let perTool = $state<Record<ToolId, InstalledVersion[]> | null>(null);

  async function reloadUsage(): Promise<void> {
    try {
      const pages = await Promise.all(ALL_TOOLS.map((tool) => api.installedVersions(tool, null)));
      perTool = Object.fromEntries(ALL_TOOLS.map((tool, i) => [tool, pages[i].versions])) as Record<
        ToolId,
        InstalledVersion[]
      >;
    } catch (e) {
      app.fail(e, t("set.usageFailed"));
    }
  }

  $effect(() => {
    if (app.settingsOpen) void reloadUsage();
  });

  const usage = $derived(
    Object.entries(perTool ?? {}).flatMap(([tool, list]) =>
      list.map((v) => ({ tool: tool as ToolId, v })),
    ),
  );
  const usageTotal = $derived(usage.reduce((s, r) => s + r.v.sizeBytes, 0));

  const dirs = $derived(
    app.boot?.layout
      ? [
          [t("set.dirRoot"), app.boot.layout.root],
          [t("set.dirInstalls"), app.boot.layout.installs],
          [t("set.dirShims"), app.boot.layout.shims],
          [t("set.dirCache"), app.boot.layout.cache],
          [t("set.dirBackups"), app.boot.layout.backups],
          [t("set.dirConfig"), app.boot.layout.config],
          [t("set.dirLogs"), app.boot.layout.logs],
        ]
      : [],
  );

  /** 语言选择器露出各语言的本族名称，只有「跟随系统」跟着界面语言翻。 */
  const langItems = $derived([
    { value: "auto", label: t("set.langAuto") },
    ...(Object.keys(LOCALE_LABEL) as Locale[]).map((loc) => ({ value: loc, label: LOCALE_LABEL[loc] })),
  ]);

  async function moveRoot(): Promise<void> {
    const dir = await api.pickFolder();
    if (dir) await app.chooseDataRoot(dir);
  }
</script>

<!-- min-w-0 不能省：整页覆盖后这一栏是弹性子项，启动日志那串长路径会把宽度顶到窗口外面。 -->
<div class="flex min-h-0 min-w-0 flex-1 flex-col md:flex-row">
  <!-- 浮层内的侧栏：设置分区留在原地，管理四项跳出去看整页。 -->
  <aside class="shrink-0 border-b border-border p-3 md:w-64 md:border-r md:border-b-0">
    <p class="px-2 pb-1 text-xs font-medium tracking-wide text-muted-foreground uppercase">{t("nav.settings")}</p>
    <ul class="space-y-0.5">
      {#each panes as p (p.key)}
        <li>
          <button
            type="button"
            class="flex w-full items-center gap-2 rounded-md px-2.5 py-2 text-left text-sm transition-colors
              {pane === p.key
                ? 'bg-primary font-medium text-primary-foreground'
                : 'text-muted-foreground hover:bg-accent/60 hover:text-foreground'}"
            aria-current={pane === p.key ? "true" : undefined}
            onclick={() => (pane = p.key)}
          >
            <span class="min-w-0 flex-1">{t(p.label)}</span>
            {#if pane === p.key}<ChevronRight class="size-3.5 shrink-0" />{/if}
          </button>
        </li>
      {/each}
    </ul>

    <p class="mt-3 px-2 pb-1 text-xs font-medium tracking-wide text-muted-foreground uppercase">{t("set.manageTitle")}</p>
    <ul class="space-y-0.5">
      {#each manage as m (m.view)}
        {@const Icon = m.icon}
        <li>
          <button
            type="button"
            class="flex w-full items-center gap-2 rounded-md px-2.5 py-2 text-left text-sm text-muted-foreground transition-colors hover:bg-accent/60 hover:text-foreground"
            aria-label={t(m.key)}
            onclick={() => openManage(m.view)}
          >
            <Icon class="size-4 shrink-0" />
            <span class="min-w-0 flex-1">{t(m.key)}</span>
            <ChevronRight class="size-3.5 shrink-0" />
          </button>
        </li>
      {/each}
    </ul>
  </aside>

  <div class="min-h-0 min-w-0 flex-1 overflow-y-auto px-6 py-5">
    {#if pane === "general"}
      <Section title={t("set.startupTitle")} desc={t("set.startupDesc")}>
        <Field label={t("set.autostartField")} hint={t("set.autostartHint")}>
          <Switch
            label={t("home.autostartSwitch")}
            checked={app.settings?.autostart ?? false}
            onchange={async (next) => {
              try {
                const on = await api.setAutostart(next);
                await app.patch({ autostart: on }, on ? t("set.autostartOn") : t("set.autostartOff"));
              } catch (e) {
                app.fail(e, t("set.autostartFailed"));
              }
            }}
          />
        </Field>
        <div class="pt-3">
          <p class="flex items-center gap-2 text-sm font-medium">
            {t("set.logTitle")}
            <Hint text={t("set.logDesc")} />
          </p>
          <div class="mt-2 flex flex-wrap items-center gap-2">
            <span class="mono min-w-0 flex-1 truncate text-xs text-muted-foreground" title={app.startupLog}>
              {app.startupLog || t("set.logUnknown")}
            </span>
            <Button
              size="sm"
              variant="outline"
              disabled={!app.startupLog}
              onclick={() => void api.revealPath(app.startupLog)}
            >
              <FolderOpen class="size-3.5" />
              {t("app.openLog")}
            </Button>
          </div>
        </div>
        <div class="pt-3">
          <p class="flex items-center gap-2 text-sm font-medium">
            {t("set.closeTitle")}
            <Hint text={t("set.closeDesc")} />
          </p>
          <Segmented
            class="mt-3"
            fill
            ariaLabel={t("set.closeAria")}
            value={app.settings?.trayBehavior ?? "minimizeToTray"}
            items={[
              { value: "minimizeToTray", label: t("set.closeTray") },
              { value: "quit", label: t("set.closeQuit") },
            ]}
            onchange={(next) => void app.patch({ trayBehavior: next as "minimizeToTray" })}
          />
        </div>
        <div class="pt-3">
          <p class="flex items-center gap-2 text-sm font-medium">
            {t("set.themeTitle")}
            <Hint text={t("set.themeDesc")} />
          </p>
          <Segmented
            class="mt-3"
            fill
            ariaLabel={t("set.themeTitle")}
            value={app.settings?.theme ?? "system"}
            items={[
              { value: "system", label: t("set.themeSystem"), icon: Monitor },
              { value: "light", label: t("set.themeLight"), icon: Sun },
              { value: "dark", label: t("set.themeDark"), icon: Moon },
            ]}
            onchange={(next) => void app.patch({ theme: next as "system" })}
          />
        </div>
        <div class="pt-3">
          <p class="flex items-center gap-2 text-sm font-medium">
            {t("set.langTitle")}
            <Hint text={t("set.langDesc")} />
          </p>
          <Segmented
            class="mt-3"
            fill
            ariaLabel={t("set.langTitle")}
            value={app.settings?.language ?? "auto"}
            items={langItems}
            onchange={(next) => void app.patch({ language: next as Language }, t("set.langChanged"))}
          />
        </div>
      </Section>
    {:else if pane === "dirs"}
      <Section title={t("set.dirTitle")} desc={t("set.dirDesc")}>
        <ul class="mb-2 space-y-1">
          {#each dirs as [label, path] (label)}
            <li class="flex items-center gap-2 text-xs">
              <span class="w-20 shrink-0 text-muted-foreground">{label}</span>
              <span class="mono min-w-0 flex-1 truncate" title={path}>{path}</span>
              <button
                type="button"
                class="shrink-0 text-muted-foreground hover:text-foreground"
                title={t("set.openDir", { label })}
                aria-label={t("set.openDir", { label })}
                onclick={() => void api.revealPath(path)}
              ><FolderOpen class="size-3.5" /></button>
            </li>
          {/each}
        </ul>
        <div class="flex items-center gap-2">
          <Button size="sm" variant="outline" onclick={moveRoot}>{t("set.moveRoot")}</Button>
          <span class="text-xs text-muted-foreground">
            {#if app.boot?.layout}
              {app.boot.layout.besideExe ? t("set.besideExe") : t("set.pointerFile")}
            {/if}
          </span>
        </div>
      </Section>
    {:else if pane === "share"}
      <Section title={t("set.shareTitle")} desc={t("set.shareDesc")}>
        <div class="flex flex-wrap gap-2">
          <Button size="sm" variant="outline" onclick={() => void app.exportConfig()}>
            <Download class="size-3.5" />
            {t("set.export")}
          </Button>
          <Button size="sm" variant="outline" onclick={() => void app.importConfig()}>
            <Upload class="size-3.5" />
            {t("set.import")}
          </Button>
          <Button size="sm" variant="ghost" onclick={() => void app.revealPath(app.boot?.layout?.config ?? null)}>
            <LogIn class="size-3.5" />
            {t("set.openConfig")}
          </Button>
        </div>
        <p class="mt-2 text-xs text-muted-foreground">{t("set.importFoot")}</p>
      </Section>
    {:else}
      <Section title={t("set.usageTitle")} desc={t("set.usageDesc")}>
        {#if usage.length}
          <ul class="space-y-1 text-xs">
            {#each usage as { tool, v } (`${tool}-${v.version}`)}
              <li class="flex items-center gap-2">
                <span class="w-16 shrink-0 text-muted-foreground">{TOOL_LABEL[tool]}</span>
                <span class="mono w-24 shrink-0">{v.version}</span>
                <span class="w-20 shrink-0 text-muted-foreground">{humanBytes(v.sizeBytes)}</span>
                <span class="min-w-0 flex-1 truncate text-muted-foreground" title={v.dir}>{v.dir}</span>
                <span class="shrink-0 text-muted-foreground">{shortTime(v.installedAt)}</span>
              </li>
            {/each}
            <li class="flex items-center gap-2 border-t border-border pt-1 text-muted-foreground">
              <span class="w-16 shrink-0">{t("set.total")}</span>
              <span class="mono w-24 shrink-0">{t("set.totalValue", { n: usage.length })}</span>
              <span class="w-20 shrink-0">{humanBytes(usageTotal)}</span>
              <span class="min-w-0 flex-1">{t("set.totalFoot")}</span>
            </li>
          </ul>
        {:else}
          <p class="text-xs text-muted-foreground">{t("set.noneManaged")}</p>
        {/if}
        <p class="mt-2 text-xs text-muted-foreground">
          {#if app.settings?.keepArchives}
            {t("set.keepNote")}
          {:else}
            {t("set.purgeNote")}
          {/if}
        </p>
      </Section>
    {/if}
  </div>
</div>
