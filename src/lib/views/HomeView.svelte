<script lang="ts">
  import {
    Boxes,
    CircleCheck,
    Gauge,
    Radar,
    RefreshCw,
    Wand,
    Wrench,
  } from "@lucide/svelte";
  import Badge from "$lib/components/Badge.svelte";
  import Button from "$lib/components/Button.svelte";
  import Counter from "$lib/components/Counter.svelte";
  import Hint from "$lib/components/Hint.svelte";
  import RepoActions from "$lib/components/RepoActions.svelte";
  import RuntimeIcon from "$lib/components/RuntimeIcon.svelte";
  import Section from "$lib/components/Section.svelte";
  import { api } from "$lib/ipc";
  import { t, type MsgKey } from "$lib/i18n/store.svelte";
  import { app } from "$lib/state.svelte";
  import { ALL_TOOLS, TOOL_LABEL, humanBytes } from "$lib/utils";
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
  const CACHE_KEY = "envswitch.home.check.v1";

  interface Cache {
    at: number;
    conflicts: number;
    perTool: Record<ToolId, InstalledPage>;
  }

  /** 进度条最短走这么久：真活往往一秒内干完，一闪而过让人以为没在检。 */
  const MIN_MS = 2400;

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

  /** 真扫描：PATH、终端集成、每个工具的已装版本。 */
  async function scan(): Promise<void> {
    try {
      await Promise.all([app.refreshEnv(), app.refreshDir(app.dirContext?.dir ?? null)]);
      const dir = app.dirContext?.dir ?? null;
      const pages = await Promise.all(ALL_TOOLS.map((tool) => api.installedVersions(tool, dir)));
      perTool = Object.fromEntries(ALL_TOOLS.map((tool, i) => [tool, pages[i]])) as Record<ToolId, InstalledPage>;
    } catch (e) {
      app.fail(e, t("msg.envScanFailed"));
    }
  }

  /** 把进度推到 99% 等真活干完；剩下的 1% 和收尾一起给。 */
  function runProgress(): Promise<void> {
    progress = 0;
    const started = Date.now();
    return new Promise((resolve) => {
      const step = () => {
        const el = Date.now() - started;
        progress = Math.min(99, Math.round((el / MIN_MS) * 100));
        if (el >= MIN_MS) resolve();
        else requestAnimationFrame(step);
      };
      requestAnimationFrame(step);
    });
  }

  async function startCheck(): Promise<void> {
    if (checking) return;
    checking = true;
    const work = scan();
    const anim = runProgress();
    await Promise.all([work, anim]);
    progress = 100;
    // 100% 停半拍再切结果页，不然进度条永远看不到终点。
    await new Promise((r) => setTimeout(r, 320));
    checking = false;
    writeCache();
  }

  /** 只在点按钮时扫描；进页面读缓存，不偷偷体检。 */
  const stage = $derived(checking ? "checking" : perTool ? "done" : "idle");

  const scope = $derived(app.settings?.pathScope ?? "user");
  const activePath = $derived(app.paths.find((p) => p.scope === scope) ?? app.paths[0] ?? null);
  const missingShims = $derived((app.boot?.shims ?? []).filter((s) => !s.exists));
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
        fix: () => app.pathApply(scope),
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
        fix: () => app.shimSync(),
      });
    } else if (missingShims.length) {
      out.push({
        id: "shim",
        title: t("home.check.shimMissingTitle", { n: missingShims.length }),
        tone: "warn",
        detail: t("home.check.shimMissingDetail", { names: missingShims.map((s) => s.name).join(sep) }),
        fixLabel: t("env.shimSync"),
        fix: () => app.shimSync(),
      });
    } else {
      out.push({
        id: "shim-ok",
        title: t("home.check.shimOkTitle", { n: app.boot?.shims.length ?? 0 }),
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
          for (const h of hooksToEnable) await app.hookToggle(h.shell, true);
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
        fix: async () => {
          for (const b of brokenRecords) await app.uninstall(b.version, false, b.tool);
        },
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

  /** 每条体检项归到哪一组：大盘按组给结论，明细列表也靠它标行。 */
  const groupOf: Record<string, MsgKey> = {
    root: "home.groupStorage",
    "root-ok": "home.groupStorage",
    "path-read": "home.groupEntry",
    path: "home.groupEntry",
    order: "home.groupEntry",
    "path-ok": "home.groupEntry",
    "shim-none": "home.groupEntry",
    shim: "home.groupEntry",
    "shim-ok": "home.groupEntry",
    hook: "home.groupHook",
    "hook-ok": "home.groupHook",
    "hook-none": "home.groupHook",
    broken: "home.groupRecord",
    conflict: "home.groupRecord",
    "conflict-ok": "home.groupRecord",
  };
  const groupOrder: MsgKey[] = [
    "home.groupStorage",
    "home.groupEntry",
    "home.groupHook",
    "home.groupRecord",
  ];

  const groups = $derived(
    groupOrder
      .map((key) => {
        const items = checks.filter((c) => groupOf[c.id] === key);
        return { key, total: items.length, done: items.filter((c) => c.tone === "ok").length };
      })
      .filter((g) => g.total),
  );

  const okCount = $derived(passed.length);
  const score = $derived(checks.length ? Math.round((okCount / checks.length) * 100) : 100);
  /** 明细列表每行都标出属于哪一组，扫的时候一眼能对上大盘里的四块。 */
  function groupLabel(id: string): MsgKey {
    return groupOf[id] ?? "home.groupRecord";
  }
  /** 一圈的周长，配合 stroke-dashoffset 把进度画成弧。 */
  const RING = 2 * Math.PI * 52;
  /** 这一圈只报进度，不报警：没配满就是蓝，配满了转绿。 */
  const ringCls = $derived(score === 100 ? "stroke-success" : "stroke-primary");
  const scoreLine = $derived(problems.length ? t("home.optimizeLeft", { n: problems.length }) : t("home.healthy"));
  /** 总分下面那句解释收进气泡，页面只留结论和按钮。 */
  const scoreNote = $derived(
    fixable.length ? t("home.fixableNote", { n: fixable.length }) : problems.length ? t("home.manualNote") : t("home.allGoodNote"),
  );

  /** 结果页刚换上来时圆环从 0 转到得分，不然它一挂载就是终态，没有"转上去"这一下。 */
  let ringIn = $state(false);
  $effect(() => {
    if (stage === "done") {
      ringIn = false;
      const id = requestAnimationFrame(() => (ringIn = true));
      return () => cancelAnimationFrame(id);
    }
    ringIn = false;
  });

  const timeText = $derived(checkedAt ? new Date(checkedAt).toLocaleString() : "");

  /** 左栏那三格小数据。 */
  const stats = $derived<[string, number][]>([
    [t("home.statItems"), checks.length],
    [t("home.statTodo"), problems.length],
    [t("home.statFix"), fixable.length],
  ]);

  /**
   * 右栏下面那片空白用四行真数据填上：体检说配置缺什么，这一栏说本机到底装了什么。
   * 条的长短是「可用版本 / 全部记录」，坏记录越多条越短，和右边那两个数字对得上。
   */
  const runtimeRows = $derived(
    ALL_TOOLS.map((tool) => {
      const page = perTool?.[tool];
      const versions = page?.versions ?? [];
      const usable = versions.filter((v) => v.usable).length;
      const bytes = versions.reduce((sum, v) => sum + (v.sizeBytes ?? 0), 0);
      return {
        tool,
        total: versions.length,
        usable,
        bytes,
        current: page?.resolution.version ?? page?.defaultVersion ?? null,
        pct: versions.length ? Math.round((usable / versions.length) * 100) : 0,
      };
    }),
  );

  async function optimize(): Promise<void> {
    if (!fixable.length) {
      app.toast(t("home.optimizeAllDone"), "ok");
      return;
    }
    optimizing = true;
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

<div class="flex h-full flex-col overflow-y-auto">
  <header class="flex items-center gap-3 border-b border-border px-6 py-4">
    <h2 class="text-base font-semibold">{t("nav.home")}</h2>
    <Hint text={t("home.tagline")} />
  </header>

  <div class="space-y-5 px-6 py-5">
    <!-- Star 区置顶：进首页第一眼是它，体检和配置都排在后面。 -->
    <div class="rounded-lg border border-primary/25 bg-detail p-5 elevate-hi">
      <RepoActions />
    </div>

    {#if stage === "idle"}
      <!-- 没体检过：中间只放一颗按钮，不去偷偷扫机器。 -->
      <div class="flex flex-col items-center gap-4 rounded-lg border border-border bg-card px-6 py-14 text-center elevate">
        <span class="relative grid size-14 place-items-center rounded-full bg-primary/10">
          <Radar class="size-6 text-primary" />
        </span>
        <div>
          <p class="text-sm font-semibold">{t("home.detectTitle")}</p>
          <p class="mt-1 max-w-md text-xs leading-relaxed text-muted-foreground">{t("home.detectDesc")}</p>
        </div>
        <Button onclick={() => void startCheck()}>
          <Radar class="size-4" />
          {t("home.detectBtn")}
        </Button>
      </div>
    {:else if stage === "checking"}
      <div class="flex flex-col items-center gap-4 rounded-lg border border-border bg-card px-6 py-14 text-center elevate">
        <span class="relative grid size-14 place-items-center rounded-full bg-primary/10">
          <span class="absolute inset-0 rounded-full bg-primary/15 animate-ping [animation-duration:1.8s]" aria-hidden="true"></span>
          <Radar class="relative size-6 text-primary animate-spin [animation-duration:2.4s]" />
        </span>
        <p class="text-sm font-semibold">{t("home.scanning")}</p>

        <div class="w-full max-w-md">
          <div class="h-1.5 overflow-hidden rounded-full bg-muted" role="progressbar" aria-valuemin={0} aria-valuemax={100} aria-valuenow={progress}>
            <span
              class="block h-full rounded-full bg-primary transition-[width] duration-150 ease-linear"
              style="width:{progress}%"
            ></span>
          </div>
          <p class="mono mt-2 flex items-center justify-between text-xs text-muted-foreground">
            <span>{phase}</span>
            <span class="tabular-nums">{progress}%</span>
          </p>
        </div>

        <ul class="mt-1 flex items-center gap-2" aria-hidden="true">
          {#each phases as p, i (p)}
            <span
              class="size-1.5 rounded-full transition-colors duration-200
                {progress >= (i + 1) * 25 ? 'bg-primary' : 'bg-border'}"
            ></span>
          {/each}
        </ul>
        <p class="max-w-md text-xs leading-relaxed text-muted-foreground">{t("home.detectFoot")}</p>
      </div>
    {:else}
      <!-- 优化大盘：左边一圈进度给总分，右边按分组显示哪几块还没配满。 -->
      <div class="rounded-lg border border-border bg-card p-5 elevate">
        <div class="grid gap-6 lg:grid-cols-[minmax(0,15rem)_minmax(0,1fr)]">
          <div class="flex flex-col items-center gap-3">
            <div class="relative grid size-40 place-items-center">
              <svg viewBox="0 0 120 120" class="absolute inset-0 size-full -rotate-90" aria-hidden="true">
                <circle cx="60" cy="60" r="52" fill="none" stroke-width="9" class="stroke-border" />
                <circle
                  cx="60"
                  cy="60"
                  r="52"
                  fill="none"
                  stroke-width="9"
                  stroke-linecap="round"
                  stroke-dasharray={RING}
                  stroke-dashoffset={ringIn ? RING * (1 - score / 100) : RING}
                  class="{ringCls} transition-[stroke-dashoffset] duration-[900ms] ease-out"
                />
              </svg>
              <div class="text-center">
                <p class="text-3xl leading-none font-semibold tabular-nums">
                  <Counter value={score} />
                  {#if ringIn}<span class="text-base text-muted-foreground">%</span>{/if}
                </p>
                <p class="mt-1 text-xs text-muted-foreground">{t("home.scoreLabel")}</p>
              </div>
            </div>
            <p class="text-center text-sm font-medium {problems.length ? "text-foreground" : "text-success"}">
              {scoreLine}
            </p>
            <p class="mono flex items-center gap-1.5 text-xs text-muted-foreground">
              {t("home.checkCount", { total: checks.length, ok: okCount })}
              <Hint text={scoreNote} />
            </p>

            <!-- 三格小数据：把原来空着的左栏下半部填上，数字上来先归零再数到位。 -->
            <ul class="grid w-full grid-cols-3 gap-2 text-center">
              {#each stats as [word, n] (word)}
                <li class="rounded-lg border border-border bg-detail px-1 py-2">
                  <p class="text-base leading-none font-semibold tabular-nums"><Counter value={n} /></p>
                  <p class="mt-1 text-[11px] leading-none text-muted-foreground">{word}</p>
                </li>
              {/each}
            </ul>

            <!-- 两颗按钮等宽并排：一宽一窄还换行，看着就是没对齐。 -->
            <div class="grid w-full grid-cols-2 gap-2">
              <Button size="sm" onclick={() => void optimize()} disabled={!fixable.length || optimizing}>
                <Wand class="size-3.5" />
                {optimizing ? t("home.optimizing") : t("home.optimize")}
              </Button>
              <Button
                size="sm"
                variant="outline"
                disabled={!brokenRecords.length}
                onclick={() => {
                  app.setView("env");
                  app.toast(t("home.conflictJump"), "info", 7000);
                }}
              >
                <Wrench class="size-3.5" />
                {t("home.conflictBtn")}
              </Button>
            </div>

            <!-- 重新检测挪到这里：结果就在眼前，按钮不该跑页头去。 -->
            <div class="flex w-full items-center justify-between gap-2 border-t border-border pt-3">
              <span class="mono min-w-0 truncate text-[11px] text-muted-foreground" title={timeText}>
                {timeText ? t("home.checkedAt", { time: timeText }) : t("home.neverChecked")}
              </span>
              <Button size="sm" variant="ghost" onclick={() => void startCheck()}>
                <RefreshCw class="size-3.5" />
                {t("home.recheck")}
              </Button>
            </div>
          </div>

          <div class="flex min-w-0 flex-col gap-3">
            <div class="flex items-center gap-2">
              <Gauge class="size-4 shrink-0 text-primary" aria-hidden="true" />
              <p class="text-sm font-semibold">{t("home.scanTitle")}</p>
              <Hint text={t("home.scanDesc")} />
            </div>
            <ul class="grid gap-2 sm:grid-cols-2 xl:grid-cols-4">
              {#each groups as g (g.key)}
                <li class="flex flex-col gap-1.5 rounded-lg border border-border px-3 py-2.5 text-xs">
                  <span class="flex items-center gap-2">
                    {#if g.done === g.total}
                      <CircleCheck class="size-3.5 shrink-0 text-success" />
                    {:else}
                      <Wand class="size-3.5 shrink-0 text-primary" />
                    {/if}
                    <span class="min-w-0 flex-1 font-medium">{t(g.key)}</span>
                    <span class="mono shrink-0 tabular-nums text-muted-foreground"><Counter value={g.done} />/{g.total}</span>
                  </span>
                  <span class="h-1 overflow-hidden rounded-full bg-muted" aria-hidden="true">
                    <span
                      class="block h-full rounded-full {g.done === g.total ? "bg-success" : "bg-primary"} transition-[width] duration-700 ease-out"
                      style="width:{(g.done / g.total) * 100}%"
                    ></span>
                  </span>
                </li>
              {/each}
            </ul>

            <!-- 体检说配置缺什么，这一栏说本机装了什么：紧贴四块进度卡，默认窗口高度下不用滚动就能看到。 -->
            <div class="rounded-xl border border-border bg-detail px-4 py-3">
              <div class="flex items-center gap-2">
                <Boxes class="size-4 shrink-0 text-primary" aria-hidden="true" />
                <p class="text-sm font-semibold">{t("home.runtimeTitle")}</p>
                <Hint text={t("home.runtimeDesc")} />
              </div>
              <ul class="mt-2.5 grid gap-x-6 gap-y-2.5 sm:grid-cols-2">
                {#each runtimeRows as row, i (row.tool)}
                  <li class="flex min-w-0 flex-col gap-1">
                    <span class="flex items-center gap-2 text-xs">
                      <RuntimeIcon tool={row.tool} class="size-4 shrink-0" />
                      <span class="min-w-0 flex-1 font-medium">{TOOL_LABEL[row.tool]}</span>
                      <!-- 等宽只给版本号对齐用；「还没装」这类词塞进 mono 字体里，片假名会被拉成一格一个字。 -->
                      <span class="shrink-0 tabular-nums {row.current ? "mono" : "text-muted-foreground"}">
                        {row.current
                          ? t("home.inUseWord", { v: row.current })
                          : row.total
                            ? t("home.notSet")
                            : t("home.noneShort")}
                      </span>
                    </span>
                    <span class="flex items-center gap-2">
                      <span class="h-1.5 min-w-0 flex-1 overflow-hidden rounded-full bg-muted" aria-hidden="true">
                        <span
                          class="block h-full rounded-full {row.usable === row.total ? "bg-success" : "bg-warning"} transition-[width] duration-700 ease-out"
                          style="width:{ringIn ? row.pct : 0}%; transition-delay:{i * 90}ms"
                        ></span>
                      </span>
                      <span class="flex shrink-0 items-center gap-1.5 text-[11px] tabular-nums text-muted-foreground">
                        <span>{t("home.versionCountLine", { n: row.total, bytes: humanBytes(row.bytes) })}</span>
                        <span aria-hidden="true">·</span>
                        <span>{t("home.usableOf", { ok: row.usable, total: row.total })}</span>
                      </span>
                    </span>
                  </li>
                {/each}
              </ul>
            </div>

            <!-- 待处理明细挪进右栏：四块进度卡下面原来是一整片空白，要翻到卡片外面才看得到事项。 -->
            {#if problems.length}
              <p class="mt-1 text-sm font-semibold">{t("home.todoPanel", { n: problems.length })}</p>
              <ul class="space-y-2">
                {#each problems as c (c.id)}
                  <li class="flex flex-wrap items-start gap-3 rounded-xl border border-border bg-card px-4 py-3 text-xs">
                    <span class="grid size-7 shrink-0 place-items-center rounded-full bg-primary/10">
                      <Wand class="size-4 text-primary" />
                    </span>
                    <div class="min-w-0 flex-1">
                      <p class="flex flex-wrap items-center gap-2 text-sm font-medium">
                        {c.title}
                        <Badge tone="accent">{t("home.todoState")}</Badge>
                        <Badge tone="outline">{t(groupLabel(c.id))}</Badge>
                      </p>
                      <p class="mt-0.5 leading-relaxed text-muted-foreground">{c.detail}</p>
                    </div>
                    {#if c.fix}
                      <Button size="sm" variant="outline" disabled={optimizing} onclick={() => void c.fix?.()}>
                        {c.fixLabel ?? t("home.fixThis")}
                      </Button>
                    {/if}
                  </li>
                {/each}
              </ul>
            {:else}
              <p class="flex items-center gap-2 rounded-xl border border-success/30 bg-success/8 px-4 py-3 text-sm">
                <CircleCheck class="size-4 shrink-0 text-success" />
                {t("home.healthy")}
              </p>
            {/if}
            <p class="mt-auto pt-1 text-xs leading-relaxed text-muted-foreground">{t("home.optimizeFoot")}</p>
          </div>
        </div>
      </div>

      {#if passed.length}
        <Section title={t("home.donePanel", { n: passed.length })} icon={CircleCheck} collapsible>
          <ul class="space-y-2">
            {#each passed as c (c.id)}
              <li class="flex flex-wrap items-start gap-3 rounded-xl border border-border bg-detail px-4 py-2.5 text-xs">
                <CircleCheck class="mt-0.5 size-4 shrink-0 text-success" />
                <div class="min-w-0 flex-1">
                  <p class="text-sm font-medium">{c.title}</p>
                  <p class="mt-0.5 leading-relaxed text-muted-foreground">{c.detail}</p>
                </div>
                <Badge tone="outline">{t(groupLabel(c.id))}</Badge>
              </li>
            {/each}
          </ul>
        </Section>
      {/if}
    {/if}
  </div>
</div>
