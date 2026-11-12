<script lang="ts">
  import { onMount } from "svelte";
  import { ArrowUpRight, CircleCheck, FolderTree, HardDrive, Radar, RefreshCw, ShieldCheck, Terminal, Wand } from "@lucide/svelte";
  import Button from "$lib/components/Button.svelte";
  import RepoActions from "$lib/components/RepoActions.svelte";
  import ExternalRuntimes from "$lib/components/ExternalRuntimes.svelte";
  import { api } from "$lib/ipc";
  import { t } from "$lib/i18n/store.svelte";
  import { app } from "$lib/state.svelte";
  import { ALL_TOOLS, TOOL_LABEL, SITE_URL } from "$lib/utils";
  import type { InstalledPage, ToolId } from "$lib/types";

  type Tone = "ok" | "warn" | "bad";

  interface Check {
    id: string;
    title: string;
    tone: Tone;
    /** 一句话说清现在是什么状态，收在明细里，需要动手的行才留在页面上。 */
    detail: string;
    /** 能自动修的就带上动作，一键优化会按顺序执行。 */
    fix?: () => Promise<void>;
    fixLabel?: string;
  }

  /** 检测结果缓存：下次进首页直接显示上次的现场，不必再扫一遍机器。 */
  const CACHE_KEY = "envswitch.home.check.v2";

  interface Cache {
    at: number;
    conflicts: number;
    perTool: Record<ToolId, InstalledPage>;
  }

  let perTool = $state<Record<ToolId, InstalledPage> | null>(null);
  let checking = $state(false);
  let optimizing = $state(false);
  let progress = $state(0);
  let checkedAt = $state<number | null>(null);
  let cachedConflicts = $state(0);

  /** 四段进度只是给等待一个着落，不代表真按这四步扫。 */
  const phases = $derived([
    t("home.phasePath"),
    t("home.phaseShim"),
    t("home.phaseVersion"),
    t("home.phaseSum"),
  ]);
  const phase = $derived(phases[Math.min(phases.length - 1, Math.floor((progress / 100) * phases.length))]);

  function readCache(): Cache | null {
    try {
      const raw = localStorage.getItem(CACHE_KEY);
      if (!raw) return null;
      const c = JSON.parse(raw) as Cache;
      if (typeof c?.at !== "number" || !c.perTool) return null;
      return c;
    } catch {
      return null;
    }
  }

  function writeCache(): void {
    if (!perTool) return;
    const cache: Cache = {
      at: Date.now(),
      conflicts: app.scan?.conflicts.length ?? 0,
      perTool,
    };
    try {
      localStorage.setItem(CACHE_KEY, JSON.stringify(cache));
      checkedAt = cache.at;
    } catch {
      // 缓存写不进去只是下次要重扫，不该拦住任何人。
    }
  }

  const cached = readCache();
  if (cached) {
    perTool = cached.perTool;
    cachedConflicts = cached.conflicts;
    checkedAt = cached.at;
  }

  onMount(() => { if (!app.scan) void app.refreshEnv(); });

  /** 进度按完成的本地检查推进，不添加人为等待。 */
  async function scan(): Promise<boolean> {
    let completed = 0;
    const advance = () => { progress = Math.round(++completed / (ALL_TOOLS.length + 2) * 100); };
    try {
      const [envOk] = await Promise.all([app.refreshEnv().then((ok) => { advance(); return ok; }), app.refreshDir(app.dirContext?.dir ?? null).then(advance)]);
      if (!envOk) return false;
      const pages = await Promise.all(ALL_TOOLS.map(async (tool) => {
        const page = await api.installedVersions(tool, app.dirContext?.dir ?? null);
        advance();
        return page;
      }));
      perTool = Object.fromEntries(ALL_TOOLS.map((tool, i) => [tool, pages[i]])) as Record<ToolId, InstalledPage>;
      return true;
    } catch (e) {
      app.fail(e, t("msg.envScanFailed"));
      return false;
    }
  }

  async function startCheck(): Promise<void> {
    if (checking || optimizing) return;
    checking = true;
    progress = 0;
    try { if (await scan()) writeCache(); }
    finally { checking = false; }
  }

  /** 完整体检由按钮触发；后台只读已有环境，及时展示接管入口。 */
  const stage = $derived(checking ? "checking" : perTool && checkedAt ? "done" : "idle");

  const scope = $derived(app.settings?.pathScope ?? "user");
  const activePath = $derived(app.paths.find((p) => p.scope === scope) ?? app.paths[0] ?? null);
  const requiredShims = $derived((app.boot?.shims ?? []).filter((s) => s.tool && (perTool?.[s.tool]?.versions.length ?? 0) > 0));
  const missingShims = $derived(requiredShims.filter((s) => !s.exists));
  const brokenRecords = $derived(
    Object.entries(perTool ?? {}).flatMap(([tool, page]) =>
      page.versions.filter((v) => !v.usable).map((v) => ({ tool: tool as ToolId, version: v.version })),
    ),
  );
  const hooksToEnable = $derived(app.hooks.filter((h) => h.detected && !h.installed));
  const conflictCount = $derived(app.scan?.conflicts.length ?? cachedConflicts);
  const sep = $derived(t("home.listSep"));

  const checks = $derived(stage === "done" ? checksList() : []);

  function checksList(): Check[] {
    const out: Check[] = [];
    const root = app.boot?.layout?.root ?? "";
    if (root && /^[cC]:/.test(root)) {
      out.push({
        id: "root",
        title: t("home.check.rootTitle"),
        tone: "warn",
        detail: t("home.check.rootDetail", { dir: root }),
      });
    } else {
      out.push({
        id: "root-ok",
        title: t("home.check.rootOkTitle"),
        tone: "ok",
        detail: root ? t("home.check.rootOkDetail", { dir: root }) : t("home.check.rootUnsetDetail"),
      });
    }

    if (!activePath) {
      out.push({
        id: "path-read",
        title: t("home.check.readFailTitle"),
        tone: "warn",
        detail: t("home.check.readFailDetail"),
      });
    } else if (!activePath.installed) {
      out.push({
        id: "path",
        title: t("home.check.pathTitle"),
        tone: "bad",
        detail: t("home.check.pathDetail", { scope: scope === "system" ? t("home.scopeWhole") : t("home.scopeUser") }),
        fixLabel: t("home.check.pathFix"),
        fix: async () => { if (!await app.ensureForwarding()) throw new Error(t("home.configureHint")); await api.pathApply(scope); },
      });
    } else if (app.settings?.shimPriority && !activePath.isFirst) {
      out.push({
        id: "order",
        title: t("home.check.orderTitle"),
        tone: "warn",
        detail: t("home.check.orderDetail", { pos: (activePath.position ?? 0) + 1 }),
        fixLabel: t("home.check.orderFix"),
        fix: async () => {
          await api.pathReorder(scope);
          app.paths = await api.pathStatus();
          app.toast(t("home.check.orderDone"), "ok");
        },
      });
    } else {
      out.push({
        id: "path-ok",
        title: t("home.check.pathOkTitle"),
        tone: "ok",
        detail: t("home.check.pathOkDetail", {
          where: scope === "system" ? t("home.pathOfSystem") : t("home.pathOfUser"),
          first: activePath.isFirst ? t("home.firstBonus") : "",
        }),
      });
    }

    if ((app.boot?.shims.length ?? 0) === 0) {
      out.push({
        id: "shim-none",
        title: t("home.check.shimNoneTitle"),
        tone: "bad",
        detail: t("home.check.shimNoneDetail"),
        fixLabel: t("home.check.shimNoneFix"),
        fix: async () => { const shims = await api.shimSync(); if (app.boot) app.boot.shims = shims; },
      });
    } else if (missingShims.length) {
      out.push({
        id: "shim",
        title: t("home.check.shimMissingTitle", { n: missingShims.length }),
        tone: "warn",
        detail: t("home.check.shimMissingDetail", { names: missingShims.map((s) => s.name).join(sep) }),
        fixLabel: t("env.shimSync"),
        fix: async () => { const shims = await api.shimSync(); if (app.boot) app.boot.shims = shims; },
      });
    } else {
      out.push({
        id: "shim-ok",
        title: t("home.check.shimOkTitle", { n: requiredShims.length }),
        tone: "ok",
        detail: t("home.check.shimOkDetail"),
      });
    }

    if (hooksToEnable.length) {
      out.push({
        id: "hook",
        title: t("home.check.hookTitle"),
        tone: "warn",
        detail: t("home.check.hookDetail", { names: hooksToEnable.map((h) => h.label).join(sep) }),
        fixLabel: t("home.check.hookFix"),
        fix: async () => {
          if (!await app.ensureForwarding()) throw new Error(t("home.configureHint"));
          for (const h of hooksToEnable) await api.hookSet(h.shell, true);
        },
      });
    } else if (app.hooks.some((h) => h.installed)) {
      out.push({
        id: "hook-ok",
        title: t("home.check.hookOkTitle"),
        tone: "ok",
        detail: t("home.check.hookOkDetail", { names: app.hooks.filter((h) => h.installed).map((h) => h.label).join(sep) }),
      });
    } else {
      out.push({
        id: "hook-none",
        title: t("home.check.hookNoneTitle"),
        tone: "ok",
        detail: t("home.check.hookNoneDetail"),
      });
    }

    if (brokenRecords.length) {
      out.push({
        id: "broken",
        title: t("home.check.brokenTitle", { n: brokenRecords.length }),
        tone: "bad",
        detail: t("home.check.brokenDetail", {
          names: brokenRecords.map((b) => `${TOOL_LABEL[b.tool]} ${b.version}`).join(sep),
        }),
        fixLabel: t("home.check.brokenFix"),

      });
    }

    if (conflictCount) {
      out.push({
        id: "conflict",
        title: t("home.check.conflictTitle", { n: conflictCount }),
        tone: "warn",
        detail: t("home.check.conflictDetail"),
      });
    } else {
      out.push({
        id: "conflict-ok",
        title: t("home.check.conflictOkTitle"),
        tone: "ok",
        detail: t("home.check.conflictOkDetail"),
      });
    }

    return out;
  }

  const fixable = $derived(checks.filter((c) => c.fix));
  const problems = $derived(checks.filter((c) => c.tone !== "ok"));
  const passed = $derived(checks.filter((c) => c.tone === "ok"));

  const timeText = $derived(checkedAt ? new Date(checkedAt).toLocaleString() : "");
  async function optimize(): Promise<void> {
    if (!fixable.length) {
      app.toast(t("home.optimizeAllDone"), "ok");
      return;
    }
    optimizing = true;
    try {
      if (!await app.ensureForwarding()) { optimizing = false; return; }
    } catch (e) { optimizing = false; app.fail(e); return; }
    const done: string[] = [];
    const failed: string[] = [];
    for (const c of fixable) {
      try {
        await c.fix?.();
        done.push(c.fixLabel ?? c.title);
      } catch (e) {
        failed.push(c.title);
        app.fail(e, t("home.stepFailed", { title: c.title }));
      }
    }
    optimizing = false;
    await startCheck();
    if (failed.length) {
      app.toast(t("home.optimizePartial", { n: done.length, m: failed.length }), "warn", 8000);
    } else {
      app.toast(t("home.optimizeDone", { n: done.length, names: done.join(sep) }), "ok", 8000);
    }
  }
