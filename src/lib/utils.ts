import { clsx, type ClassValue } from "clsx";
import { twMerge } from "tailwind-merge";

import { t, type MsgKey } from "$lib/i18n/store.svelte";
import type { InstallSource, JobKind, JobPhase, PathScope, ResolutionSource, ToolId } from "./types";

export function cn(...inputs: ClassValue[]): string {
  return twMerge(clsx(inputs));
}

/** 官网与仓库：点品牌区开官网，点 GitHub 图标开仓库。 */
export const SITE_URL = "https://envswitch.iamap.cn";
export const REPO_URL = "https://github.com/lichuxin90/LingXia_EnvSwitch";

export const TOOL_LABEL: Record<ToolId, string> = {
  node: "Node.js",
  python: "Python",
  go: "Go",
  pypy: "PyPy",
};

/** 下载源等页面按工具逐个设置，四个都要露出来。 */
export const ALL_TOOLS: ToolId[] = ["node", "python", "go", "pypy"];

/** 侧边栏「运行时」单列出来的工具；PyPy 归在 Python 底下，不另开一项。 */
export const NAV_TOOLS: ToolId[] = ["node", "python", "go"];

/**
 * 下载源的名字由后端给出、写的是中文，界面按「工具 + 源 id」查词条。
 * 以后加了没登记的新源就照原文显示，不至于空白。
 */
const SOURCE_LABEL_KEY: Record<string, MsgKey> = {
  "node-official": "mir.srcNodeOfficial",
  "node-npmmirror": "mir.srcNodeNpmmirror",
  "python-github": "mir.srcPythonGithub",
  "python-gh-relay": "mir.srcPythonRelay",
  "go-official": "mir.srcGoOfficial",
  "go-google-cn": "mir.srcGoGoogleCn",
  "pypy-official": "mir.srcPypyOfficial",
};
const SOURCE_NOTE_KEY: Record<string, MsgKey> = {
  "node-npmmirror": "mir.noteNodeNpmmirror",
  "python-gh-relay": "mir.notePythonRelay",
  "go-google-cn": "mir.noteGoGoogleCn",
  "pypy-official": "mir.notePypyOfficial",
};
const REGION_KEY: Record<string, MsgKey> = {
  海外: "mir.regionOverseas",
  国内: "mir.regionDomestic",
  第三方: "mir.regionRelay",
};

const say = (map: Record<string, MsgKey>, key: string, fallback: string): string => {
  const msg = map[key];
  return msg ? t(msg) : fallback;
};

export const sourceName = (tool: ToolId, id: string, fallback: string): string =>
  say(SOURCE_LABEL_KEY, `${tool}-${id}`, fallback);

export const sourceNote = (tool: ToolId, id: string, fallback: string): string =>
  say(SOURCE_NOTE_KEY, `${tool}-${id}`, fallback);

export const regionName = (region: string): string => say(REGION_KEY, region, region);

/** Rust 的 consts::OS 是小写代号，界面上写厂商自己的写法。 */
const OS_NAME: Record<string, string> = { windows: "Windows", macos: "macOS", linux: "Linux" };

export const osName = (platform: string): string => OS_NAME[platform] ?? platform;

/** AI 客户端的显示名：产品名不翻译，四种界面语言里都写本名。id 与 Rust 那份清单一致。 */
export const AI_CLIENT_NAME: Record<string, string> = {
  claude: "Claude Code",
  codex: "Codex CLI",
  gemini: "Gemini CLI",
  qwen: "Qwen（千问）",
  dsh: "DeepSeek Harness",
  opencode: "OpenCode",
  iflow: "iFlow CLI",
  amp: "Amp",
  goose: "Goose",
  cursor: "Cursor",
  windsurf: "Windsurf",
  trae: "Trae",
  qoder: "Qoder",
  lingma: "通义灵码（Lingma）",
  codebuddy: "CodeBuddy",
  continue: "Continue",
  cline: "Cline",
  kilo: "Kilo Code",
  kiro: "Kiro",
  copilot: "GitHub Copilot",
  junie: "Junie",
  zed: "Zed",
  factory: "Factory Droid",
  aider: "Aider",
  doubao: "豆包",
  qingjian: "智谱清言",
  cherry: "Cherry Studio",
  chatgpt: "ChatGPT",
  ollama: "Ollama",
  lmstudio: "LM Studio",
};

/** 我们生成的独立文件统一叫这个名字，后缀按客户端认的那种（Cursor 只吃 .mdc）。 */
export function envswitchFile(suffix: string): string {
  return `envswitch${suffix || ".md"}`;
}

