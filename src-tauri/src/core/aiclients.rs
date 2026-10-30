//! AI 编码客户端：检测本机装过哪些，以及把规则写到哪里。
//!
//! Rust 只回事实（在不在、在哪个目录、哪种写法能生效），界面文字一律留在前端的语言目录里。
//! 写入这条线有两个落点：我们自己生成的独立文件（默认，绝不碰用户配好的说明文件），
//! 和用户主说明文件里被标记包住的一段（只在用户明确勾选时才动）。

use std::fs;
use std::path::{Path, PathBuf};

use serde::Serialize;

use super::error::{Error, Result};

const MARK_START: &str = "<!-- envswitch:rules:start -->";
const MARK_END: &str = "<!-- envswitch:rules:end -->";

/// 规则文件的落点按根分成两类，检测装在哪个盘和规则写在哪是两件事，分开记。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Base {
    /// 家目录（USERPROFILE，跟着用户走，不一定在 C 盘）。
    Home,
    /// Roaming 配置目录。
    Config,
}

/// 一个客户端的探测与写入描述。字段都允许空：空就是"这条路没有"，不猜。
/// 每条不用全写，没写的走 `SPEC_DEFAULT`（只当"这条路没有"）。
#[derive(Clone, Copy)]
struct Spec {
    id: &'static str,
    /// 家目录下的相对路径。
    home: &'static [&'static str],
    config: &'static [&'static str],
    local: &'static [&'static str],
    /// Packages 下的目录名前缀（Store 应用带一串后缀哈希）。
    package: &'static [&'static str],
    /// 卸载登记里 DisplayName 命中这些词就算装过，顺便拿安装位置——装在哪个盘都跑不掉。
    needle: &'static [&'static str],
    /// Program Files 下的安装目录名，逐盘找：装在 D 盘还是 F 盘都用这一条。
    program: &'static [&'static str],
    /// 全局自动加载的规则目录：往里放一个文件就生效，不用碰用户的说明文件。
    rules: Option<(Base, &'static str)>,
    /// 规则目录里文件名的后缀，比如 Cursor 只认 `.mdc`。
    rules_suffix: &'static str,
    /// 全局规则文件开头必须带的头部（不写就不生效），没有就留空串。
    rules_head: &'static str,
    /// 全局主说明文件：只有它这种客户端才需要走"追加一段"。
    main: Option<(Base, &'static str, &'static str)>,
    /// 项目里自动加载的规则目录（相对项目根，用 / 写）。
    project_rules: Option<&'static str>,
    /// 项目规则文件开头必须带的头部，没有就留空串。
    project_head: &'static str,
    /// 项目里唯一会被读走的说明文件名，比如 AGENTS.md。
    project_file: Option<&'static str>,
}

const SPEC_DEFAULT: Spec = Spec {
    id: "",
    home: &[],
    config: &[],
    local: &[],
    package: &[],
    needle: &[],
    program: &[],
    rules: None,
    rules_suffix: ".md",
    rules_head: "",
    main: None,
    project_rules: None,
    project_head: "",
    project_file: None,
};

macro_rules! spec {
    ($id:literal, $($field:ident = $value:expr),* $(,)?) => {
        Spec { id: $id, $($field: $value),*, ..SPEC_DEFAULT }
    };
}

/// 规则文件开头的头部：这些客户端只认带特定头部的文件，少了就等于没写。
const HEAD_ALWAYS_APPLY: &str = "---\nalwaysApply: true\n---\n";
const HEAD_ACTIVATION_ALWAYS_ON: &str = "---\nactivation: always_on\n---\n";
const HEAD_INCLUSION_ALWAYS: &str = "---\ninclusion: always\n---\n";
const HEAD_APPLY_TO_ALL: &str = "---\napplyTo: \"**\"\n---\n";

