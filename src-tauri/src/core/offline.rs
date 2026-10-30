//! 离线包导入：把别人拷来的官方压缩包（内网办公常见）直接装成受管版本。
//!
//! 认包靠文件名猜版本、靠文件头确认压缩格式，最后仍由解压那一步判断对不对版。
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};

use serde::Serialize;

use super::archive;
use super::download;
use super::error::{Error, Result};
use super::install;
use super::job::Job;
use super::model::{InstallSource, InstalledVersion, ToolId};
use super::paths;
use super::util;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Guess {
    pub file: PathBuf,
    pub file_name: String,
    pub size_bytes: u64,
    pub tool: Option<ToolId>,
    pub tool_label: Option<String>,
    pub version: Option<String>,
    /// "zip" / "tar.gz" / "other"
    pub kind: &'static str,
    /// 同目录下的 .sha256 边文件里带的校验值，导入时自动核对。
    pub sidecar_sha256: Option<String>,
    /// 包名里的平台标记和这台电脑不一致时的说明。
    pub platform_note: Option<String>,
    pub ok: bool,
    pub why_not: Option<String>,
}

impl Guess {
    fn unknown(path: &Path, why: &str) -> Self {
        Self {
            file: path.to_path_buf(),
            file_name: path
                .file_name()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_default(),
            size_bytes: 0,
            tool: None,
            tool_label: None,
            version: None,
            kind: "other",
            sidecar_sha256: None,
            platform_note: None,
            ok: false,
            why_not: Some(why.to_string()),
        }
    }
}

/// 认包：读文件名与文件头，给出能不能装的结论。
pub fn guess(path: &Path) -> Result<Guess> {
    let file_name = path
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default();
    if path.is_dir() {
        return Ok(Guess::unknown(
            path,
            "这是已经解压好的文件夹，请到环境体检里用接管现有安装",
        ));
    }
    let size_bytes = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
    if size_bytes == 0 {
        return Ok(Guess::unknown(path, "这个文件是空的，或者路径不对"));
    }

    let kind = kind_of(path)?;
    if kind == "other" {
        return Ok(Guess::unknown(
            path,
            "认不出压缩格式，只收官方发布的 .zip 或 .tar.gz",
        ));
    }

    let Some((tool, version)) = parse_name(&file_name) else {
        return Ok(Guess::unknown(
            path,
            "文件名里读不出版本，请保留官方原始文件名（形如 node-v20.18.0-win-x64.zip）",
        ));
    };

    let (ok, why_not) = if matches_platform(&file_name, tool) {
        (true, None)
    } else {
        (
            false,
            Some(format!(
                "这个包不是给这台电脑（{}）用的，装了也没法运行",
                platform_label()
            )),
        )
    };

    Ok(Guess {
        file: path.to_path_buf(),
        platform_note: source_package_note(&file_name),
        file_name,
        size_bytes,
        tool: Some(tool),
        tool_label: Some(tool.display().to_string()),
        version: Some(version),
        kind,
        sidecar_sha256: read_sidecar(path),
        ok,
        why_not,
    })
}

/// 界面拖入一批文件后先做预检，把每个文件的结论摊开展示。
pub fn precheck(files: &[PathBuf]) -> Vec<Guess> {
    files
        .iter()
        .map(|p| guess(p).unwrap_or_else(|e| Guess::unknown(p, &e.to_string())))
        .collect()
}

/// 压缩格式看文件头，不看扩展名：离线包经常被改名。
fn kind_of(path: &Path) -> Result<&'static str> {
    let head = archive::peek_first_bytes(path, 4)?;
    if head.starts_with(b"PK\x03\x04") {
        Ok("zip")
    } else if head.starts_with(b"\x1f\x8b") {
        Ok("tar.gz")
    } else if head.starts_with(b"BZh") {
        Ok("tar.bz2")
    } else {
        Ok("other")
    }
}

