//! 全局界面状态。所有写操作都走 Rust，本地只缓存展示用的结果。
import { api, messageOf } from "./ipc";
import { i18n, t } from "$lib/i18n/store.svelte";
import type {
  Boot,
  DirContext,
  Guess,
  HookStatus,
  InstalledPage,
  JobEvent,
  MirrorProbe,
  PathScope,
  PathStatus,
  ScanReport,
  Settings,
  SourceInfo,
  ToolId,
  VersionPage,
} from "./types";
import { scopeLabel, onSystemDrive, TOOL_LABEL } from "./utils";

export type View = "home" | "tool" | "project" | "mirror" | "env" | "ai" | "settings" | "about";

/** 中间区域被点选的那张卡片；右侧详情只在这个有值时才出现。 */
export interface Selection {
  tool: ToolId;
  version: string;
}

export interface Toast {
  id: number;
  text: string;
  tone: "info" | "ok" | "warn" | "bad";
}

export interface ProgressRow {
  jobId: string;
  tool: ToolId | null;
  version: string | null;
  phase: JobEvent["phase"];
  received: number;
  total: number | null;
  bytesPerSec: number;
  etaSecs: number | null;
  percent: number | null;
  message: string | null;
}

/** 危险确认弹窗的内容；resolve 由弹窗组件在用户点按钮时调用。 */
export interface DangerAsk {
  tone?: "default" | "danger";
  title: string;
  body: string;
  confirm: string;
  resolve: (ok: boolean) => void;
}

let toastSeq = 1;

export class AppState {
  boot: Boot | null = $state(null);
  /** 后端起来了但首屏数据没读到：整页显示这个，别让人对着空白转圈猜。 */
  bootError: string | null = $state(null);
  /** 启动日志在哪：失败页和设置页都拿它当"去哪儿看细节"的入口。 */
  startupLog = $state("");
  settings: Settings | null = $state(null);
  view: View = $state("home");
  tool: ToolId = $state("node");
  /** 设置不再占一个侧栏视图，改成盖在当前页上的浮层；这个开关只控制浮层。 */
  settingsOpen = $state(false);
  /** 有值就弹危险确认框，用户点完才继续。 */
  danger: DangerAsk | null = $state(null);
  selected: Selection | null = $state(null);

  page: VersionPage | null = $state(null);
  installed: InstalledPage | null = $state(null);
  dirContext: DirContext | null = $state(null);
  sources: SourceInfo[] = $state([]);
  probes: MirrorProbe[] = $state([]);
  hooks: HookStatus[] = $state([]);
  paths: PathStatus[] = $state([]);
  scan: ScanReport | null = $state(null);
  guesses: Guess[] = $state([]);

  progress = new Map<string, ProgressRow>();
  /** 进度条每来一个包就自增，界面用它驱动重渲染。 */
  tick = $state(0);

  busy = $state(false);
  loadingVersions = $state(false);
  scanning = $state(false);
  private envRequest: Promise<boolean> | null = null;
  private toolCache = new Map<ToolId, { page: VersionPage; installed: InstalledPage; sources: SourceInfo[] }>();
  private toolRequests = new Map<ToolId, Promise<void>>();
  private toolGeneration = new Map<ToolId, number>();
  filter = $state("");
  toasts: Toast[] = $state([]);
  dark = $state(false);

  get ready(): boolean {
    return this.boot !== null && !this.boot.needsSetup;
  }

  toast(text: string, tone: Toast["tone"] = "info", ms = 5200): void {
    const id = toastSeq++;
    this.toasts.push({ id, text, tone });
    setTimeout(() => {
      this.toasts = this.toasts.filter((t) => t.id !== id);
    }, ms);
  }

  /** 所有失败都经这里：一句话，不堆内部术语。 */
  fail(e: unknown, prefix = t("msg.opFailed")): void {
    this.toast(`${prefix}：${messageOf(e)}`, "bad", 9000);
  }

  applyTheme(): void {
    const want = this.settings?.theme ?? "system";
    const sysDark = window.matchMedia("(prefers-color-scheme: dark)").matches;
    this.dark = want === "dark" || (want === "system" && sysDark);
    document.documentElement.classList.toggle("dark", this.dark);
  }

  /** 语言存在 Rust 的 settings 里，所以每次读回设置都要把它同步进 i18n 状态。 */
  applyLanguage(): void {
    i18n.pref = this.settings?.language ?? "auto";
    document.documentElement.lang = i18n.locale;
  }

