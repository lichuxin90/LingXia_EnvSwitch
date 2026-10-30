//! 界面到 Rust 的唯一通道。命令名保持 Rust 函数名的下划线写法，参数按 serde 的小驼峰。
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

import type {
  Adopted,
  AiClient,
  Boot,
  DataRootCandidate,
  DirContext,
  ForeignRuntime,
  Guess,
  HookStatus,
  ImportOutcome,
  InstalledPage,
  JobEvent,
  JobHandle,
  JobView,
  Layout,
  MirrorProbe,
  PathBackup,
  PathScope,
  PathStatus,
  PathView,
  ProjectLock,
  ProxyMode,
  ResetOutcome,
  ResetPreview,
  ScanReport,
  Settings,
  ShimFile,
  SourceInfo,
  Switched,
  ToolId,
  TransferSummary,
  UpdateInfo,
  VersionPage,
} from "./types";

export const EV_JOB = "envswitch://job";
export const EV_INSTALLED = "envswitch://installed";
export const EV_SETTINGS = "envswitch://settings";
export const EV_PROJECT = "envswitch://project";

/** 调用失败时 Rust 侧发回的是 { kind, message, detail }，统一转成 Error 好让界面直接展示。 */
export function messageOf(e: unknown): string {
  if (e && typeof e === "object") {
    const o = e as { message?: string; detail?: string };
    if (o.message) return o.detail ? `${o.message}（${o.detail}）` : o.message;
  }
  return String(e);
}