fn version_re() -> &'static regex::Regex {
    static RE: OnceLock<regex::Regex> = OnceLock::new();
    RE.get_or_init(|| regex::Regex::new(r"(\d+\.\d+\.\d+)").expect("版本号正则始终合法"))
}

fn parse_name(raw: &str) -> Option<(ToolId, String)> {
    use super::tool::pypy::{PyPyPlugin, parse_name as parse_pypy};
    let lower = raw.to_lowercase();
    // PyPy 的包名里没有 x.y.z 三段式版本号，按官方命名单独读。
    if lower.starts_with("pypy") {
        let (compat, release, _) = parse_pypy(&lower, PyPyPlugin::platform_token())?;
        return Some((ToolId::PyPy, format!("{compat}-{release}")));
    }
    let tool = if lower.contains("python") || lower.contains("cpython") {
        ToolId::Python
    } else if lower.contains("node") {
        ToolId::Node
    } else if lower.starts_with("go") && lower.as_bytes().get(2).is_some_and(|b| b.is_ascii_digit()) {
        ToolId::Go
    } else {
        return None;
    };
    let version = version_re().captures(&lower).map(|c| c[1].to_string())?;
    Some((tool, version))
}

/// 旁边放一个同名 `.sha256` 就自动核对；没放则只靠解压后的结构判断完整性。
fn read_sidecar(path: &Path) -> Option<String> {
    let text = std::fs::read_to_string(format!("{}.sha256", path.display())).ok()?;
    parse_sidecar(&text)
}

fn parse_sidecar(text: &str) -> Option<String> {
    let line = text.lines().map(str::trim).find(|l| !l.is_empty() && !l.starts_with('#'))?;
    let candidate = line
        .split_whitespace()
        .next()
        .unwrap_or_default()
        .trim_start_matches('*');
    let clean: String = candidate.chars().filter(|c| c.is_ascii_hexdigit()).collect();
    (clean.len() == 64).then(|| clean.to_lowercase())
}

/// 包名里的平台标记要和这台电脑对得上，否则装完也跑不起来。
fn matches_platform(file_name: &str, tool: ToolId) -> bool {
    let lower = file_name.to_lowercase();
    if lower.contains("source") {
        return false;
    }
    let tokens: &[&str] = match (tool, platform_label()) {
        (ToolId::Node, "windows-x64") => &["win-x64", "win-x86_64"],
        (ToolId::Node, "darwin-arm64") => &["darwin-arm64"],
        (ToolId::Node, "darwin-x64") => &["darwin-x64", "osx-x64"],
        (ToolId::Node, "linux-arm64") => &["linux-arm64"],
        (ToolId::Node, _) => &["linux-x64"],
        (ToolId::Python, "windows-x64") => &["x86_64-pc-windows-msvc"],
        (ToolId::Python, "darwin-arm64") => &["aarch64-apple-darwin"],
        (ToolId::Python, "darwin-x64") => &["x86_64-apple-darwin"],
        (ToolId::Python, "linux-arm64") => &["aarch64-unknown-linux-gnu"],
        (ToolId::Python, _) => &["x86_64-unknown-linux-gnu"],
        (ToolId::Go, "windows-x64") => &["windows-amd64"],
        (ToolId::Go, "darwin-arm64") => &["darwin-arm64"],
        (ToolId::Go, "darwin-x64") => &["darwin-amd64"],
        (ToolId::Go, "linux-arm64") => &["linux-arm64"],
        (ToolId::Go, _) => &["linux-amd64"],
        (ToolId::PyPy, "windows-x64") => &["win64"],
        (ToolId::PyPy, "darwin-arm64") => &["macos_arm64"],
        (ToolId::PyPy, "darwin-x64") => &["macos_x86_64", "osx64"],
        (ToolId::PyPy, "linux-arm64") => &["aarch64"],
        (ToolId::PyPy, _) => &["linux64"],
    };
    tokens.iter().any(|t| lower.contains(t))
}