const SPECS: &[Spec] = &[
    spec!("claude", home = &[".claude"], config = &["Claude"], local = &["Claude"], package = &[],
        needle = &["claude code", "claude"], program = &["Claude"],
        rules = None, main = Some((Base::Home, ".claude", "CLAUDE.md")),
        project_rules = Some(".claude/rules"), project_file = Some("CLAUDE.md")),
    spec!("codex", home = &[".codex"], config = &["Codex"], local = &["Codex"], package = &[],
        needle = &["codex"], program = &[],
        rules = None, main = Some((Base::Home, ".codex", "AGENTS.md")),
        project_rules = None, project_file = Some("AGENTS.md")),
    spec!("gemini", home = &[".gemini"], config = &["Gemini"], local = &[], package = &[],
        needle = &["gemini"], program = &[],
        rules = None, main = Some((Base::Home, ".gemini", "GEMINI.md")),
        project_rules = None, project_file = Some("GEMINI.md")),
    spec!("qwen", home = &[".qwen"], config = &["Qwen"], local = &["Programs/Qwen", "Programs/千问"], package = &["Qwen", "Tongyi", "AlibabaCloud.Qwen"],
        needle = &["千问", "qwen code", "qwencode", "qwen"], program = &["Qwen", "千问"],
        rules = None, main = Some((Base::Home, ".qwen", "QWEN.md")),
        project_rules = None, project_file = Some("QWEN.md")),
    spec!("dsh", home = &[".dsh"], config = &["@deepseek-ai"], local = &["@deepseek-aidsh-desktop-updater"], package = &[],
        needle = &["deepseek harness", "deepseek"], program = &["deepseek"],
        rules = None, main = Some((Base::Home, ".dsh", "AGENTS.md")),
        project_rules = None, project_file = Some("AGENTS.md")),
    spec!("opencode", home = &[".config/opencode", ".opencode"], config = &["opencode"], local = &[], package = &[],
        needle = &["opencode"], program = &[],
        rules = None, main = Some((Base::Home, ".config/opencode", "AGENTS.md")),
        project_rules = None, project_file = Some("AGENTS.md")),
    spec!("iflow", home = &[".iflow"], config = &["iflow"], local = &[], package = &[],
        needle = &["iflow"], program = &[],
        rules = None, main = Some((Base::Home, ".iflow", "IFLOW.md")),
        project_rules = None, project_file = Some("IFLOW.md")),
    spec!("amp", home = &[".config/amp"], config = &["amp"], local = &[], package = &[],
        needle = &["amp code", "sourcegraph amp"], program = &[],
        rules = None, main = Some((Base::Home, ".config/amp", "AGENTS.md")),
        project_rules = None, project_file = Some("AGENTS.md")),
    spec!("goose", home = &[".config/goose"], config = &["Block"], local = &["Block"], package = &[],
        needle = &["goose"], program = &[],
        rules = None, main = None, project_rules = None, project_file = Some("AGENTS.md")),
    spec!("cursor", home = &[".cursor"], config = &["Cursor"], local = &["Cursor", "Programs/Cursor"], package = &[],
        needle = &["cursor"], program = &["Cursor"],
        rules = Some((Base::Home, ".cursor/rules")), rules_suffix = ".mdc", rules_head = HEAD_ALWAYS_APPLY,
        main = None, project_rules = Some(".cursor/rules"), project_head = HEAD_ALWAYS_APPLY,
        project_file = Some("AGENTS.md")),
    spec!("windsurf", home = &[".codeium/windsurf"], config = &["Windsurf"], local = &["Windsurf", "Programs/Windsurf", "Programs/Windsurf CN"], package = &[],
        needle = &["windsurf"], program = &["Windsurf"],
        rules = None, main = Some((Base::Home, ".codeium/windsurf/memories", "global_rules.md")),
        project_rules = Some(".windsurf/rules"), project_head = HEAD_ACTIVATION_ALWAYS_ON,
        project_file = Some("AGENTS.md")),
    spec!("trae", home = &[".trae", ".trae-cn", ".trae-aicc"], config = &["Trae", "Trae CN"], local = &["Trae", "Trae CN", "Programs/Trae", "Programs/Trae CN"], package = &[],
        needle = &["trae", "traecode"], program = &["Trae", "Trae CN"],
        rules = None, main = None,
        project_rules = Some(".trae/rules"), project_file = Some("AGENTS.md")),
    spec!("qoder", home = &[".qoder"], config = &["Qoder"], local = &["Qoder"], package = &[],
        needle = &["qoder"], program = &["Qoder"],
        rules = None, main = None,
        project_rules = Some(".qoder/rules"), project_file = Some("AGENTS.md")),
    spec!("lingma", home = &[".lingma"], config = &["Lingma"], local = &["Lingma"], package = &[],
        needle = &["lingma", "通义灵码", "灵码"], program = &[],
        rules = None, main = None,
        project_rules = Some(".lingma/rules"), project_file = Some("AGENTS.md")),
    spec!("codebuddy", home = &[".codebuddy"], config = &["CodeBuddy CN", "CodeBuddy", "WorkBuddy AI"], local = &["CodeBuddyExtension", "Programs/CodeBuddy", "Programs/WorkBuddy"], package = &[],
        needle = &["codebuddy", "workbuddy"], program = &[],
        rules = None, main = None,
        project_rules = Some(".codebuddy/rules"), project_file = Some("AGENTS.md")),
    spec!("continue", home = &[".continue"], config = &["Continue"], local = &[], package = &[],
        needle = &["continue dev"], program = &[],
        rules = None, main = None,
        project_rules = Some(".continue/rules"), project_head = HEAD_ALWAYS_APPLY,
        project_file = Some("AGENTS.md")),
    spec!("cline", home = &[".cline"], config = &["Cline"], local = &["Cline"], package = &[],
        needle = &["cline"], program = &[],
        rules = Some((Base::Home, ".cline/rules")), main = None,
        project_rules = Some(".clinerules"), project_file = Some("AGENTS.md")),
    spec!("kilo", home = &[".kilo", ".config/kilo", ".kilocode"], config = &["kilo", "Kilo Code"], local = &[], package = &[],
        needle = &["kilo code"], program = &[],
        rules = None, main = None,
        project_rules = Some(".kilo/rules"), project_file = Some("AGENTS.md")),
    spec!("kiro", home = &[".kiro"], config = &["Kiro"], local = &["Kiro", "Programs/Kiro"], package = &[],
        needle = &["kiro"], program = &["Kiro"],
        rules = Some((Base::Home, ".kiro/steering")), rules_head = HEAD_INCLUSION_ALWAYS, main = None,
        project_rules = Some(".kiro/steering"), project_head = HEAD_INCLUSION_ALWAYS,
        project_file = Some("AGENTS.md")),
    spec!("copilot", home = &[".copilot"], config = &["Copilot"], local = &["github-copilot"], package = &[],
        needle = &["github copilot", "copilot"], program = &[],
        rules = None, main = Some((Base::Home, ".copilot", "copilot-instructions.md")),
        project_rules = Some(".github/instructions"), rules_suffix = ".instructions.md",
        project_head = HEAD_APPLY_TO_ALL, project_file = None),
    spec!("junie", home = &[".junie"], config = &["Junie"], local = &["Junie"], package = &[],
        needle = &["junie"], program = &[],
        rules = None, main = Some((Base::Home, ".junie", "AGENTS.md")),
        project_rules = Some(".junie"), project_file = Some("AGENTS.md")),
    spec!("zed", home = &[".config/zed"], config = &["Zed"], local = &["Zed", "Programs/Zed"], package = &[],
        needle = &["zed editor"], program = &["Zed"],
        rules = None, main = Some((Base::Config, "Zed", "AGENTS.md")),
        project_rules = None, project_file = Some("AGENTS.md")),
    spec!("factory", home = &[".factory"], config = &["Factory"], local = &["Factory"], package = &[],
        needle = &["factory droid"], program = &[],
        rules = None, main = Some((Base::Home, ".factory", "AGENTS.md")),
        project_rules = None, project_file = Some("AGENTS.md")),
    spec!("aider", home = &[".aider"], config = &["aider"], local = &[], package = &[],
        needle = &["aider"], program = &[],
        rules = None, main = None, project_rules = None, project_file = None),
    spec!("doubao", home = &[".doubao"], config = &["Doubao", "豆包"], local = &["Doubao", "豆包", "Programs/Doubao", "Programs/豆包"], package = &["ByteDance.Doubao", "Doubao"],
        needle = &["豆包", "doubao"], program = &["Doubao"],
        rules = None, main = None, project_rules = None, project_file = None),
    spec!("qingjian", home = &[".zhipu"], config = &["Qingjian"], local = &["Qingjian", "Programs/Qingjian", "Programs/智谱清言"], package = &["Zhipu.Qingyan", "Zhipu"],
        needle = &["智谱", "清言", "chatglm"], program = &[],
        rules = None, main = None, project_rules = None, project_file = None),
    spec!("cherry", home = &[".cherrystudio"], config = &["CherryStudio"], local = &["CherryStudio", "Programs/CherryStudio", "Programs/Cherry Studio"], package = &[],
        needle = &["cherry studio"], program = &["Cherry Studio"],
        rules = None, main = None, project_rules = None, project_file = None),
    spec!("chatgpt", home = &[".chatgpt"], config = &["ChatGPT"], local = &["ChatGPT", "Programs/ChatGPT"], package = &["OpenAI.ChatGPT"],
        needle = &["chatgpt"], program = &["ChatGPT"],
        rules = None, main = None, project_rules = None, project_file = None),
    spec!("ollama", home = &[".ollama"], config = &["Ollama"], local = &["Programs/Ollama", "Ollama"], package = &[],
        needle = &["ollama"], program = &["Ollama"],
        rules = None, main = None, project_rules = None, project_file = None),
    spec!("lmstudio", home = &[".lmstudio"], config = &["LM Studio"], local = &["LM-Studio", "Programs/LM Studio", "Programs/LMStudio"], package = &[],
        needle = &["lm studio", "lmstudio"], program = &["LM Studio"],
        rules = None, main = None, project_rules = None, project_file = None),
];

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AiClient {
    pub id: &'static str,
    pub found: bool,
    /// 配置目录：规则要往这儿附近放。没找到配置目录、只找到安装位置的，这里留空。
    pub path: Option<String>,
    /// 实际装在哪个盘：从卸载登记或各盘的 Program Files 里读出来的，可能在 D 盘以外。
    pub install: Option<String>,
    /// 有它能写：要么有全局规则目录，要么有主说明文件，要么项目里有落点。假的就是纯聊天客户端，只能进应用里设。
    pub writable: bool,
    /// 全局自动加载的规则目录；有它就能扔一个自己的文件进去，不用动用户的说明文件。
    pub rules_dir: Option<String>,
    /// 规则目录里文件名的后缀（`.md` / `.mdc` / `.instructions.md`）；全局和项目两个目录共用。
    pub rules_suffix: &'static str,
    /// 全局规则文件必须带的头部，空串就是不用带。
    pub rules_head: &'static str,
    /// 全局主说明文件全路径；只有它时才需要"往里加一段"。
    pub main_file: Option<String>,
    /// 项目内自动加载的规则目录（相对项目根，用 / 分隔）。
    pub project_rules: Option<&'static str>,
    /// 项目规则文件必须带的头部，空串就是不用带。
    pub project_head: &'static str,
    /// 项目内会被读走的唯一说明文件名，比如 AGENTS.md。
    pub project_file: Option<&'static str>,
}