export const api = {
  boot: () => invoke<Boot>("boot"),
  chooseDataRoot: (dir: string) => invoke<Layout>("choose_data_root", { dir }),
  dataRootCandidates: () => invoke<DataRootCandidate[]>("data_root_candidates"),
  revealPath: (path: string | null) => invoke<void>("reveal_path", { path }),
  openLink: (url: string) => invoke<void>("open_link", { url }),

  listVersions: (tool: ToolId, refresh = false, source?: string) =>
    invoke<VersionPage>("list_versions", { tool, refresh, source: source ?? null }),
  sourcesOf: (tool: ToolId) => invoke<SourceInfo[]>("sources_of", { tool }),
  probeSources: (tool: ToolId) => invoke<MirrorProbe[]>("probe_sources", { tool }),
  installedVersions: (tool: ToolId, dir?: string | null) =>
    invoke<InstalledPage>("installed_versions", { tool, dir: dir ?? null }),

  installVersion: (tool: ToolId, version: string, source?: string, force = false) =>
    invoke<JobHandle>("install_version", {
      tool,
      version,
      source: source ?? null,
      force,
    }),
  importOffline: (files: string[]) => invoke<JobHandle[]>("import_offline", { files }),
  precheckOffline: (files: string[]) => invoke<Guess[]>("precheck_offline", { files }),
  adoptRuntime: (tool: ToolId, exe: string, version: string | null, copy: boolean) =>
    invoke<Adopted>("adopt_runtime", { tool, exe, version, copy }),
  uninstallVersion: (tool: ToolId, version: string, purge = false) =>
    invoke<string>("uninstall_version", { tool, version, purge }),
  switchVersion: (tool: ToolId, version: string) =>
    invoke<Switched>("switch_version", { tool, version }),

  cancelJob: (jobId: string) => invoke<void>("cancel_job", { jobId }),
  activeJobs: () => invoke<JobView[]>("active_jobs"),

  getSettings: () => invoke<Settings>("get_settings"),
  updateSettings: (patch: Settings) => invoke<Settings>("update_settings", { patch }),
  testProxy: (mode: ProxyMode, url: string | null) =>
    invoke<string>("test_proxy", { mode, url }),
  setAutostart: (on: boolean) => invoke<boolean>("set_autostart", { on }),

  pathStatus: () => invoke<PathStatus[]>("path_status"),
  pathApply: (scope: PathScope) => invoke<PathStatus>("path_apply", { scope }),
  pathWithdraw: (scope: PathScope) => invoke<PathStatus>("path_withdraw", { scope }),
  pathReorder: (scope: PathScope) => invoke<PathStatus>("path_reorder", { scope }),
  pathBackups: () => invoke<PathBackup[]>("path_backups"),
  pathRestore: (id: string) => invoke<PathStatus>("path_restore", { id }),
  pathView: () => invoke<PathView>("path_view"),

  shimList: () => invoke<ShimFile[]>("shim_list"),
  shimSync: () => invoke<ShimFile[]>("shim_sync"),

  envScan: () => invoke<ScanReport>("env_scan"),
  foreignOf: async (): Promise<ForeignRuntime[]> => (await api.envScan()).foreign,

  hookStatus: () => invoke<HookStatus[]>("hook_status"),
  hookSet: (shell: string, on: boolean) => invoke<HookStatus[]>("hook_set", { shell, on }),

  dirResolution: (dir: string | null) => invoke<DirContext>("dir_resolution", { dir }),
  lockProject: (dir: string, tool: ToolId, version: string) =>
    invoke<ProjectLock>("lock_project", { dir, tool, version }),
  unlockProject: (dir: string, tool: ToolId) =>
    invoke<ProjectLock | null>("unlock_project", { dir, tool }),
  registeredProjects: () => invoke<ProjectLock[]>("registered_projects"),
  forgetProject: (dir: string) => invoke<void>("forget_project", { dir }),
  setWatchDir: (dir: string | null) => invoke<void>("set_watch_dir", { dir }),
  setGlobalLock: (locked: boolean) => invoke<void>("set_global_lock", { locked }),

  pickFolder: () => invoke<string | null>("pick_folder"),
  pickFiles: () => invoke<string[]>("pick_files"),
  pickSavePath: (name: string) => invoke<string | null>("pick_save_path", { name }),

  resetPreview: () => invoke<ResetPreview>("reset_preview"),
  resetNow: (keepInstalls: boolean) => invoke<ResetOutcome>("reset_now", { keepInstalls }),

  exportConfig: (dest: string) => invoke<TransferSummary>("export_config", { dest }),
  importConfig: (src: string, applySettings: boolean) =>
    invoke<ImportOutcome>("import_config", { src, applySettings }),
  configSuggestedName: () => invoke<string>("config_suggested_name"),

  checkUpdate: () => invoke<UpdateInfo>("check_update"),
  aiDetect: () => invoke<AiClient[]>("ai_detect"),
  /** 写我们自己的独立规则文件：`head` 是客户端硬要求的文件头部，没有就传空串。 */
  aiWriteFile: (dir: string, file: string, head: string, content: string) =>
    invoke<string>("ai_write_file", { dir, file, head, content }),
  aiRemoveFile: (dir: string, file: string) => invoke<string>("ai_remove_file", { dir, file }),
  aiWriteRules: (dir: string, file: string, content: string) =>
    invoke<string>("ai_write_rules", { dir, file, content }),
  aiRemoveRules: (dir: string, file: string) => invoke<string>("ai_remove_rules", { dir, file }),
  /** 界面首帧画完了就报一声，守护进程拿这个当"真的起来了"的证据。 */
  appReady: () => invoke<void>("app_ready"),
  startupLog: () => invoke<string>("startup_log"),
  /** 托盘菜单和窗口标题在界面外面，前端算出真正生效的语言后送一份过去。 */
  setShellLang: (lang: string) => invoke<void>("set_shell_lang", { lang }),
};

export function onJob(handler: (e: JobEvent) => void): Promise<UnlistenFn> {
  return listen<JobEvent>(EV_JOB, (ev) => handler(ev.payload));
}

export function onSettings(handler: (s: Settings) => void): Promise<UnlistenFn> {
  return listen<Settings>(EV_SETTINGS, (ev) => handler(ev.payload));
}

export function onProject(handler: (dir: string) => void): Promise<UnlistenFn> {
  return listen<string>(EV_PROJECT, (ev) => handler(ev.payload));
}

export function onInstalled(handler: (tool: ToolId) => void): Promise<UnlistenFn> {
  return listen<ToolId>(EV_INSTALLED, (ev) => handler(ev.payload));
}