</script>

<div class="home-view flex h-full flex-col overflow-y-auto">
  <header class="home-header"><div><span class="header-kicker">WORKSPACE</span><h2>{t("home.workspaceTitle")}</h2></div><span class="mono header-version">v{app.boot?.version}</span></header>
  <div class="home-content">
    <section class="welcome-banner">
      <button type="button" class="logo-scene" aria-label={t("window.website")} title={t("window.website")} onclick={() => void app.openLink(SITE_URL)}>
        <span class="logo-grid"></span>
        <img class="hero-logo" src="/logo.svg" alt="灵匣 EnvSwitch" />
        <svg class="logo-sparkles" viewBox="0 0 240 200" fill="none" aria-hidden="true"><path d="M37 39v12m-6-6h12M204 127v10m-5-5h10" stroke="currentColor" stroke-width="1.5"/><path d="m191 37 4 7-4 7-4-7Z" fill="currentColor"/><circle cx="48" cy="150" r="3" stroke="currentColor"/></svg>
        <span class="logo-caption">BUILD YOUR NEXT IDEA</span>
      </button>
      <div class="welcome-copy"><span class="welcome-eyebrow">OPEN SOURCE · MADE FOR DEVELOPERS</span><RepoActions /></div>
    </section>
    <section class="scan-card glass-card overflow-hidden" aria-label={t("home.scanTitle")}>
      <div class="flex flex-wrap items-center gap-3 px-5 py-4">
        <span class="scan-icon"><ShieldCheck class="size-5" strokeWidth={1.6} /></span>
        <div class="min-w-0 flex-1">
          <h2 class="text-sm font-semibold">{t("home.scanTitle")}</h2>
          <p class="mt-1 text-xs text-muted-foreground">{stage === "done" ? t("home.checkedAt", { time: timeText }) : t("home.detectDesc")}</p>
        </div>
        {#if stage === "done"}
          <span class="text-xs {problems.length ? 'text-warning' : 'text-success'}">{problems.length ? t("home.todoPanel", { n: problems.length }) : t("home.healthy")}</span>
        {/if}
        <Button variant="outline" disabled={optimizing || checking || stage !== "done" || !fixable.length} title={stage !== "done" ? t("home.configureHint") : undefined} onclick={() => void optimize()}><Wand class="size-4" />{optimizing ? t("home.optimizing") : t("home.configure")}</Button>
        <Button variant="default" disabled={checking || optimizing} onclick={() => void startCheck()}>
          {#if checking}<RefreshCw class="size-3.5 animate-spin" />{:else}<Radar class="size-3.5" />{/if}
          {checking ? t("home.scanning") : stage === "done" ? t("home.recheck") : t("home.detectBtn")}
        </Button>
      </div>
      {#if checking}
        <div class="border-t border-border px-5 py-4">
          <div class="h-1 overflow-hidden rounded-full bg-muted" role="progressbar" aria-label={t("home.scanning")} aria-valuemin={0} aria-valuemax={100} aria-valuenow={progress}>
            <span class="block h-full bg-primary transition-[width] duration-150" style="width:{progress}%"></span>
          </div>
          <p class="mt-2 flex justify-between text-xs text-muted-foreground"><span>{phase}</span><span class="mono">{progress}%</span></p>
        </div>
      {:else if stage === "done"}
        {#if problems.length}
          <ul class="divide-y divide-border border-t border-border">
            {#each problems as c (c.id)}
              <li class="flex flex-wrap items-center gap-3 px-5 py-3.5">
                <span class="size-1.5 shrink-0 rounded-full {c.tone === 'bad' ? 'bg-destructive' : 'bg-warning'}"></span>
                <div class="min-w-0 flex-1"><p class="text-sm">{c.title}</p><p class="mt-1 text-xs leading-relaxed text-muted-foreground">{c.detail}</p></div>
                {#if c.fix}<Button size="sm" variant="outline" disabled={optimizing} onclick={async () => { try { await c.fix?.(); await startCheck(); } catch (e) { app.fail(e); } }}>{c.fixLabel ?? t("home.fixThis")}</Button>{/if}
              </li>
            {/each}
          </ul>
        {/if}
        {#if passed.length}
          <details class="border-t border-border px-5 py-3">
            <summary class="cursor-pointer text-xs text-muted-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring">{t("home.donePanel", { n: passed.length })}</summary>
            <ul class="mt-3 space-y-3">
              {#each passed as c (c.id)}
                <li class="flex items-start gap-2.5"><CircleCheck class="mt-0.5 size-3.5 shrink-0 text-success" /><div><p class="text-xs">{c.title}</p><p class="mt-1 text-xs text-muted-foreground">{c.detail}</p></div></li>
              {/each}
            </ul>
          </details>
        {/if}
      {:else}
        <div class="flex flex-wrap gap-x-6 gap-y-2 border-t border-border bg-white/10 px-5 py-3 text-xs text-muted-foreground">
          <span>PATH</span><span>{t("home.groupEntry")}</span><span>{t("home.groupHook")}</span><span>{t("home.groupRecord")}</span>
        </div>
      {/if}
    </section>
    <ExternalRuntimes />
    <div class="quick-grid">
      <button type="button" class="quick-card" onclick={() => app.setView("project")}><span class="quick-icon"><FolderTree size={20} strokeWidth={1.5} /></span><span><strong>{t("home.projectQuickTitle")}</strong><small>{t("home.projectQuickHint")}</small></span><ArrowUpRight size={17} /></button>
      <button type="button" class="quick-card" onclick={() => app.setView("env")}><span class="quick-icon"><Terminal size={20} strokeWidth={1.5} /></span><span><strong>{t("home.terminalQuickTitle")}</strong><small>{t(activePath?.installed ? "home.pathReady" : "home.pathPending")}</small></span><ArrowUpRight size={17} /></button>
    </div>
    <div class="workspace-location"><HardDrive size={14} /><span>{t("nav.dataDir")}</span><button type="button" class="mono" title={app.boot?.layout?.root} onclick={() => void app.revealPath(app.boot?.layout?.root ?? null)}>{app.boot?.layout?.root}</button><ShieldCheck size={14} /></div>
  </div>
</div>

<style>
  .home-header { min-height: 77px !important; display: flex; align-items: center; padding: 12px 32px; gap: 15px; border: 0; }
  .home-header > div { display: flex; align-items: center; gap: 12px; } .home-header h2 { font-size: 15px !important; font-weight: 600; }
  .header-kicker { font-size: 9px; letter-spacing: 1.6px; color: var(--muted-foreground); border-right: 1px solid var(--border); padding-right: 12px; }
  .header-version { margin-left: auto; font-size: 11px; color: var(--muted-foreground); }
  .home-content { width: 100%; max-width: 1300px; margin: 0 auto; padding: 0 32px 25px; display: flex; flex-direction: column; gap: 18px; }
  .welcome-banner { display: grid; grid-template-columns: 210px minmax(0,1fr); min-height: 230px; border-radius: 22px; border: 1px solid var(--glass-stroke); background: linear-gradient(115deg,#ffffffb5,#e7efff8c 60%,#ede9ffa3); position: relative; overflow: hidden; }
  .welcome-banner::after { content: ''; pointer-events: none; position: absolute; width: 240px; height: 240px; border-radius: 50%; right: -120px; top: -130px; border: 35px solid #ffffff30; }
  .welcome-copy { min-width: 0; display: flex; flex-direction: column; justify-content: center; padding: 26px 30px 26px 12px; }
  .welcome-eyebrow { font-size: 9px; letter-spacing: 1.8px; color: #7990b4; margin-bottom: 14px; }
  .logo-scene { position: relative; display: grid; align-content: center; justify-items: center; isolation: isolate; }
  .logo-grid { position: absolute; inset: 24px 10px; opacity: .35; background-image: linear-gradient(#a7bbdf40 1px,transparent 1px),linear-gradient(90deg,#a7bbdf40 1px,transparent 1px); background-size: 24px 24px; mask-image: radial-gradient(ellipse,#000,transparent 70%); z-index: -1; }
  .hero-logo { width: 160px; height: 160px; object-fit: contain; animation: logo-float 6s ease-in-out infinite; filter: drop-shadow(0 12px 12px #6c88b91a); }
  .logo-sparkles { position: absolute; inset: 10px 0; width: 100%; height: 180px; color: #89a7d9; animation: sparkle 5s ease-in-out infinite; }
  .logo-caption { font-size: 8px; letter-spacing: 1.8px; color: #8297b7; margin-top: 6px; }
  .glass-card { border: 1px solid var(--glass-stroke); border-radius: 18px; background: var(--card); box-shadow: var(--glass-shadow); }
  .scan-card { border-top: 2px solid #95b5f0; }
  .scan-icon { display: grid; place-items: center; width: 42px; height: 42px; border-radius: 13px; background: var(--accent); color: var(--primary); }
  .scan-card :global(.ui-button) { min-height: 36px; }
  .quick-grid { display: grid; grid-template-columns: 1fr 1fr; gap: 16px; }
  .quick-card { display: flex; text-align: left; align-items: center; gap: 13px; border: 1px solid var(--glass-stroke); border-radius: 16px; padding: 20px; background: var(--card); transition: transform .2s, box-shadow .2s; }
  .quick-card:hover { transform: translateY(-2px); box-shadow: var(--glass-shadow); } .quick-icon { color: var(--primary); display: flex; } .quick-card > span:nth-child(2) { flex: 1; min-width: 0; }
  .quick-card strong { display: block; font-size: 13px; font-weight: 550; } .quick-card small { display: block; margin-top: 6px; font-size: 11px; color: var(--muted-foreground); } .quick-card > :global(svg) { color: var(--muted-foreground); }
  .workspace-location { display: flex; gap: 8px; align-items: center; color: var(--muted-foreground); font-size: 10px; padding: 3px 0; } .workspace-location button { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; text-align: left; } .workspace-location > :global(svg:last-child) { margin-left: auto; flex-shrink: 0; }
  :global(.dark) .welcome-banner { background: linear-gradient(115deg,#263a55c9,#283758a8,#333959a3); } :global(.dark) .hero-logo { filter: invert(.85) brightness(1.7) drop-shadow(0 12px 12px #0002); }
  @keyframes logo-float { 0%,100% { transform: translateY(0) rotate(-2deg); } 50% { transform: translateY(-7px) rotate(2deg); } }
  @keyframes sparkle { 0%,100% { opacity: .35; transform: translateY(0); } 50% { opacity: .85; transform: translateY(-4px); } }
  @media(max-width:1050px) { .home-content { padding: 0 24px 20px; gap: 15px; } .home-header { padding-inline: 24px; } .welcome-banner { grid-template-columns: 170px minmax(0,1fr); } .hero-logo { width: 140px; height: 140px; } .welcome-copy { padding-right: 22px; } }
  @media(prefers-reduced-motion:reduce) { .hero-logo,.logo-sparkles { animation: none; } }
</style>
