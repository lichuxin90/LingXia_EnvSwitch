//! 环境体检：这台电脑上已经有什么、谁在跟 EnvSwitch 抢 PATH、已有的安装能不能直接接管。
//!
//! 扫描只读不写；接管由 adopt() 完成，默认只登记原目录、不动用户文件。
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::sync::OnceLock;
use std::time::Duration;

use serde::Serialize;

use super::archive;
use super::error::{Error, Result};
use super::model::{
    Conflict, ConflictKind, ForeignRuntime, InstallSource, InstalledVersion, ScanReport,
    TakeoverMode, ToolId,
};
use super::records::{Record, Registry};
use super::tool::Plugin;
use super::{install, paths, pathmod, shim, util};

/// 探测版本最多等这么久：node --version 一般在几十毫秒内返回。
const PROBE_TIMEOUT: Duration = Duration::from_millis(2500);

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PathView {
    /// 生效顺序排列的 PATH 条目
    pub entries: Vec<String>,
    /// 取自注册表（新开终端按它算）还是当前进程
    pub from: &'static str,
    pub shim_dir: PathBuf,
    pub shim_present: bool,
    pub shim_first: bool,
}

/// 已知的版本管理器：名称、属于哪类工具、会出现在 PATH 里的目录名片段。
const MANAGERS: &[(&str, &str, &[&str])] = &[
    ("nvm", "Node", &["nvm"]),
    ("fnm", "Node", &["fnm"]),
    ("volta", "Node", &["volta"]),
    ("pyenv", "Python", &["pyenv"]),
    ("scoop", "通用", &["scoop"]),
    (
        "conda",
        "Python",
        &["miniconda", "anaconda", "miniforge", "mambaforge", "micromamba"],
    ),
    ("mise", "通用", &["mise"]),
    ("asdf", "通用", &["asdf"]),
    ("uv", "Python", &["uv\\bin", "\\uv\\", "/uv/bin", "/uv/"]),
];

/// 每个工具在 PATH 里可能出现的命令名。
const RUNTIME_NAMES: &[(ToolId, &[&str])] = &[
    (ToolId::Node, &["node"]),
    (ToolId::Python, &["python", "python3"]),
    (ToolId::Go, &["go"]),
    (ToolId::PyPy, &["pypy", "pypy3"]),
];

pub fn path_view() -> Result<PathView> {
    let layout = paths::layout()?;
    let (entries, from) = effective_path()?;
    let index = entries.iter().position(|e| same_dir(e, &layout.shims));
    Ok(PathView {
        entries,
        from,
        shim_dir: layout.shims,
        shim_present: index.is_some(),
        shim_first: index == Some(0),
    })
}

/// 当前进程的 PATH 可能是启动时的旧快照，注册表里的才是新开终端拿到的。
fn effective_path() -> Result<(Vec<String>, &'static str)> {
    let session: Vec<String> = std::env::var_os("PATH")
        .map(|raw| util::split_path_list(&raw.to_string_lossy()))
        .unwrap_or_default();
    if let Some(mut merged) = registry_path() {
        if !merged.is_empty() {
            for e in &session {
                if !merged.iter().any(|m| same_dir(m, Path::new(e))) {
                    merged.push(e.clone());
                }
            }
            return Ok((merged, "registry"));
        }
    }
    Ok((session, "session"))
}

#[cfg(windows)]
fn registry_path() -> Option<Vec<String>> {
    use super::model::PathScope;
    let mut out = pathmod::read_raw(PathScope::System)
        .map(|r| util::split_path_list(&r))
        .unwrap_or_default();
    out.extend(
        pathmod::read_raw(PathScope::User)
            .map(|r| util::split_path_list(&r))
            .unwrap_or_default(),
    );
    Some(out)
}

#[cfg(not(windows))]
fn registry_path() -> Option<Vec<String>> {
    None
}

fn same_dir(raw: &str, path: &Path) -> bool {
    paths::norm_eq(Path::new(raw.trim_matches('"')), path)
}