  /** 设置从 Rust 回来后进这里：配色和语言都要跟着刷新。 */
  applySettings(next: Settings): void {
    const previous = this.settings;
    if (previous && (JSON.stringify(previous.sources) !== JSON.stringify(next.sources) ||
        JSON.stringify(previous.customBase) !== JSON.stringify(next.customBase) ||
        previous.proxyMode !== next.proxyMode || previous.proxyUrl !== next.proxyUrl)) {
      for (const tool of new Set([...this.toolCache.keys(), ...this.toolRequests.keys()])) this.invalidateTool(tool);
    }
    this.settings = next;
    this.applyTheme();
    this.applyLanguage();
  }

  async start(): Promise<void> {
    const boot = await api.boot();
    this.boot = boot;
    this.settings = boot.settings;
    this.hooks = boot.hooks;
    this.paths = boot.path;
    this.applyTheme();
    this.applyLanguage();
    document.querySelectorAll<HTMLScriptElement>("script[data-theme-boot]").forEach((n) => n.remove());
    if (boot.needsSetup) return;
    // 首屏不等待远程版本清单；已有环境由首页在后台读取。
    await this.refreshDir(boot.settings.lastProjectDir ?? null);
  }

  setView(view: View): void {
    if (view === "settings") {
      this.settingsOpen = true;
      return;
    }
    this.view = view;
    if (view !== "tool") this.selected = null;
    if (view === "project") void this.refreshDir(null);
  }

  closeSettings(): void {
    this.settingsOpen = false;
  }

  /** 弹一次危险确认，等用户点完再往下走。 */
  askDanger(title: string, body: string, confirm: string, tone: "default" | "danger" = "danger"): Promise<boolean> {
    this.answerDanger(false);
    return new Promise((resolve) => {
      this.danger = { title, body, confirm, tone, resolve };
    });
  }

  answerDanger(ok: boolean): void {
    const ask = this.danger;
    this.danger = null;
    ask?.resolve(ok);
  }

  /** 落在系统盘上的目录要用户亲自点头才用；换个盘就直接放行。 */
  private async confirmSystemDrive(dir: string, what: string): Promise<boolean> {
    if (!onSystemDrive(dir, this.boot?.systemDrive ?? "")) return true;
    return this.askDanger(
      t("danger.sysDriveTitle", { what }),
      t("danger.sysDriveBody", { dir, drive: this.boot?.systemDrive ?? "" }),
      t("danger.sysDriveConfirm"),
    );
  }

  /** 直接指定项目目录（点已登记的那条）：系统盘先确认，再解析并落盘。 */
  async useProjectDir(dir: string): Promise<boolean> {
    if (!(await this.confirmSystemDrive(dir, t("danger.whatProjectDir")))) return false;
    await this.refreshDir(dir);
    return true;
  }

  /** 选项目目录的唯一入口：选完先过系统盘确认，再解析并落盘。 */
  async pickProjectDir(): Promise<string | null> {
    let picked: string | null = null;
    try {
      picked = await api.pickFolder();
    } catch (e) {
      this.fail(e, t("ai.pickFailed"));
      return null;
    }
    if (!picked) return null;
    return (await this.useProjectDir(picked)) ? picked : null;
  }

  /** 版本页里的工具切换；下载源那种「只是换个工具看」的调用传 false，别把页面顶走。 */
  async setTool(tool: ToolId, gotoVersions = true): Promise<void> {
    this.tool = tool;
    if (gotoVersions) this.view = "tool";
    this.selected = null;
    this.filter = "";
    const cached = this.toolCache.get(tool);
    this.page = cached?.page ?? null;
    this.installed = cached?.installed ?? null;
    this.sources = cached?.sources ?? [];
    this.loadingVersions = !cached;
    if (!cached) await this.refreshTool();
    else {
      // Local state may change while another runtime is selected.
      try {
        const installed = await api.installedVersions(tool, this.dirContext?.dir ?? null);
        cached.installed = installed;
        if (this.tool === tool) this.installed = installed;
      } catch (e) { if (this.tool === tool) this.fail(e); }
    }
  }

  select(version: string | null): void {
    this.selected = version ? { tool: this.tool, version } : null;
  }

  private invalidateTool(tool: ToolId): void {
    this.toolCache.delete(tool);
    this.toolGeneration.set(tool, (this.toolGeneration.get(tool) ?? 0) + 1);
    this.toolRequests.delete(tool);
  }

