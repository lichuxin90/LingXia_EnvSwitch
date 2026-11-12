<script lang="ts">
  import WindowBar from "$lib/components/WindowBar.svelte";
  import {
    ArrowLeft,
    Bot,
    Boxes,
    ChevronDown,
    FolderTree,
    Globe,
    Terminal,
    House,
    Lock,
    LockOpen,
    RefreshCw,
    Settings,
    ShieldCheck,
  } from "@lucide/svelte";
  import { getCurrentWebview } from "@tauri-apps/api/webview";
  import Button from "$lib/components/Button.svelte";
  import JobDock from "$lib/components/JobDock.svelte";
  import Modal from "$lib/components/Modal.svelte";
  import RuntimeIcon from "$lib/components/RuntimeIcon.svelte";
  import Switch from "$lib/components/Switch.svelte";
  import Tip from "$lib/components/Tip.svelte";
  import Toasts from "$lib/components/Toasts.svelte";
  import { api, messageOf, onJob, onProject, onSettings } from "$lib/ipc";
  import { i18n, t, type MsgKey } from "$lib/i18n/store.svelte";
  import { app, type View } from "$lib/state.svelte";
  import AboutView from "$lib/views/AboutView.svelte";
  import AiView from "$lib/views/AiView.svelte";
  import DetailView from "$lib/views/DetailView.svelte";
  import EnvView from "$lib/views/EnvView.svelte";
  import FirstRun from "$lib/views/FirstRun.svelte";
  import HomeView from "$lib/views/HomeView.svelte";
  import MirrorView from "$lib/views/MirrorView.svelte";
  import ProjectView from "$lib/views/ProjectView.svelte";
  import SettingsView from "$lib/views/SettingsView.svelte";
  import ToolView from "$lib/views/ToolView.svelte";
  import { NAV_TOOLS, SITE_URL, TOOL_LABEL, siblings } from "$lib/utils";
  import type { ToolId } from "$lib/types";

  /** 顶部主入口：首页、AI 客户端，以及紧跟其后的项目锁——配完 AI 就直接锁项目。词条存 key，切语言才会跟着换。 */
  const primary: { view: View; key: MsgKey; icon: typeof Boxes }[] = [
    { view: "home", key: "nav.home", icon: House },
    { view: "ai", key: "nav.ai", icon: Bot },
    { view: "project", key: "nav.project", icon: FolderTree },
  ];

  /** Python 这一项带着 PyPy 分支，导航里做成常驻树形。 */
  const branchOf: Partial<Record<ToolId, { tool: ToolId; key: MsgKey }[]>> = {
    python: [{ tool: "python", key: "nav.branchStandard" }, { tool: "pypy", key: "nav.branchPyPy" }],
  };

  let branchOpen = $state(true);

  function isActive(navTool: ToolId): boolean {
    return app.view === "tool" && siblings(navTool).includes(app.tool);
  }

  /** 每个运行时都把自己当前的状态标出来，不留空行。 */
  type RowStatus = { text: string; tone: "ok" | "warn" | "bad" | "idle"; title: string };

  /**
   * 三档颜色只说三件事：绿=这个版本现在能跑，黄=记录还在但文件不全，红=这台电脑还没装。
   * 目录还没读出来时不猜，走 idle 灰档。细节全在悬浮气泡里，行内只留版本号和一颗豆豆。
   */
  function statusOf(tool: ToolId): RowStatus {
    const r = app.dirContext?.resolutions.find((x) => x.tool === tool);
    if (!r) return { text: "", tone: "idle", title: t("nav.booting") };
    if (!r.version)
      return { text: "", tone: "idle", title: `${t("tool.notInstalledWord")}：${t("home.noneInstalled")}` };
    if (!r.execPath) return { text: r.version, tone: "warn", title: `${r.version}：${t("nav.statusBroken")}` };
    return {
      text: r.version,
      tone: "ok",
      title: `${r.version}：${r.source === "globalDefault" ? t("common.globalDefault") : t("common.inUse")}`,
    };
  }

  const dotCls: Record<RowStatus["tone"], string> = {
    ok: "bg-success", warn: "bg-warning", bad: "bg-destructive", idle: "bg-muted-foreground/25",
  };

  let unsubs: (() => void)[] = [];

  /** 首屏只读一次数据：读成了界面正常走，读失败也要把原因摆在页面上，别让人对着黑屏猜。 */
  async function boot(): Promise<void> {
    app.bootError = null;
    try {
      app.startupLog = await api.startupLog();
    } catch {
      app.startupLog = "";
    }
    try {
      await app.start();
    } catch (e) {
      app.bootError = messageOf(e);
      app.fail(e, t("app.bootFailed"));
    }
  }

  $effect(() => {
    // 画出一帧就向守护报"活着"：首屏取数据可能要等网络，那份慢不该被当成起不来。
    requestAnimationFrame(() => void api.appReady().catch(() => {}));
    void boot();

    const media = window.matchMedia("(prefers-color-scheme: dark)");
    const onMedia = () => app.applyTheme();
    media.addEventListener("change", onMedia);

    unsubs = [];
    void onJob((ev) => app.onJob(ev)).then((u) => unsubs.push(u));
    void onSettings((s) => app.applySettings(s)).then((u) => unsubs.push(u));
    void onProject((dir) => void app.refreshDir(dir)).then((u) => unsubs.push(u));
    void getCurrentWebview()
      .onDragDropEvent((event) => {
        if (event.payload.type === "drop") void handleDrop(event.payload.paths);
      })
      .then((u) => unsubs.push(u));

    return () => {
      media.removeEventListener("change", onMedia);
      unsubs.forEach((u) => u());
    };
  });

  $effect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "," && (e.ctrlKey || e.metaKey) && !e.altKey && app.ready && !app.danger) {
        e.preventDefault();
        app.settingsOpen = !app.settingsOpen;
      }
      if (e.key === "Escape") {
        if (app.settingsOpen) app.closeSettings();
        else app.select(null);
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  });

  /** 套着浏览器内核不等于开了浏览器：右键菜单和刷新/查看源码一类快捷键在这里挡住，
   *  不然用户点右键看到的是"重新加载 / 另存为 / 检查"。 */
  $effect(() => {
    const noMenu = (e: Event) => e.preventDefault();
    const blockKey = (e: KeyboardEvent) => {
      const k = e.key.toLowerCase();
      const reload = k === "f5" || (e.ctrlKey && !e.altKey && k === "r");
      const browser = ["f12", "u", "j", "o", "p", "s", "g", "n", "w"].includes(k);
      const navigate = e.altKey && (k === "arrowleft" || k === "arrowright");
      if (reload || navigate || (browser && (e.ctrlKey || k.startsWith("f")))) e.preventDefault();
    };
    window.addEventListener("contextmenu", noMenu);
    window.addEventListener("keydown", blockKey);
    return () => {
      window.removeEventListener("contextmenu", noMenu);
      window.removeEventListener("keydown", blockKey);
    };
  });

  /** 托盘菜单和窗口标题在 webview 外面，读不到语言目录，只能把这里算好的语言送过去。 */
  $effect(() => {
    void api.setShellLang(i18n.locale).catch(() => {});
  });

  async function handleDrop(paths: string[]): Promise<void> {
    if (!paths.length) return;
    try {
      app.guesses = await api.precheckOffline(paths);
      app.view = "tool";
      app.toast(t("app.dropFound", { n: app.guesses.length }), "ok", 7000);
    } catch (err) {
      app.fail(err, t("app.dropUnreadable"));
    }
  }