/// 扫一遍环境，给出冲突清单和可接管的现成安装。
pub fn scan() -> Result<ScanReport> {
    let view = path_view()?;
    let mut report = ScanReport {
        conflicts: Vec::new(),
        foreign: Vec::new(),
        ours_shim_first: view.shim_first,
        notes: Vec::new(),
    };

    for entry in &view.entries {
        if let Some((name, scope)) = match_manager(entry) {
            report.conflicts.push(Conflict {
                name: name.to_string(),
                kind: ConflictKind::Manager,
                path: Some(PathBuf::from(entry)),
                detail: format!("PATH 里有 {entry}，那是 {name}（{scope}）的目录"),
                suggestion: format!(
                    "两个版本管理器同时生效会互相抢：要么把 {name} 从 PATH 去掉，要么在设置里关掉 EnvSwitch 的优先权"
                ),
            });
        }
    }

    let mut seen: Vec<PathBuf> = Vec::new();
    for &(tool, names) in RUNTIME_NAMES {
        for name in names {
            let Some(exe) = find_outside_shims(name, &view) else {
                continue;
            };
            if seen.iter().any(|s| same_dir(&s.display().to_string(), &exe)) {
                continue;
            }
            seen.push(exe.clone());
            let item = describe(tool, &exe);
            report.conflicts.push(Conflict {
                name: tool.display().to_string(),
                kind: ConflictKind::Runtime,
                path: Some(exe),
                detail: format!(
                    "系统 PATH 里已经有一个 {}{}",
                    tool.display(),
                    item
                        .version
                        .as_deref()
                        .map(|v| format!(" {v}"))
                        .unwrap_or_default()
                ),
                suggestion:
                    "可以在体检页一键接管，之后终端里用哪个版本由 EnvSwitch 说了算".to_string(),
            });
            report.foreign.push(item);
        }
    }

    if !view.shim_present {
        report.notes.push(
            "转发目录还没进 PATH，终端里的 node、python 暂时轮不到 EnvSwitch 决定".to_string(),
        );
    } else if !view.shim_first {
        report.notes.push(
            "转发目录在 PATH 里但没排在最前，可能被别的同名程序抢先；设置里有一键提到最前".to_string(),
        );
    }
    dedup_conflicts(&mut report);
    Ok(report)
}

/// 认出 PATH 条目属于哪个管理器；我们自己的目录不算冲突。
fn match_manager(entry: &str) -> Option<(&'static str, &'static str)> {
    let lower = normalize(entry);
    if lower.contains("envswitch") {
        return None;
    }
    MANAGERS
        .iter()
        .find(|(_, _, keys)| keys.iter().any(|k| lower.contains(&normalize(k))))
        .map(|(name, scope, _)| (*name, *scope))
}

fn normalize(raw: &str) -> String {
    raw.replace('/', "\\").to_lowercase()
}

/// 找到 PATH 里第一个不在我们自己转发目录下的同名程序。
fn find_outside_shims(name: &str, view: &PathView) -> Option<PathBuf> {
    let exe_name = if cfg!(windows) {
        format!("{name}.exe")
    } else {
        name.to_string()
    };
    view.entries.iter().find_map(|entry| {
        if normalize(entry).contains("envswitch") {
            return None;
        }
        let cand = PathBuf::from(entry).join(&exe_name);
        cand.is_file().then_some(cand)
    })
}

fn describe(tool: ToolId, exe: &Path) -> ForeignRuntime {
    let plugin = Plugin::for_tool(tool).ok();
    let root = guess_root(exe);
    let complete = plugin
        .as_ref()
        .map(|p| root.join(p.exec_rel()).is_file())
        .unwrap_or(false)
        || exe_is_tool_entry(exe, plugin);
    let version = probe_version(exe);
    let manager = MANAGERS
        .iter()
        .find(|(_, _, keys)| {
            let lower = normalize(&exe.display().to_string());
            keys.iter().any(|k| lower.contains(&normalize(k)))
        })
        .map(|(name, _, _)| name.to_string());

    let (takeable, why_not) = if is_system_owned(exe) {
        (
            false,
            Some("这是系统包管理器装的，删掉会影响别的程序，建议留着别动".to_string()),
        )
    } else if version.is_none() {
        (
            false,
            Some("问不出它的版本号，先确认它能正常执行再加进来".to_string()),
        )
    } else if !complete {
        (
            false,
            Some("目录结构和 EnvSwitch 认的不一样，接进来也当不了完整版本".to_string()),
        )
    } else {
        (true, None)
    };

    ForeignRuntime {
        tool: Some(tool),
        name: exe
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default(),
        version,
        exe: exe.to_path_buf(),
        root,
        manager,
        takeable,
        why_not,
        // 默认引用原目录：接管老安装不重复占磁盘，也不会删用户的东西。
        mode: TakeoverMode::Reference,
    }
}