  async refreshTool(refresh = false): Promise<void> {
    const tool = this.tool;
    if (!refresh && this.toolRequests.has(tool)) return this.toolRequests.get(tool);
    const generation = (this.toolGeneration.get(tool) ?? 0) + 1;
    this.toolGeneration.set(tool, generation);
    this.loadingVersions = true;
    const request = (async () => {
      try {
        const [page, installed, sources] = await Promise.all([
          api.listVersions(tool, refresh),
          api.installedVersions(tool, this.dirContext?.dir ?? null),
          api.sourcesOf(tool),
        ]);
        if (this.toolGeneration.get(tool) !== generation) return;
        this.toolCache.set(tool, { page, installed, sources });
        if (this.tool === tool) {
          this.page = page;
          this.installed = installed;
          this.sources = sources;
        }
      } catch (e) {
        if (this.tool === tool && this.toolGeneration.get(tool) === generation)
          this.fail(e, t("msg.versionListFailed", { tool: TOOL_LABEL[tool] }));
      } finally {
        if (this.toolGeneration.get(tool) === generation) {
          this.toolRequests.delete(tool);
          if (this.tool === tool) this.loadingVersions = false;
        }
      }
    })();
    this.toolRequests.set(tool, request);
    return request;
  }

  get toolName(): string {
    return TOOL_LABEL[this.tool];
  }

  async refreshMirror(): Promise<void> {
    const tool = this.tool;
    try {
      const [sources, probes] = await Promise.all([api.sourcesOf(tool), api.probeSources(tool)]);
      if (this.tool === tool) { this.sources = sources; this.probes = probes; }
    } catch (e) { if (this.tool === tool) this.fail(e, t("msg.probeFailed")); }
  }

  async refreshEnv(): Promise<boolean> {
    if (this.envRequest) return this.envRequest;
    this.scanning = true;
    this.envRequest = (async () => {
      try {
        const [scan, hooks, paths, shims] = await Promise.all([api.envScan(), api.hookStatus(), api.pathStatus(), api.shimList()]);
        this.scan = scan; this.hooks = hooks; this.paths = paths;
        if (this.boot) this.boot.shims = shims;
        return true;
      } catch (e) { this.fail(e, t("msg.envScanFailed")); return false; }
      finally { this.scanning = false; this.envRequest = null; }
    })();
    return this.envRequest;
  }

  async refreshDir(dir: string | null): Promise<void> {
    try {
      this.dirContext = await api.dirResolution(dir);
      const now = this.dirContext.dir || null;
      if (this.settings) {
        this.settings = { ...this.settings, lastProjectDir: now };
      }
      // 用户挑的目录要落盘，否则重开程序又回到"没选过"，界面再次显示别的目录。
      if (dir && dir.trim()) await api.rememberProjectDir(now);
    } catch (e) {
      this.fail(e, t("msg.dirReadFailed"));
    }
  }

  /** 取消登记：后端会顺手清掉记住的目录，这里重读一次让界面跟着变。 */
  async forgetProject(dir: string): Promise<void> {
    await api.forgetProject(dir);
    await this.refreshDir(null);
  }

  /** 进度事件按任务号归并，卸载/接管这类没有百分比的阶段也照样占一行。 */
  onJob(ev: JobEvent): void {
    this.progress.set(ev.jobId, {
      jobId: ev.jobId,
      tool: ev.tool,
      version: ev.version,
      phase: ev.phase,
      received: ev.received,
      total: ev.total,
      bytesPerSec: ev.bytesPerSec,
      etaSecs: ev.etaSecs,
      percent: ev.percent,
      message: ev.message,
    });
    this.tick += 1;
    if (ev.phase === "done" || ev.phase === "failed" || ev.phase === "cancelled") {
      const snapshot = ev;
      setTimeout(() => {
        this.progress.delete(snapshot.jobId);
        this.tick += 1;
      }, snapshot.phase === "done" ? 2600 : 8000);
      if (snapshot.phase === "done") {
        if (snapshot.tool) this.invalidateTool(snapshot.tool);
        if (this.view === "tool") void this.refreshTool();
        void this.refreshDir(null);
      }
    }
  }

  private forwardingCheck: Promise<boolean> | null = null;

