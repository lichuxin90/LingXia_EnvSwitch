//! 与 Rust 侧一一对应的数据结构；字段名按 serde 的 camelCase 写死。
//! 这里只做界面展示用的类型声明，任何判定都在 Rust 核心里。

export type ToolId = "node" | "python" | "go" | "pypy";

export type InstallSource = "official" | "mirror" | "offline" | "takeover";

export type JobPhase =
  | "queued"
  | "planning"
  | "downloading"
  | "verifying"
  | "extracting"
  | "linking"
  | "done"
  | "failed"
  | "cancelled";

export type JobKind = "install" | "uninstall" | "takeover" | "import";

export type PathScope = "user" | "system";
export type BackupKind = "path" | "config";
export type ProxyMode = "system" | "direct" | "custom";
export type Theme = "system" | "light" | "dark";
export type Language = "auto" | "zh-CN" | "zh-TW" | "en" | "ja";
export type TrayBehavior = "minimizeToTray" | "quit";
export type ResolutionSource = "projectFile" | "globalDefault" | "onlyInstalled" | "none";
export type ConflictKind = "manager" | "runtime" | "stale";
export type TakeoverMode = "reference" | "copy";

export interface Companion {
  name: string;
  version: string | null;
  note: string | null;
}

export interface AvailableVersion {
  tool: ToolId;
  version: string;
  released: string | null;
  ltsCodename: string | null;
  isLts: boolean;
  ltsActive: boolean | null;
  /** 这条线是否已过停止更新日期；null 表示官方时间表里没这一条。 */
  eol: boolean | null;
  eolDate: string | null;
  isLineLatest: boolean;
  lineCount: number;
  sizeHint: number | null;
  installed: boolean;
  downloadUrl: string | null;
  fileName: string | null;
  sha256: string | null;
  originTag: string | null;
  lineKey: string;
  unavailableReason: string | null;
}

export interface VersionLine {
  tool: ToolId;
  major: string;
  newest: string;
  isLts: boolean;
  ltsActive: boolean | null;
  ltsCodename: string | null;
  eol: boolean | null;
  eolDate: string | null;
  released: string | null;
  installed: boolean;
  isGlobalDefault: boolean;
  sizeBytes: number;
  memberCount: number;
  members: AvailableVersion[];
}

export interface InstalledVersion {
  tool: ToolId;
  version: string;
  dir: string;
  execPath: string;
  sizeBytes: number;
  installedAt: string;
  source: InstallSource;
  sha256: string | null;
  origin: string | null;
  companions: Companion[];
  isActive: boolean;
  isGlobalDefault: boolean;
  boundProjects: string[];
  usable: boolean;
  unusableReason: string | null;
}

export interface Resolution {
  tool: ToolId;
  version: string | null;
  source: ResolutionSource;
  foundIn: string | null;
  execPath: string | null;
  note: string | null;
}

export interface ProjectLock {
  dir: string;
  file: string;
  entries: Record<string, string>;
  extraLines: string[];
}

export interface JobEvent {
  jobId: string;
  tool: ToolId | null;
  version: string | null;
  phase: JobPhase;
  received: number;
  total: number | null;
  bytesPerSec: number;
  etaSecs: number | null;
  percent: number | null;
  message: string | null;
}

export interface JobHandle {
  jobId: string;
  tool: ToolId;
  version: string;
  phase: JobPhase;
}

export interface JobView {
  id: string;
  tool: ToolId;
  version: string;
  kind: JobKind;
  phase: JobPhase;
  running: boolean;
  elapsedSecs: number;
  message: string | null;
}

export interface PathStatus {
  scope: PathScope;
  shimDir: string;
  installed: boolean;
  isFirst: boolean;
  position: number | null;
  entries: string[];
  backupCount: number;
  latestBackup: string | null;
  elevated: boolean;
  note: string | null;
}

export interface PathBackup {
  id: string;
  createdAt: string;
  scope: PathScope;
  kind: BackupKind;
  entryCount: number;
  preview: string[];
}

export interface PathView {
  entries: string[];
  from: string;
  shimDir: string;
  shimPresent: boolean;
  shimFirst: boolean;
}

export interface ForeignRuntime {
  tool: ToolId | null;
  name: string;
  version: string | null;
  exe: string;
  root: string;
  manager: string | null;
  takeable: boolean;
  whyNot: string | null;
  mode: TakeoverMode;
}

export interface Conflict {
  name: string;
  kind: ConflictKind;
  path: string | null;
  detail: string;
  suggestion: string;
}

export interface ScanReport {
  conflicts: Conflict[];
  foreign: ForeignRuntime[];
  oursShimFirst: boolean;
  notes: string[];
}