fn exe_is_tool_entry(exe: &Path, plugin: Option<Plugin>) -> bool {
    let Some(plugin) = plugin else {
        return false;
    };
    let stem = exe
        .file_stem()
        .map(|s| s.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    plugin.shims().iter().any(|s| s.name == stem)
}

/// Windows 上 node.exe 同级就是版本根；类 Unix 下 bin 的上一级才是根。
fn guess_root(exe: &Path) -> PathBuf {
    let parent = exe.parent().map(Path::to_path_buf).unwrap_or_default();
    if !cfg!(windows) && parent.file_name().and_then(|n| n.to_str()) == Some("bin") {
        return parent.parent().map(Path::to_path_buf).unwrap_or(parent);
    }
    parent
}

#[cfg(unix)]
fn is_system_owned(exe: &Path) -> bool {
    const PREFIXES: &[&str] = &["/usr/bin", "/usr/sbin", "/bin", "/usr/local/bin", "/sbin"];
    let text = exe.display().to_string();
    PREFIXES.iter().any(|p| text.starts_with(p))
}

#[cfg(not(unix))]
fn is_system_owned(_exe: &Path) -> bool {
    false
}

/// 跑一次 `--version` 问出版本号；卡住就放弃，不让体检把界面拖死。
fn probe_version(exe: &Path) -> Option<String> {
    use std::process::Stdio;
    let (tx, rx) = mpsc::channel();
    let path = exe.to_path_buf();
    std::thread::spawn(move || {
        let mut cmd = std::process::Command::new(&path);
        cmd.arg("--version").stdin(Stdio::null());
        #[cfg(windows)]
        {
            // 界面进程没有控制台，不挂这个标记，每探测一个运行时就闪出一个黑窗。
            use std::os::windows::process::CommandExt;
            const CREATE_NO_WINDOW: u32 = 0x0800_0000;
            cmd.creation_flags(CREATE_NO_WINDOW);
        }
        let r = cmd.output();
        let _ = tx.send(r);
    });
    let out = rx.recv_timeout(PROBE_TIMEOUT).ok()?.ok()?;
    let stdout = String::from_utf8_lossy(&out.stdout).trim().to_string();
    let text = if stdout.is_empty() {
        String::from_utf8_lossy(&out.stderr).trim().to_string()
    } else {
        stdout
    };
    parse_version(&text)
}

fn version_re() -> &'static regex::Regex {
    static RE: OnceLock<regex::Regex> = OnceLock::new();
    RE.get_or_init(|| regex::Regex::new(r"(\d+\.\d+(?:\.\d+)?)").expect("版本号正则始终合法"))
}

fn parse_version(text: &str) -> Option<String> {
    version_re()
        .captures(text)
        .map(|c| c[1].to_string())
        .or_else(|| {
            text.split_whitespace()
                .find(|w| w.chars().next().is_some_and(|c| c.is_ascii_digit()))
                .map(|w| w.trim_start_matches('v').to_string())
        })
}

/// 同一个位置只报一次，避免 nvm 在 PATH 里出现两次就报两条。
fn dedup_conflicts(report: &mut ScanReport) {
    let mut seen: Vec<String> = Vec::new();
    report.conflicts.retain(|c| {
        let key = format!(
            "{}|{}",
            c.name,
            c.path.as_ref().map(|p| p.display().to_string()).unwrap_or_default()
        );
        if seen.contains(&key) {
            false
        } else {
            seen.push(key);
            true
        }
    });
}

/// 把一份现成安装登记成某个版本：不下载，引用或复制两种落位方式。
pub fn adopt(
    tool: ToolId,
    exe: &Path,
    version: Option<&str>,
    copy: bool,
) -> Result<InstalledVersion> {
    let plugin = Plugin::for_tool(tool)?;
    let layout = paths::layout()?;
    let mut registry = Registry::load()?;
    let version = version
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
        .or_else(|| probe_version(exe))
        .ok_or_else(|| Error::Detail(format!("问不出 {} 的版本号，请手动补一个", exe.display())))?;
    if registry.find(tool, &version).is_some() {
        return Err(Error::AlreadyInstalled(
            tool.display().to_string(),
            version,
        ));
    }

    let root = guess_root(exe);
    let exec_rel = plugin.exec_rel();
    let source_exec = if root.join(&exec_rel).is_file() {
        root.join(&exec_rel)
    } else if exe.is_file() {
        exe.to_path_buf()
    } else {
        return Err(Error::NotFound(format!(
            "{} 里找不到 {}，接不进来",
            root.display(),
            exec_rel.display()
        )));
    };

    let (dir, referenced) = if copy {
        let target = layout.version_dir(tool.as_str(), &version);
        if target.exists() {
            util::remove_tree_strict(&target)?;
        }
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)?;
        }
        archive::copy_tree(&root, &target)
            .map_err(|e| e.with_detail(format!("把 {} 复制进数据目录", root.display())))?;
        (target, false)
    } else {
        (root.clone(), true)
    };

    let exec_path = if referenced {
        source_exec
    } else {
        dir.join(&exec_rel)
    };
    if !exec_path.is_file() {
        return Err(Error::Detail(format!(
            "登记完了却找不到 {}，这次接管算失败",
            exec_path.display()
        )));
    }

    let companions = plugin.companions(&dir);
    let record = Record {
        tool,
        size_bytes: if referenced { 0 } else { util::dir_size(&dir) },
        dir,
        exec_path,
        companions,
        version: version.clone(),
        sha256: None,
        source: InstallSource::Takeover,
        installed_at: chrono::Local::now().to_rfc3339(),
        origin: (!referenced).then(|| root.clone()),
        referenced,
    };
    let first_of_tool = registry.list(tool).is_empty();
    registry.put(record);
    if first_of_tool || registry.default_of(tool).is_none() {
        registry.set_default(tool, &version)?;
    }
    registry.save()?;
    shim::ensure_tool_shims(tool)?;

    install::installed(tool)?
        .into_iter()
        .find(|i| i.version == version)
        .ok_or_else(|| Error::Detail("接管记录写好了但读不回来".to_string()))
}