  async ensureForwarding(): Promise<boolean> {
    if (this.forwardingCheck) return this.forwardingCheck;
    this.forwardingCheck = (async () => {
      const missing = await api.shimMissing();
      if (!missing.length) return true;
      if (!(await this.askDanger(t("shim.repairTitle"), t("shim.repairBody", { files: missing.join("\n") }), t("shim.repairConfirm"), "default"))) return false;
      const files = await api.shimSync();
      if (this.boot) this.boot = { ...this.boot, shims: files };
      this.toast(t("msg.shimSynced", { n: files.length }), "ok");
      return true;
    })();
    try { return await this.forwardingCheck; }
    finally { this.forwardingCheck = null; }
  }

  async install(version: string): Promise<void> {
    try {
      if (!(await this.ensureForwarding())) return;
      await api.installVersion(this.tool, version);
      this.toast(t("msg.installStarted", { tool: this.toolName, version }), "info");
    } catch (e) {
      this.fail(e, t("msg.installNotStarted"));
    }
  }

  async cancel(jobId: string): Promise<void> {
    try {
      await api.cancelJob(jobId);
    } catch (e) {
      this.fail(e, t("msg.cancelNotSent"));
    }
  }

  async switchTo(version: string): Promise<void> {
    const tool = this.tool;
    try {
      if (!(await this.ensureForwarding())) return;
      const res = await api.switchVersion(tool, version);
      this.invalidateTool(tool);
      if (this.tool === tool) await this.refreshTool();
      this.toast(
        res.note ?? t("msg.switched", { tool: TOOL_LABEL[tool], version }),
        "ok",
      );
    } catch (e) {
      this.fail(e, t("msg.switchFailed"));
    }
  }

  async uninstall(version: string, purge: boolean, tool: ToolId = this.tool): Promise<void> {
    try {
      const note = await api.uninstallVersion(tool, version, purge);
      this.invalidateTool(tool);
      this.toast(note, "ok", 7000);
      await Promise.all([this.refreshTool(), this.refreshDir(null)]);
    } catch (e) {
      this.fail(e, t("msg.uninstallFailed"));
    }
  }

  async adopt(exe: string, tool: ToolId, version: string | null, copy: boolean): Promise<void> {
    try {
      if (!(await this.ensureForwarding())) return;
      const res = await api.adoptRuntime(tool, exe, version, copy);
      this.invalidateTool(tool);
      this.toast(`${res.installed.version}：${res.note}`, "ok", 8000);
      if (this.view === "tool") void this.refreshTool();
      await this.refreshEnv();
    } catch (e) {
      this.fail(e, t("msg.adoptFailed"));
    }
  }

  async pickOffline(): Promise<void> {
    try {
      const files = await api.pickFiles();
      if (!files.length) return;
      this.guesses = await api.precheckOffline(files);
      this.view = "tool";
    } catch (e) {
      this.fail(e, t("msg.offlineUnreadable"));
    }
  }

  async importOffline(files: string[]): Promise<void> {
    try {
      if (!(await this.ensureForwarding())) return;
      const jobs = await api.importOffline(files);
      this.toast(t("msg.importStarted", { n: jobs.length }), "info");
      this.guesses = [];
    } catch (e) {
      this.fail(e, t("msg.importFailed"));
    }
  }

  async saveSettings(next: Settings): Promise<void> {
    try {
      this.applySettings(await api.updateSettings(next));
      this.paths = await api.pathStatus();
      this.toast(t("msg.settingsSaved"), "ok", 2600);
    } catch (e) {
      this.fail(e, t("msg.settingsNotSaved"));
    }
  }

  async patch(part: Partial<Settings>, savedNote?: string): Promise<void> {
    if (!this.settings) return;
    const next = { ...this.settings, ...part };
    try {
      if ((part.autoSwitch === true && !this.settings.autoSwitch) ||
          (part.pathScope !== undefined && part.pathScope !== this.settings.pathScope)) {
        if (!(await this.ensureForwarding())) return;
      }
      this.applySettings(await api.updateSettings(next));
      this.paths = await api.pathStatus();
      if (savedNote) this.toast(savedNote, "ok", 3200);
    } catch (e) {
      this.fail(e, t("msg.switchNotApplied"));
    }
  }

  async revealPath(path: string | null): Promise<void> {
    if (!path) return;
    try {
      await api.revealPath(path);
    } catch (e) {
      this.fail(e, t("msg.revealFailed"));
    }
  }

  async openLink(url: string): Promise<void> {
    try {
      await api.openLink(url);
    } catch (e) {
      this.fail(e, t("msg.browserFailed"));
    }
  }