pub fn platform_label() -> &'static str {
    if cfg!(windows) {
        "windows-x64"
    } else if cfg!(target_os = "macos") {
        if cfg!(target_arch = "aarch64") {
            "darwin-arm64"
        } else {
            "darwin-x64"
        }
    } else if cfg!(target_arch = "aarch64") {
        "linux-arm64"
    } else {
        "linux-x64"
    }
}

/// 源码包永远装不了，提前挡掉，别等解压失败再让用户猜原因。
fn source_package_note(file_name: &str) -> Option<String> {
    let lower = file_name.to_lowercase();
    (lower.contains("source") || lower.contains("-src")).then(|| "这像是源码包，不能直接装".to_string())
}

/// 装离线包：校验 → 解压 → 归位 → 登记。原文件保持不动。
pub async fn import(
    on_event: Option<download::OnEvent>,
    job: Arc<Job>,
    path: &Path,
    tool: Option<ToolId>,
    version: Option<&str>,
    sha256: Option<&str>,
) -> Result<InstalledVersion> {
    let g = guess(path)?;
    let tool = tool.or(g.tool).ok_or_else(|| {
        Error::Detail(
            g.why_not
                .clone()
                .unwrap_or_else(|| "认不出这个包属于哪个工具".to_string()),
        )
    })?;
    let version = version
        .map(|v| v.trim().to_string())
        .or_else(|| g.version.clone())
        .ok_or_else(|| Error::Detail("请补一个版本号".to_string()))?;
    let expected = sha256
        .map(|s| s.trim().to_lowercase())
        .filter(|s| !s.is_empty())
        .or_else(|| g.sidecar_sha256.clone());
    let total = Some(g.size_bytes);

    download::verify_file(path, expected.as_deref()).await?;

    let layout = paths::layout()?;
    let staging = layout.tmp.join(format!(
        "offline-{}-{}",
        util::safe_file_stem(&version),
        util::timestamp()
    ));
    let src = path.to_path_buf();
    let events = on_event.clone();
    let placed = {
        let job_ref = Arc::clone(&job);
        tokio::task::spawn_blocking(move || {
            install::place_from_archive(
                &src,
                &staging,
                tool,
                &version,
                expected,
                InstallSource::Offline,
                Some(src.clone()),
                total,
                &job_ref,
                &events,
            )
        })
        .await
        .map_err(|e| Error::Detail(format!("解压线程挂了：{e}")))??
    };
    Ok(placed)
}

#[cfg(test)]
mod tests {
    use super::{matches_platform, parse_name, ToolId};

    #[test]
    fn 四种离线包都认得出工具与版本() {
        assert_eq!(
            parse_name("node-v20.18.0-win-x64.zip"),
            Some((ToolId::Node, "20.18.0".into()))
        );
        assert_eq!(
            parse_name("cpython-3.12.16+20261026-x86_64-pc-windows-msvc-install_only.tar.gz"),
            Some((ToolId::Python, "3.12.16".into()))
        );
        assert_eq!(
            parse_name("go1.27.3.windows-amd64.zip"),
            Some((ToolId::Go, "1.27.3".into()))
        );
        // PyPy 的版本要写成"兼容的 CPython + 发布号"，跟在线清单一致，否则登记不上号。
        assert_eq!(
            parse_name("pypy3.11-v8.0.1-win64.zip"),
            Some((ToolId::PyPy, "3.11-v8.0.1".into()))
        );
        assert_eq!(parse_name("goland-2026.1.zip"), None);
    }

    #[test]
    fn 平台标记不对就不装() {
        assert!(matches_platform("go1.27.3.windows-amd64.zip", ToolId::Go));
        assert!(!matches_platform("go1.27.3.linux-amd64.tar.gz", ToolId::Go));
        assert!(matches_platform("pypy3.11-v8.0.1-win64.zip", ToolId::PyPy));
        assert!(!matches_platform("pypy3.11-v8.0.1-linux64.tar.gz", ToolId::PyPy));
    }
}