fn has_write_target(s: &Spec) -> bool {
    s.rules.is_some() || s.main.is_some() || s.project_rules.is_some() || s.project_file.is_some()
}

fn base_dir(base: Base) -> Option<PathBuf> {
    match base {
        Base::Home => dirs::home_dir(),
        Base::Config => dirs::config_dir(),
    }
}

/// Packages 根目录：Store 应用都摊在这里，目录名带一串哈希后缀，所以按前缀匹配。
fn packages_root() -> Option<PathBuf> {
    dirs::data_dir().map(|d| d.join("Packages"))
}

fn first_existing(base: Option<PathBuf>, rels: &[&str]) -> Option<PathBuf> {
    let base = base?;
    rels
        .iter()
        .map(|r| base.join(r))
        .find(|p| p.is_dir())
}

fn package_hit(prefixes: &[&str]) -> Option<PathBuf> {
    if prefixes.is_empty() {
        return None;
    }
    let root = packages_root()?;
    let entries = fs::read_dir(&root).ok()?;
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_lowercase();
        if prefixes.iter().any(|p| name.starts_with(&p.to_lowercase())) && entry.path().is_dir() {
            return Some(entry.path());
        }
    }
    None
}

/// Program Files 的候选根：环境变量里的两个先试（它们本身可能就指向 D 盘），
/// 再补 D 到 Z 的 Program Files，装在哪个盘都能捞到。
fn program_roots() -> Vec<PathBuf> {
    let mut roots: Vec<PathBuf> = ["ProgramFiles(x86)", "ProgramFiles", "PROGRAMFILES"]
        .iter()
        .filter_map(|k| std::env::var(k).ok())
        .map(PathBuf::from)
        .collect();
    if cfg!(windows) {
        for letter in b'D'..=b'Z' {
            let drive = PathBuf::from(format!("{}:\\", letter as char));
            if !drive.is_dir() {
                continue;
            }
            roots.push(drive.join("Program Files"));
            roots.push(drive.join("Program Files (x86)"));
        }
    }
    roots
}