/** 项目根 + 相对路径拼成绝对路径；Windows 上分隔符统一成反斜杠，免得混着写。 */
export function joinDir(base: string, rel: string): string {
  const head = base.replace(/[\\/]+$/, "");
  const tail = rel.replace(/^[\\/]+/, "").split(/[\\/]/).filter(Boolean);
  if (!head) return tail.join("/");
  return head.includes("\\") || /^[a-zA-Z]:/.test(head)
    ? [head, ...tail].join("\\")
    : [head, ...tail].join("/");
}

/** 从文件全路径里取出所在目录，取不出就原样给回去。 */
export function dirOf(p: string): string {
  const i = Math.max(p.lastIndexOf("\\"), p.lastIndexOf("/"));
  return i > 0 ? p.slice(0, i) : p;
}

/** 从文件全路径里取出文件名。 */
export function fileNameOf(p: string): string {
  const i = Math.max(p.lastIndexOf("\\"), p.lastIndexOf("/"));
  return i >= 0 ? p.slice(i + 1) : p;
}

/** 同一个导航项里一起展示的工具（Python 那项带 PyPy 分支）。 */
export function siblings(tool: ToolId): ToolId[] {
  return tool === "python" ? ["python", "pypy"] : [tool];
}

/**
 * 下面几组标签都改成函数而不是常量表：常量在模块加载时就定死了语言，
 * 切成函数后每次取词都会读 i18n 的语言状态，换语言才会立刻重绘。
 * 只用于界面文字，Rust 侧的 job.label() 是给终端和托盘看的。
 */
const PHASE_KEYS: Record<JobPhase, MsgKey> = {
  queued: "label.phase.queued",
  planning: "label.phase.planning",
  downloading: "label.phase.downloading",
  verifying: "label.phase.verifying",
  extracting: "label.phase.extracting",
  linking: "label.phase.linking",
  done: "label.phase.done",
  failed: "label.phase.failed",
  cancelled: "label.phase.cancelled",
};

const KIND_KEYS: Record<JobKind, MsgKey> = {
  install: "label.kind.install",
  uninstall: "label.kind.uninstall",
  takeover: "label.kind.takeover",
  import: "label.kind.import",
};

const SOURCE_KEYS: Record<InstallSource, MsgKey> = {
  official: "label.source.official",
  mirror: "label.source.mirror",
  offline: "label.source.offline",
  takeover: "label.source.takeover",
};

const RESOLUTION_KEYS: Record<ResolutionSource, MsgKey> = {
  projectFile: "label.res.projectFile",
  globalDefault: "label.res.globalDefault",
  onlyInstalled: "label.res.onlyInstalled",
  none: "label.res.none",
};

const SCOPE_KEYS: Record<PathScope, MsgKey> = {
  user: "label.scope.user",
  system: "label.scope.system",
};

export function phaseLabel(phase: JobPhase): string {
  return t(PHASE_KEYS[phase]);
}

export function kindLabel(kind: JobKind): string {
  return t(KIND_KEYS[kind]);
}

export function sourceLabel(source: InstallSource): string {
  return t(SOURCE_KEYS[source]);
}

export function resolutionLabel(source: ResolutionSource): string {
  return t(RESOLUTION_KEYS[source]);
}

export function scopeLabel(scope: PathScope): string {
  return t(SCOPE_KEYS[scope]);
}

export function humanBytes(n: number | null | undefined): string {
  if (n === null || n === undefined) return t("fmt.sizeUnknown");
  const units = ["B", "KB", "MB", "GB", "TB"];
  let v = n;
  let i = 0;
  while (v >= 1024 && i < units.length - 1) {
    v /= 1024;
    i += 1;
  }
  if (i === 0) return `${n} B`;
  return `${v.toFixed(1)} ${units[i]}`;
}

export function humanRate(bytesPerSec: number): string {
  if (bytesPerSec <= 0) return t("fmt.rateComputing");
  return `${humanBytes(bytesPerSec)}${t("fmt.perSecond")}`;
}

export function humanEta(secs: number | null | undefined): string {
  if (secs === null || secs === undefined) return "";
  if (secs < 60) return t("fmt.etaSecs", { n: Math.max(1, Math.round(secs)) });
  return t("fmt.etaMins", { n: Math.round(secs / 60) });
}

/** 时间戳只截到分钟，界面不放一长串数字。 */
export function shortTime(iso: string | null | undefined): string {
  if (!iso) return "";
  const m = /^(\d{4})-(\d{2})-(\d{2})[T ](\d{2}):(\d{2})/.exec(iso);
  if (!m) return iso.slice(0, 16).replace("T", " ");
  return `${m[1]}-${m[2]}-${m[3]} ${m[4]}:${m[5]}`;
}

export function baseName(p: string | null | undefined): string {
  if (!p) return "";
  const parts = p.split(/[\\/]/);
  return parts[parts.length - 1] || p;
}

export function percentOf(received: number, total: number | null): number {
  if (!total || total <= 0) return 0;
  return Math.min(100, Math.round((received / total) * 100));
}