</script>

{#snippet statusDot(tool: ToolId)}
  {@const st = statusOf(tool)}
  {#if st.text}
    <span class="mono shrink-0 text-[11px] leading-none text-muted-foreground">{st.text}</span>
  {/if}
  <Tip text={st.title} class="shrink-0">
    <span class="block size-1.5 rounded-full {dotCls[st.tone]}"></span>
  </Tip>
{/snippet}

<WindowBar />
{#if app.bootError}
  <div class="flex h-dvh items-center justify-center bg-background px-6">
    <div class="w-full max-w-2xl rounded-xl border border-border bg-card p-6 elevate">
      <div class="flex items-start gap-3">
        <ShieldCheck class="mt-0.5 size-5 shrink-0 text-destructive" />
        <div class="min-w-0 flex-1">
          <h1 class="text-base font-semibold">{t("app.bootFailedTitle")}</h1>
          <p class="mt-1 text-xs leading-relaxed text-muted-foreground">{t("app.bootFailedDesc")}</p>
          <p class="mono mt-3 break-all rounded-md border border-border bg-detail p-3 text-xs leading-relaxed">
            {app.bootError}
          </p>
          {#if app.startupLog}
            <p class="mono mt-2 truncate text-xs text-muted-foreground" title={app.startupLog}>
              {t("app.logAtLine", { path: app.startupLog })}
            </p>
          {/if}
          <div class="mt-4 flex flex-wrap gap-2">
            <Button size="sm" onclick={() => void boot()}>
              <RefreshCw class="size-3.5" />
              {t("app.retry")}
            </Button>
            {#if app.startupLog}
              <Button size="sm" variant="outline" onclick={() => void app.revealPath(app.startupLog)}>
                <FolderTree class="size-3.5" />
                {t("app.openLog")}
              </Button>
            {/if}
            <Button size="sm" variant="ghost" onclick={() => void app.openLink(SITE_URL)}>
              <Globe class="size-3.5" />
              {t("app.getHelp")}
            </Button>
          </div>
        </div>
      </div>
    </div>
  </div>
{:else if !app.boot}
  <div class="flex h-dvh items-center justify-center gap-2 bg-background text-sm text-muted-foreground">
    <RefreshCw class="size-4 animate-spin" />
    {t("nav.booting")}
  </div>
{:else if app.boot.needsSetup}
  <FirstRun />
{:else}
  <div class="desktop-root flex h-dvh flex-col text-foreground">
    <!-- 设置页盖上来时，侧边栏连同内容一起退出可访问性和点击。 -->
    <div class="app-shell flex min-h-0 flex-1" inert={app.settingsOpen}>
      <nav class="app-sidebar flex shrink-0 flex-col bg-sidebar" aria-label={t("nav.workspace")}>
        <button type="button" class="sidebar-brand" title={t("window.website")} aria-label={t("window.website")} onclick={() => void app.openLink(SITE_URL)}>
          <span class="brand-mark"><img src="/logo.svg" alt="" /></span>
          <span class="sidebar-brand-name">EnvSwitch<small>灵匣 · Developer tools</small></span>
        </button>

        <div class="sidebar-scroll">
          <p class="nav-label mb-2 px-2.5">{t("nav.workspace")}</p>
          <ul class="workspace-nav space-y-1">
            {#each primary as item (item.view)}
              {@const Icon = item.icon}
              <li>
                <button type="button"
                  class="nav-row flex w-full items-center gap-2.5 rounded-md px-2.5 py-2 text-sm transition-colors hover:bg-muted {app.view === item.view ? 'font-medium' : 'text-muted-foreground hover:text-foreground'}"
                  aria-current={app.view === item.view ? "page" : undefined}
                  onclick={() => app.setView(item.view)}>
                  <Icon class="size-4 shrink-0" strokeWidth={1.7} />
                  <span class="min-w-0 flex-1 truncate text-left">{t(item.key)}</span>
                </button>
              </li>
            {/each}
          </ul>

          <p class="nav-label mt-7 mb-2 px-2.5">{t("nav.runtime")}</p>
          <ul class="runtime-nav space-y-1">
            {#each NAV_TOOLS as tool (tool)}
              {@const active = isActive(tool)}
              {@const branches = branchOf[tool]}
              <li>
                <button type="button"
                  class="nav-row flex w-full items-center gap-2.5 rounded-md px-2.5 py-2 text-sm transition-colors hover:bg-muted {active ? 'font-medium' : 'text-muted-foreground hover:text-foreground'}"
                  aria-current={active ? "page" : undefined}
                  onclick={() => { if (branches) branchOpen = true; void app.setTool(tool); }}>
                  <RuntimeIcon {tool} class="size-4 shrink-0 opacity-80" />
                  <span class="min-w-0 flex-1 truncate text-left">{TOOL_LABEL[tool]}</span>
                  {@render statusDot(tool)}
                  {#if branches}<ChevronDown class="size-3 shrink-0 {active && branchOpen ? '' : '-rotate-90'}" aria-hidden="true" />{/if}
                </button>
                {#if branches && active && branchOpen}
                  <ul class="my-1 ml-[17px] space-y-0.5 border-l border-border pl-3" aria-label={t("nav.branchExpand")}>
                    {#each branches as b (b.tool)}
                      {@const on = app.view === "tool" && app.tool === b.tool}
                      <li>
                        <button type="button"
                          class="flex w-full items-center gap-2 rounded-md px-2 py-1.5 text-xs transition-colors {on ? 'bg-accent text-accent-foreground' : 'text-muted-foreground hover:bg-muted hover:text-foreground'}"
                          onclick={() => void app.setTool(b.tool)}>
                          <span class="min-w-0 flex-1 truncate text-left">{t(b.key)}</span>
                        </button>
                      </li>
                    {/each}
                  </ul>
                {/if}
              </li>
            {/each}
          </ul>

          <div class="manager-nav">
          <button type="button" class="nav-row flex w-full items-center gap-2.5 rounded-md px-2.5 py-2 text-sm text-muted-foreground transition-colors hover:bg-muted hover:text-foreground"
            aria-current={app.view === "env" ? "page" : undefined} onclick={() => app.setView("env")}>
            <Terminal class="size-4" strokeWidth={1.7} />{t("nav.env")}
          </button>
          <button type="button" class="nav-row flex w-full items-center gap-2.5 rounded-md px-2.5 py-2 text-sm text-muted-foreground transition-colors hover:bg-muted hover:text-foreground"
            aria-current={app.view === "mirror" ? "page" : undefined} onclick={() => app.setView("mirror")}>
            <Globe class="size-4" strokeWidth={1.7} />{t("nav.mirror")}
          </button>
          </div>
        </div>

        <div class="sidebar-footer shrink-0">
          <div class="lock-row flex items-center gap-2" title={app.settings?.lockGlobal ? t("nav.lockedOn") : t("nav.lockedOff")}>
            {#if app.settings?.lockGlobal}<Lock class="size-3.5 shrink-0 text-muted-foreground" />
            {:else}<LockOpen class="size-3.5 shrink-0 text-muted-foreground" />{/if}
            <span class="lock-label min-w-0 flex-1 text-xs text-muted-foreground">{t("nav.lockSwitch")}</span>
            <Switch label={t("nav.lockSwitch")} checked={app.settings?.lockGlobal ?? false} onchange={(next) => void app.patch({ lockGlobal: next })} />
          </div>
          <button type="button" class="settings-link flex w-full items-center gap-2 rounded-md py-1 text-sm text-muted-foreground hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
            aria-haspopup="dialog" aria-expanded={app.settingsOpen} onclick={() => app.settingsOpen = true}>
            <Settings class="size-4" strokeWidth={1.7} /><span class="flex-1 text-left">{t("nav.settings")}</span><kbd>Ctrl ,</kbd>
          </button>
        </div>
      </nav>

      <main class="app-main relative flex min-w-0 flex-1 flex-col">
        <div class="min-h-0 flex-1">
          {#if app.view === "home"}
            <HomeView />
          {:else if app.view === "tool"}
            <ToolView />
          {:else if app.view === "project"}
            <ProjectView />
          {:else if app.view === "mirror"}
            <MirrorView />
          {:else if app.view === "env"}
            <EnvView />
          {:else if app.view === "ai"}
            <AiView />
          {:else}
            <AboutView />
          {/if}
        </div>

        <JobDock />
      </main>

      {#if app.selected && app.view === "tool"}
        <div class="detail-shell shrink-0">
          <DetailView />
        </div>
      {/if}
    </div>

    {#if app.settingsOpen}
      <!-- 设置整页盖住整个窗口，侧边栏也在它底下：窗口本来就不大，还留一条栏只会有两块地方要读。
           底下那层整套退出可访问性和点击，模态框不该让人穿过去点中后面的按钮。 -->
      <div
        class="settings-window fixed z-30 flex flex-col"
        role="dialog"
        aria-modal="true"
        aria-label={t("nav.settings")}
      >
        <header class="flex shrink-0 items-center gap-2 border-b border-border px-4 py-2.5">
          <Button variant="ghost" size="sm" onclick={() => app.closeSettings()}>
            <ArrowLeft class="size-4" />
            {t("common.back")}
          </Button>
          <h2 class="text-sm font-semibold">{t("nav.settings")}</h2>
        </header>
        <div class="flex min-h-0 flex-1">
          <SettingsView />
        </div>
      </div>
    {/if}
  </div>
{/if}

{#if app.danger}
  {@const ask = app.danger}
  <Modal title={ask.title} tone={ask.tone ?? "danger"} onclose={() => app.answerDanger(false)}>
    <p class="whitespace-pre-line text-muted-foreground">{ask.body}</p>
    {#snippet footer()}
      <Button size="sm" variant="outline" onclick={() => app.answerDanger(false)}>{t("common.cancel")}</Button>
      <Button size="sm" variant={ask.tone === "default" ? "default" : "danger"} onclick={() => app.answerDanger(true)}>{ask.confirm}</Button>
    {/snippet}
  </Modal>
{/if}

<Toasts />