/// 在给定的一批根里找安装目录名。拆出来是为了能拿临时目录验证"换盘也认得"。
fn find_in_program_roots(roots: &[PathBuf], names: &[&str]) -> Option<PathBuf> {
    for root in roots {
        for name in names {
            let p = root.join(name);
            if p.is_dir() {
                return Some(p);
            }
        }
    }
    None
}

/// 逐盘翻 Program Files：装在 D 盘还是 F 盘都用这一条。
fn program_hit(names: &[&str]) -> Option<PathBuf> {
    if names.is_empty() {
        return None;
    }
    find_in_program_roots(&program_roots(), names)
}

/// 卸载登记里的安装位置：装在哪个盘、装没装过，这里最准。整张表读一次就缓存住。
#[cfg(windows)]
fn registry_hit(needles: &[&str]) -> Option<PathBuf> {
    use std::sync::OnceLock;
    use winreg::enums::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, KEY_READ};
    use winreg::RegKey;

    static TABLE: OnceLock<Vec<(String, Option<PathBuf>)>> = OnceLock::new();
    if needles.is_empty() {
        return None;
    }
    let table = TABLE.get_or_init(|| {
        let mut out = Vec::new();
        let hives = [
            (
                HKEY_LOCAL_MACHINE,
                r"SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall",
            ),
            (
                HKEY_LOCAL_MACHINE,
                r"SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall",
            ),
            (HKEY_CURRENT_USER, r"Software\Microsoft\Windows\CurrentVersion\Uninstall"),
        ];
        for (hive, sub) in hives {
            let Ok(key) = RegKey::predef(hive).open_subkey_with_flags(sub, KEY_READ) else {
                continue;
            };
            for name in key.enum_keys().filter_map(|r| r.ok()) {
                let Ok(item) = key.open_subkey_with_flags(&name, KEY_READ) else {
                    continue;
                };
                let display: String = item.get_value("DisplayName").unwrap_or_default();
                if display.trim().is_empty() {
                    continue;
                }
                let loc = item
                    .get_value::<String, _>("InstallLocation")
                    .ok()
                    .filter(|s| !s.trim().is_empty());
                let icon = item.get_value::<String, _>("DisplayIcon").ok();
                let un = item.get_value::<String, _>("UninstallString").ok();
                out.push((display.to_lowercase(), install_of(loc, icon, un)));
            }
        }
        out
    });
    needles.iter().find_map(|needle| {
        table
            .iter()
            .find(|(display, _)| name_matches(display, needle))
            .and_then(|(_, path)| path.clone())
    })
}

/// DisplayName 整词命中才算：这台机器上卸载登记里的 "UltraEdit" 含 "trae"，
/// 按子串匹配会把 UltraEdit 认成 Trae。中日韩字没有词边界，所以只按 ASCII 字母数字判邻居。
fn name_matches(display: &str, needle: &str) -> bool {
    let hay = display.to_lowercase();
    let low = needle.to_lowercase();
    if low.is_empty() {
        return false;
    }
    let bytes = hay.as_bytes();
    let mut from = 0usize;
    while let Some(off) = hay[from..].find(&low) {
        let start = from + off;
        let end = start + low.len();
        let before_word = start > 0 && bytes[start - 1].is_ascii_alphanumeric();
        let after_word = end < bytes.len() && bytes[end].is_ascii_alphanumeric();
        if !(before_word || after_word) {
            return true;
        }
        from = end;
    }
    false
}