export interface MirrorProbe {
  id: string;
  label: string;
  url: string;
  region: string;
  ok: boolean;
  latencyMs: number | null;
  error: string | null;
  selected: boolean;
}

export interface SourceInfo {
  id: string;
  label: string;
  region: string;
  base: string;
  wrap: string | null;
  note: string | null;
  probeUrl: string;
}

export interface ResetOutcome {
  shimsRemoved: number;
  pathRestored: string | null;
  pathScope: PathScope;
  installsKept: number;
  installsRemoved: number;
  hooksRemoved: string[];
  failed: string[];
}

export interface ResetPreview {
  shimDir: string;
  shims: number;
  scope: PathScope;
  backupCount: number;
  hooks: string[];
  installedCount: number;
  referencedCount: number;
  installsBytes: number;
}

export interface TransferSummary {
  file: string;
  tools: Record<string, string>;
  projectCount: number;
  installedCount: number;
}

export interface ImportOutcome {
  summary: TransferSummary;
  missing: string[];
  skippedProjects: string[];
  note: string;
}

export interface UpdateInfo {
  current: string;
  latest: string;
  title: string;
  hasUpdate: boolean;
  url: string;
}

/** Rust 只报"在不在、在哪个目录"，客户端叫什么、怎么说明都由前端语言目录决定。 */
export interface AiClient {
  id: string;
  found: boolean;
  /** 配置目录；只找到安装位置时这里为空。 */
  path: string | null;
  /** 实际装在哪个盘，从卸载登记或各盘 Program Files 里读出来的。 */
  install: string | null;
  /** 有能写进去的文件位；false 就是纯聊天客户端，只能在应用里设。 */
  writable: boolean;
  rulesDir: string | null;
  rulesSuffix: string;
  rulesHead: string;
  mainFile: string | null;
  projectRules: string | null;
  projectHead: string;
  projectFile: string | null;
}

export interface ShimFile {
  name: string;
  path: string;
  tool: ToolId | null;
  exists: boolean;
}

export interface HookStatus {
  shell: string;
  label: string;
  scriptReady: boolean;
  script: string;
  installed: boolean;
  detected: boolean;
  rcFile: string | null;
  note: string | null;
}

export interface Guess {
  file: string;
  fileName: string;
  sizeBytes: number;
  tool: ToolId | null;
  toolLabel: string | null;
  version: string | null;
  kind: string;
  sidecarSha256: string | null;
  platformNote: string | null;
  ok: boolean;
  whyNot: string | null;
}

export interface Layout {
  root: string;
  installs: string;
  shims: string;
  cache: string;
  partial: string;
  backups: string;
  config: string;
  logs: string;
  tmp: string;
  besideExe: boolean;
}

export interface Settings {
  sources: Record<string, string>;
  customBase: Record<string, string>;
  proxyMode: ProxyMode;
  proxyUrl: string | null;
  autostart: boolean;
  trayBehavior: TrayBehavior;
  pathScope: PathScope;
  shimPriority: boolean;
  lockGlobal: boolean;
  autoSwitch: boolean;
  keepArchives: boolean;
  downloadConns: number;
  theme: Theme;
  language: Language;
  lastProjectDir: string | null;
  schemaVersion: number;
}

export interface DataRootCandidate {
  /** 后端给的稳定标识：exe / drive / programData / home，界面按它查词条。 */
  id: string;
  dir: string;
  label: string;
  rank: number;
  note: string | null;
}

export interface Boot {
  version: string;
  platform: string;
  needsSetup: boolean;
  layout: Layout | null;
  candidates: DataRootCandidate[];
  settings: Settings;
  path: PathStatus[];
  elevated: boolean;
  jobs: JobView[];
  hooks: HookStatus[];
  shims: ShimFile[];
  note: string | null;
}

export interface VersionPage {
  lines: VersionLine[];
  flat: AvailableVersion[];
  sourceId: string;
  sourceLabel: string;
  fetchedAt: string | null;
  staleNote: string | null;
  installed: InstalledVersion[];
  defaultVersion: string | null;
}

export interface InstalledPage {
  tool: ToolId;
  versions: InstalledVersion[];
  defaultVersion: string | null;
  resolution: Resolution;
}

export interface Adopted {
  installed: InstalledVersion;
  referenced: boolean;
  note: string;
}

export interface Switched {
  tool: ToolId;
  version: string;
  resolution: Resolution;
  note: string | null;
}

export interface DirContext {
  dir: string;
  exists: boolean;
  lock: ProjectLock | null;
  resolutions: Resolution[];
  summary: string[];
}