  async chooseDataRoot(dir: string): Promise<void> {
    if (!(await this.confirmSystemDrive(dir, t("danger.whatDataRoot")))) return;
    try {
      const layout = await api.chooseDataRoot(dir);
      this.boot = await api.boot();
      this.settings = this.boot!.settings;
      this.toast(t("msg.dataRootSet", { dir: layout.root }), "ok", 7000);
      await this.refreshTool();
    } catch (e) {
      this.fail(e, t("msg.dataRootUnusable"));
    }
  }

  async pathApply(scope: PathScope): Promise<void> {
    try {
      if (!(await this.ensureForwarding())) return;
      const st = await api.pathApply(scope);
      this.paths = await api.pathStatus();
      this.toast(
        st.installed ? t("msg.pathApplied", { scope: scopeLabel(scope) }) : t("msg.pathMissing"),
        st.installed ? "ok" : "warn",
        7000,
      );
    } catch (e) {
      this.fail(e, t("msg.pathFailed"));
    }
  }

  async pathWithdraw(scope: PathScope): Promise<void> {
    try {
      await api.pathWithdraw(scope);
      this.paths = await api.pathStatus();
      this.toast(t("msg.pathWithdrawn"), "ok", 7000);
    } catch (e) {
      this.fail(e, t("msg.pathWithdrawFailed"));
    }
  }

  async pathRestore(id: string): Promise<void> {
    try {
      await api.pathRestore(id);
      this.paths = await api.pathStatus();
      this.toast(t("msg.pathRestored"), "ok", 6000);
    } catch (e) {
      this.fail(e, t("msg.restoreFailed"));
    }
  }

  async hookToggle(shell: string, on: boolean): Promise<void> {
    try {
      if (on && !(await this.ensureForwarding())) return;
      this.hooks = await api.hookSet(shell, on);
    } catch (e) {
      this.fail(e, on ? t("msg.hookOnFailed") : t("msg.hookOffFailed"));
    }
  }

  async shimSync(): Promise<void> {
    try {
      const files = await api.shimSync();
      if (this.boot) this.boot = { ...this.boot, shims: files };
      this.toast(t("msg.shimSynced", { n: files.length }), "ok");
    } catch (e) {
      this.fail(e, t("msg.shimSyncFailed"));
    }
  }

  async lockProject(tool: ToolId, version: string): Promise<void> {
    const dir = this.dirContext?.dir;
    if (!dir) return;
    try {
      if (!(await this.ensureForwarding())) return;
      await api.lockProject(dir, tool, version);
      await this.refreshDir(dir);
      this.toast(t("msg.projectLocked", { version }), "ok");
    } catch (e) {
      this.fail(e, t("msg.projectLockFailed"));
    }
  }

  async unlockProject(tool: ToolId): Promise<void> {
    const dir = this.dirContext?.dir;
    if (!dir) return;
    try {
      await api.unlockProject(dir, tool);
      await this.refreshDir(dir);
      this.toast(t("msg.projectUnlocked"), "ok");
    } catch (e) {
      this.fail(e, t("msg.projectUnlockFailed"));
    }
  }

  async exportConfig(): Promise<void> {
    try {
      const name = await api.configSuggestedName();
      const dest = await api.pickSavePath(name);
      if (!dest) return;
      const sum = await api.exportConfig(dest);
      this.toast(t("msg.configExported", { n: sum.installedCount }), "ok", 7000);
    } catch (e) {
      this.fail(e, t("msg.exportFailed"));
    }
  }

  async importConfig(): Promise<void> {
    try {
      const files = await api.pickFiles();
      if (!files.length) return;
      const res = await api.importConfig(files[0], true);
      this.toast(res.note, res.missing.length ? "warn" : "ok", 9000);
      await this.refreshTool();
      await this.refreshDir(null);
    } catch (e) {
      this.fail(e, t("msg.importFailed"));
    }
  }

  async resetNow(keepInstalls: boolean): Promise<void> {
    try {
      const out = await api.resetNow(keepInstalls);
      this.toast(
        out.failed.length
          ? t("msg.resetDonePartial", { n: out.shimsRemoved, m: out.failed.length })
          : t("msg.resetDone", { n: out.shimsRemoved }),
        out.failed.length ? "warn" : "ok",
        9000,
      );
      this.boot = await api.boot();
    } catch (e) {
      this.fail(e, t("msg.resetFailed"));
    }
  }
}

export const app = new AppState();