#[cfg(not(windows))]
fn registry_hit(_needles: &[&str]) -> Option<PathBuf> {
    None
}

/// 安装位置：优先登记好的目录，其次从图标/卸载串里的程序路径倒推上一层。
#[cfg(windows)]
fn install_of(loc: Option<String>, icon: Option<String>, un: Option<String>) -> Option<PathBuf> {
    if let Some(raw) = loc {
        let p = PathBuf::from(raw.trim().trim_matches('"').trim_end_matches('\\'));
        if p.is_dir() {
            return Some(p);
        }
    }
    // InstallLocation 十次有八次是空的，尤其 Squirrel 那种装到用户目录的安装方式；
    // 图标和卸载串里至少带着 exe 的全路径，从那儿取上一层更靠得住。
    for raw in [icon, un].into_iter().flatten() {
        if let Some(dir) = parent_of_exe(&raw) {
            return Some(dir);
        }
    }
    None
}

/// 从 `F:\Program Files\Qoder\uninstallerIcon.ico` 这类串里取出所在目录。
#[cfg(windows)]
fn parent_of_exe(raw: &str) -> Option<PathBuf> {
    let low = raw.to_lowercase();
    let at = [".exe", ".ico"].iter().filter_map(|s| low.find(s)).min()?;
    let end = at + 4;
    if !raw.is_char_boundary(end) {
        return None;
    }
    let head = raw[..end].trim().trim_matches('"').to_string();
    if head.is_empty() || head.to_lowercase().starts_with("msiexec") {
        return None;
    }
    let p = PathBuf::from(&head);
    if p.is_file() {
        return p.parent().map(|d| d.to_path_buf());
    }
    None
}

/// 装在哪：先查卸载登记（哪个盘都认），再逐盘翻 Program Files。
/// 只当"确实装过"的证据用；写规则还是得靠配置目录。
fn install_hit(spec: &Spec) -> Option<PathBuf> {
    registry_hit(spec.needle).or_else(|| program_hit(spec.program))
}

/// 配置目录：家目录、Roaming、LocalAppData、Store 应用目录挨个试。
fn config_hit(spec: &Spec) -> Option<PathBuf> {
    dirs::home_dir()
        .and_then(|h| first_existing(Some(h), spec.home))
        .or_else(|| first_existing(dirs::config_dir(), spec.config))
        .or_else(|| first_existing(dirs::data_dir(), spec.local))
        .or_else(|| package_hit(spec.package))
}

fn resolved_dir(base: Base, rel: &str) -> Option<String> {
    base_dir(base).map(|b| strip(&b.join(rel)))
}

fn strip(p: &Path) -> String {
    super::util::strip_verbatim(p).display().to_string()
}

pub fn detect() -> Vec<AiClient> {
    SPECS
        .iter()
        .map(|s| {
            let path = config_hit(s);
            let install = install_hit(s);
            AiClient {
                id: s.id,
                found: path.is_some() || install.is_some(),
                writable: has_write_target(s),
                path: path.as_ref().map(|p| strip(p)),
                install: install.as_ref().map(|p| strip(p)),
                rules_dir: s.rules.and_then(|(b, r)| resolved_dir(b, r)),
                rules_suffix: s.rules_suffix,
                rules_head: s.rules_head,
                main_file: s
                    .main
                    .and_then(|(b, dir, file)| base_dir(b).map(|base| strip(&base.join(dir).join(file)))),
                project_rules: s.project_rules,
                project_head: s.project_head,
                project_file: s.project_file,
            }
        })
        .collect()
}

/// 文件名只允许字母数字点横线下划线，避免把 dir 之外的路径拼进来。
fn safe_file_name(file: &str) -> Result<String> {
    let name = file.trim();
    let ok = !name.is_empty()
        && !name.contains("..")
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'));
    if !ok {
        return Err(Error::Detail(format!("文件名不合规矩：{name}")));
    }
    Ok(name.to_string())
}

/// 写我们自己的文件：整个文件都是这次生成的内容，重写就是把上一次的覆盖掉。
/// `head` 是客户端要求的文件头部（比如 Cursor 的 `alwaysApply: true`），原样放在第一行。
/// 目录不存在就建出来：项目里 `.cursor/rules` 这类目录常常还没生成，等用户手动建太不友好。
pub fn write_standalone(dir: &str, file: &str, head: &str, content: &str) -> Result<String> {
    let name = safe_file_name(file)?;
    let base = Path::new(dir.trim().trim_matches('"'));
    let base = super::util::strip_verbatim(base);
    if base.as_os_str().is_empty() {
        return Err(Error::Detail("还没选好往哪个目录写".to_string()));
    }
    if !base.is_absolute() {
        return Err(Error::Detail(format!("得给完整路径才行：{}", base.display())));
    }
    if !base.is_dir() {
        fs::create_dir_all(&base)
            .map_err(|e| Error::Io(format!("建 {} 失败：{e}", base.display())))?;
    }
    let target = base.join(&name);
    if target.is_file() {
        let old = fs::read_to_string(&target).unwrap_or_default();
        if !old.trim().is_empty() && !is_ours(&old) {
            // 同名文件不是我们写的：不能拿人家的内容换掉。
            return Err(Error::Detail(format!(
                "{} 里已经有你写的内容，换个文件名或者让我们追加到说明文件里",
                name
            )));
        }
    }
    let body = format!("{head}{OURS_MARK}\n{content}\n");
    fs::write(&target, body).map_err(|e| Error::Io(format!("写 {} 失败：{e}", name)))?;
    Ok(strip(&target))
}

/// 独立文件头上的记号：既是署名，也是"这份可以整份重写/删掉"的依据。
const OURS_MARK: &str = "<!-- 由 灵匣·EnvSwitch 生成，删掉这个文件即可撤销。 -->";

/// 客户端要求的头部必须占第一行，否则整份文件都不生效。
fn is_ours(text: &str) -> bool {
    text.lines().take(12).any(|l| l.contains("灵匣·EnvSwitch 生成"))
}

/// 撤掉我们写过的那份独立文件：只删带记号的，用户自己写的同名文件碰都不碰。
pub fn remove_standalone(dir: &str, file: &str) -> Result<String> {
    let name = safe_file_name(file)?;
    let target = super::util::strip_verbatim(Path::new(dir.trim().trim_matches('"'))).join(&name);
    if !target.is_file() {
        return Ok(strip(&target));
    }
    let old = fs::read_to_string(&target).unwrap_or_default();
    if !is_ours(&old) {
        return Err(Error::Detail(format!(
            "{} 不是我们写的，不敢删",
            name
        )));
    }
    fs::remove_file(&target).map_err(|e| Error::Io(format!("删 {} 失败：{e}", name)))?;
    Ok(strip(&target))
}

/// 把规则块写进 dir/file：已有同名块就整块替换，没有就追加，绝不覆盖别人的内容。
pub fn write_rules(dir: &str, file: &str, content: &str) -> Result<String> {
    let name = safe_file_name(file)?;
    let base = Path::new(dir);
    if !base.exists() || !base.is_dir() {
        return Err(Error::Detail(format!("目录不存在或打不开：{dir}")));
    }
    let target = base.join(&name);
    let block = format!("{MARK_START}\n{}\n{MARK_END}", content.trim_end());
    let body = match fs::read_to_string(&target) {
        Ok(old) => {
            if let (Some(s), Some(e)) = (old.find(MARK_START), old.find(MARK_END)) {
                if e < s {
                    return Err(Error::Detail(format!("{} 里的标记顺序不对", name)));
                }
                let mut out = String::new();
                out.push_str(&old[..s]);
                out.push_str(&block);
                out.push_str(&old[e + MARK_END.len()..]);
                out
            } else {
                let mut out = old.trim_end().to_string();
                if !out.is_empty() {
                    out.push_str("\n\n");
                }
                out.push_str(&block);
                out.push('\n');
                out
            }
        }
        Err(_) => format!("{block}\n"),
    };
    fs::write(&target, body).map_err(|e| Error::Detail(format!("写 {} 失败：{e}", name)))?;
    Ok(target.display().to_string())
}

/// 把标记包住的那段撤走：只删我们自己那一段，用户写的内容一个字不动。
pub fn remove_rules(dir: &str, file: &str) -> Result<String> {
    let name = safe_file_name(file)?;
    let target = super::util::strip_verbatim(Path::new(dir)).join(&name);
    if !target.is_file() {
        return Ok(strip(&target));
    }
    let old = fs::read_to_string(&target).unwrap_or_default();
    let (s, e) = match (old.find(MARK_START), old.find(MARK_END)) {
        (Some(s), Some(e)) if e > s => (s, e),
        _ => return Ok(strip(&target)),
    };
    let head = old[..s].trim_end();
    let tail = old[e + MARK_END.len()..].trim_matches('\n');
    let body = match (head.is_empty(), tail.is_empty()) {
        (true, true) => String::new(),
        (true, false) => format!("{tail}\n"),
        (false, true) => format!("{head}\n"),
        (false, false) => format!("{head}\n\n{tail}\n"),
    };
    fs::write(&target, body).map_err(|e2| Error::Io(format!("改 {} 失败：{e2}", name)))?;
    Ok(strip(&target))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_name_rejects_traversal() {
        assert!(safe_file_name("../evil").is_err());
        assert!(safe_file_name("a/b.md").is_err());
        assert!(safe_file_name(".cursorrules").is_ok());
        assert!(safe_file_name("CLAUDE.md").is_ok());
    }

    #[test]
    fn block_replaces_itself_only() {
        let dir = std::env::temp_dir().join(format!("envswitch-ai-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let target = dir.join("AGENTS.md");
        fs::write(&target, "我的项目说明\n").unwrap();
        let first = write_rules(dir.to_str().unwrap(), "AGENTS.md", "规则一").unwrap();
        assert_eq!(Path::new(&first), target.as_path());
        let txt = fs::read_to_string(&target).unwrap();
        assert!(txt.starts_with("我的项目说明"));
        assert!(txt.contains("规则一"));
        write_rules(dir.to_str().unwrap(), "AGENTS.md", "规则二").unwrap();
        let txt = fs::read_to_string(&target).unwrap();
        assert!(!txt.contains("规则一"), "旧块没被替换掉：{txt}");
        assert_eq!(txt.matches(MARK_START).count(), 1);
        let _ = fs::remove_dir_all(&dir);
    }

    /// 卸载登记里名字花得狠：挨着 ASCII 字母数字的不算命中，挨着中日韩字的照旧算。
    #[test]
    fn registry_name_must_match_a_whole_word() {
        assert!(name_matches("Trae CN", "trae"));
        assert!(name_matches("Qoder", "QODER"));
        assert!(name_matches("LM Studio", "lm studio"));
        assert!(name_matches("豆包桌面版", "豆包"));
        assert!(!name_matches("UltraEdit", "trae"), "子串匹配会把 UltraEdit 认成 Trae");
        assert!(!name_matches("ProgramData Helper", "rog"));
        assert!(!name_matches("", "claude"));
    }

    /// 撤销只带走标记里那一段：用户自己的说明留着，没有标记的文件原样不动。
    #[test]
    fn revoke_block_keeps_the_users_own_words() {
        let dir = std::env::temp_dir().join(format!("envswitch-ai-rules-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let target = dir.join("AGENTS.md");
        fs::write(&target, "我的项目说明\n别的一段也要留着\n").unwrap();
        write_rules(dir.to_str().unwrap(), "AGENTS.md", "规则一").unwrap();
        remove_rules(dir.to_str().unwrap(), "AGENTS.md").unwrap();
        let txt = fs::read_to_string(&target).unwrap();
        assert!(txt.contains("我的项目说明") && txt.contains("别的一段也要留着"), "把用户的删了：{txt}");
        assert!(!txt.contains("规则一") && !txt.contains(MARK_START), "那段没撤干净：{txt}");
        fs::write(dir.join("OTHER.md"), "别人写的\n").unwrap();
        remove_rules(dir.to_str().unwrap(), "OTHER.md").unwrap();
        assert_eq!(fs::read_to_string(dir.join("OTHER.md")).unwrap(), "别人写的\n");
        let _ = fs::remove_dir_all(&dir);
    }

    /// 这台机器装了什么不算数，但每条都得报全：报了位置的必须是真实存在的目录，
    /// "装过"必须正好等于"配置目录或安装位置至少找到一个"。
    #[test]
    fn detect_reports_every_client_once() {
        let list = detect();
        assert_eq!(list.len(), SPECS.len());
        for spec in SPECS {
            assert_eq!(list.iter().filter(|c| c.id == spec.id).count(), 1, "{} 出现多次", spec.id);
        }
        for c in &list {
            assert_eq!(
                c.found,
                c.path.is_some() || c.install.is_some(),
                "{} 的 found 和位置对不上",
                c.id
            );
            for p in [c.path.as_ref(), c.install.as_ref()].into_iter().flatten() {
                assert!(Path::new(p).exists(), "{} 报了个不存在的位置：{p}", c.id);
            }
        }
    }

    /// 装在别的盘也要认得出来：这台机器上 DeepSeek Harness 在 F 盘、Trae 在 E 盘。
    /// 报了位置的那些必须是真目录；而"能不能看到系统盘以外"由候选根决定，这里直接验候选根。
    #[test]
    fn reported_installs_all_exist() {
        let list = detect();
        for c in &list {
            if let Some(p) = &c.install {
                assert!(Path::new(p).is_dir(), "{} 报了个不存在的安装位置：{p}", c.id);
            }
            if let Some(p) = &c.path {
                assert!(Path::new(p).is_dir(), "{} 报了个不存在的配置目录：{p}", c.id);
            }
        }
    }

    /// 候选根要是只写死 C 盘，装在 D/E/F 盘的客户端就永远查不到。
    /// 这台机器有 D 盘才判，没有就跳过（换机器不等于代码坏了）。
    #[test]
    fn program_roots_cover_more_than_the_system_drive() {
        let system_drive = std::env::var("SystemDrive").unwrap_or_else(|_| "C:".to_string());
        let roots = program_roots();
        if !Path::new("D:\\").is_dir() {
            return;
        }
        let off_system = roots
            .iter()
            .filter(|p| {
                let s = p.to_string_lossy();
                s.len() > 2 && &s[1..2] == ":" && !s.to_uppercase().starts_with(&system_drive.to_uppercase())
            })
            .count();
        assert!(
            off_system > 0,
            "候选根全落在系统盘：非 C 盘的安装根本就没进清单， roots={roots:?}"
        );
    }

    /// 逐盘找这条路本身：拿临时目录冒充另一个盘的 Program Files，认不出来就是没走根列表。
    #[test]
    fn program_scan_uses_whichever_roots_it_is_given() {
        let root = std::env::temp_dir().join(format!("envswitch-ai-pf-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let fake = root.join("Fake Client");
        fs::create_dir_all(&fake).unwrap();
        let hit = find_in_program_roots(&[root.clone()], &["Fake Client", "Another Client"]);
        assert_eq!(hit.as_deref(), Some(fake.as_path()), "给了根目录也没找到");
        assert!(find_in_program_roots(&[root.clone()], &["没装的东西"]).is_none());
        let _ = fs::remove_dir_all(&root);
    }

    /// 独立文件只认自己的记号：别人已经写好的同名文件不许覆盖。
    #[test]
    fn standalone_never_overwrites_a_foreign_file() {
        let dir = std::env::temp_dir().join(format!("envswitch-ai-solo-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("ENVSWITCH.md"), "我自己写的东西\n").unwrap();
        let e = write_standalone(dir.to_str().unwrap(), "ENVSWITCH.md", "", "规则").unwrap_err();
        assert!(e.to_string().contains("已经有你写的内容"), "报错不像话：{e}");
        assert_eq!(fs::read_to_string(dir.join("ENVSWITCH.md")).unwrap(), "我自己写的东西\n");
        // 我们的文件可以反复重写，且带上撤销说明。
        fs::remove_file(dir.join("ENVSWITCH.md")).unwrap();
        write_standalone(dir.to_str().unwrap(), "ENVSWITCH.md", "", "规则一").unwrap();
        write_standalone(dir.to_str().unwrap(), "ENVSWITCH.md", "", "规则二").unwrap();
        let txt = fs::read_to_string(dir.join("ENVSWITCH.md")).unwrap();
        assert!(!txt.contains("规则一"), "没覆盖掉上一次：{txt}");
        assert!(txt.contains("规则二"));
        assert!(is_ours(&txt));
        let _ = fs::remove_dir_all(&dir);
    }

    /// 客户端要求的头部必须占第一行，否则 Cursor/Kiro 那种只认 frontmatter 的直接把文件当普通说明。
    #[test]
    fn standalone_puts_the_head_on_the_first_line() {
        let dir = std::env::temp_dir().join(format!("envswitch-ai-head-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let nested = dir.join("rules");
        let path = write_standalone(
            nested.to_str().unwrap(),
            "envswitch.mdc",
            HEAD_ALWAYS_APPLY,
            "规则正文",
        )
        .unwrap();
        let txt = fs::read_to_string(&path).unwrap();
        assert!(txt.starts_with("---\nalwaysApply: true\n---\n"), "头部没放在最前：{txt}");
        assert!(is_ours(&txt), "头部后面的撤销记号丢了：{txt}");
        assert!(Path::new(&path).is_file());
        let _ = fs::remove_dir_all(&dir);
    }

    /// 规则目录还不存在时，写第一份要能把整条目录建出来（项目里的 .qoder/rules 常见）。
    #[test]
    fn standalone_creates_the_rules_dir_inside_an_existing_project() {
        let root = std::env::temp_dir().join(format!("envswitch-ai-proj-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        let nested = root.join(".qoder").join("rules");
        let path = write_standalone(nested.to_str().unwrap(), "envswitch.md", "", "规则").unwrap();
        assert!(Path::new(&path).is_file(), "没写出文件：{path}");
        let _ = fs::remove_dir_all(&root);
    }

    /// 相对路径不写：拿项目根拼出来的绝对路径才算数，否则不知道往哪儿建目录。
    #[test]
    fn standalone_refuses_a_relative_dir() {
        let e = write_standalone("some/relative/dir", "envswitch.md", "", "规则").unwrap_err();
        assert!(e.to_string().contains("完整路径"), "报错不像话：{e}");
    }

    /// 撤销只删自己写的：用户同名文件删不掉，删不存在的路径不算失败。
    #[test]
    fn remove_only_deletes_our_own_file() {
        let dir = std::env::temp_dir().join(format!("envswitch-ai-rm-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("ENVSWITCH.md"), "我自己写的东西\n").unwrap();
        let e = remove_standalone(dir.to_str().unwrap(), "ENVSWITCH.md").unwrap_err();
        assert!(e.to_string().contains("不是我们写的"), "报错不像话：{e}");
        assert!(dir.join("ENVSWITCH.md").is_file(), "别人的文件被删了");
        fs::remove_file(dir.join("ENVSWITCH.md")).unwrap();
        write_standalone(dir.to_str().unwrap(), "ENVSWITCH.md", "", "规则").unwrap();
        remove_standalone(dir.to_str().unwrap(), "ENVSWITCH.md").unwrap();
        assert!(!dir.join("ENVSWITCH.md").exists());
        remove_standalone(dir.to_str().unwrap(), "ENVSWITCH.md").unwrap();
        let _ = fs::remove_dir_all(&dir);
    }

    /// 每条 spec 都得给出至少一条写入路：全空的行等于用户勾了也白勾。
    /// 聊天类客户端是例外——它们根本没有能放进去的文件，说明书只能开在应用里。
    #[test]
    fn every_spec_offers_a_write_target() {
        const CHAT_ONLY: &[&str] = &[
            "doubao", "qingjian", "cherry", "chatgpt", "ollama", "lmstudio", "aider",
        ];
        for s in SPECS {
            if CHAT_ONLY.contains(&s.id) {
                assert!(!has_write_target(s), "{} 不该有写入落点", s.id);
                continue;
            }
            assert!(has_write_target(s), "{} 没有任何写入落点", s.id);
        }
    }
}
