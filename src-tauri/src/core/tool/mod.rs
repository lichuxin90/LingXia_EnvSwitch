use std::path::PathBuf;

use serde::Serialize;

use crate::core::error::{Error, Result};
use crate::core::model::{AvailableVersion, Companion, ToolId};

pub mod eol;
pub mod go;
pub mod node;
pub mod python;
pub mod pypy;

/// 一个下载源。`base` 用于拼官方相对路径，`wrap` 用于把完整地址套上中转前缀。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceInfo {
    pub id: String,
    pub label: String,
    /// "国内" / "海外" / "第三方"
    pub region: &'static str,
    pub base: String,
    /// 形如 `https://gh-proxy.com/{url}`，把官方完整地址填进 {url}。
    pub wrap: Option<String>,
    pub note: Option<String>,
    /// 版本清单地址，用于测速。
    pub probe_url: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ArchiveKind {
    Zip,
    TarGz,
    TarBz2,
}

/// 下载与校验所需的全部信息。
#[derive(Debug, Clone)]
pub struct Artifact {
    pub url: String,
    pub file_name: String,
    pub sha256: Option<String>,
    pub size_hint: Option<u64>,
    pub kind: ArchiveKind,
}

/// shim 转发时的目标：直接执行某个文件、用解释器跑某个脚本、或 `-m 模块`。
#[derive(Debug, Clone)]
pub enum ShimTarget {
    Bin(PathBuf),
    Script(PathBuf),
    Module(String),
}

#[derive(Debug, Clone)]
pub struct ShimSpec {
    pub name: String,
    pub target: ShimTarget,
}

/// 四个插件用枚举静态分发，避免 dyn + async 的额外开销和样板。
#[derive(Debug, Clone, Copy)]
pub enum Plugin {
    Node(node::NodePlugin),
    Python(python::PythonPlugin),
    Go(go::GoPlugin),
    PyPy(pypy::PyPyPlugin),
}

impl Plugin {
    pub fn all() -> Vec<Plugin> {
        vec![
            Plugin::Node(node::NodePlugin),
            Plugin::Python(python::PythonPlugin),
            Plugin::Go(go::GoPlugin),
            Plugin::PyPy(pypy::PyPyPlugin),
        ]
    }

    pub fn nav_all() -> Vec<Plugin> {
        ToolId::NAV.into_iter().filter_map(Self::for_tool_ok).collect()
    }

    pub fn for_tool(tool: ToolId) -> Result<Plugin> {
        Self::for_tool_ok(tool).ok_or_else(|| Error::NotFound(format!("还没有 {} 这个工具", tool.display())))
    }

    fn for_tool_ok(tool: ToolId) -> Option<Plugin> {
        Some(match tool {
            ToolId::Node => Plugin::Node(node::NodePlugin),
            ToolId::Python => Plugin::Python(python::PythonPlugin),
            ToolId::Go => Plugin::Go(go::GoPlugin),
            ToolId::PyPy => Plugin::PyPy(pypy::PyPyPlugin),
        })
    }

    pub fn id(&self) -> ToolId {
        match self {
            Plugin::Node(_) => ToolId::Node,
            Plugin::Python(_) => ToolId::Python,
            Plugin::Go(_) => ToolId::Go,
            Plugin::PyPy(_) => ToolId::PyPy,
        }
    }

    pub fn sources(&self) -> Vec<SourceInfo> {
        match self {
            Plugin::Node(p) => p.sources(),
            Plugin::Python(p) => p.sources(),
            Plugin::Go(p) => p.sources(),
            Plugin::PyPy(p) => p.sources(),
        }
    }

    pub fn source(&self, id: &str) -> Result<SourceInfo> {
        let all = self.sources();
        all
            .iter()
            .find(|s| s.id == id)
            .cloned()
            .ok_or_else(|| {
                Error::NotFound(format!(
                    "没有 id 为 {id} 的下载源，可选：{}",
                    all.iter().map(|s| s.id.as_str()).collect::<Vec<_>>().join("、")
                ))
            })
    }

    pub async fn list_versions(&self, client: &reqwest::Client, src: &SourceInfo) -> Result<Vec<AvailableVersion>> {
        match self {
            Plugin::Node(p) => p.list_versions(client, src).await,
            Plugin::Python(p) => p.list_versions(client, src).await,
            Plugin::Go(p) => p.list_versions(client, src).await,
            Plugin::PyPy(p) => p.list_versions(client, src).await,
        }
    }

    pub async fn artifact(
        &self,
        client: &reqwest::Client,
        src: &SourceInfo,
        v: &AvailableVersion,
    ) -> Result<Artifact> {
        match self {
            Plugin::Node(p) => p.artifact(client, src, v).await,
            Plugin::Python(p) => p.artifact(src, v),
            Plugin::Go(p) => p.artifact(src, v),
            Plugin::PyPy(p) => p.artifact(src, v),
        }
    }

    /// 解压后判断"这是一份有效载荷"的相对路径候选。
    pub fn payload_markers(&self) -> Vec<PathBuf> {
        match self {
            Plugin::Node(_) => node::payload_markers(),
            Plugin::Python(_) => python::payload_markers(),
            Plugin::Go(_) => go::payload_markers(),
            Plugin::PyPy(_) => pypy::payload_markers(),
        }
    }

    pub fn exec_rel(&self) -> PathBuf {
        match self {
            Plugin::Node(_) => node::exec_rel(),
            Plugin::Python(_) => python::exec_rel(),
            Plugin::Go(_) => go::exec_rel(),
            Plugin::PyPy(_) => pypy::exec_rel(),
        }
    }

    /// 需要为本版本生成的 shim 名称与转发方式。
    pub fn shims(&self) -> Vec<ShimSpec> {
        match self {
            Plugin::Node(_) => node::shim_specs(),
            Plugin::Python(_) => python::shim_specs(),
            Plugin::Go(_) => go::shim_specs(),
            Plugin::PyPy(_) => pypy::shim_specs(),
        }
    }

    /// 装完后附带的包管理器，右侧详情面板要展示。
    pub fn companions(&self, root: &std::path::Path) -> Vec<Companion> {
        match self {
            Plugin::Node(p) => p.companions(root),
            Plugin::Python(p) => p.companions(root),
            Plugin::Go(p) => p.companions(root),
            Plugin::PyPy(p) => p.companions(root),
        }
    }

    /// 该工具在 `.tool-versions` 里的键名。
    pub fn key(&self) -> &'static str {
        self.id().as_str()
    }

    /// 平台标签，界面在"关于"里展示，便于用户核对下错包的情况。
    pub fn platform_triple(&self) -> &'static str {
        if cfg!(target_os = "windows") {
            if cfg!(target_arch = "aarch64") {
                "windows-arm64"
            } else {
                "windows-x64"
            }
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
}

/// 把官方完整地址按源换算成实际下载地址。
pub fn apply_wrap(src: &SourceInfo, official_url: &str) -> String {
    match &src.wrap {
        Some(t) if t.contains("{url}") => t.replace("{url}", official_url),
        Some(t) => format!("{}{official_url}", t.trim_end_matches('/')),
        None => official_url.to_string(),
    }
}

/// 版本号归一：官方清单里 Node 带 v 前缀，界面和数据都去掉了。
pub fn strip_v(raw: &str) -> String {
    raw.trim().trim_start_matches('v').to_string()
}

/// 卡片主线号：Node 按大版本（20），Python/Go/PyPy 各按自己的规则。
/// 清单和已装记录都靠它对齐，改分组规则只改这里。
pub fn line_key(tool: ToolId, version: &str) -> String {
    match tool {
        ToolId::Node => version.trim().trim_start_matches('v').split('.').next().unwrap_or("").to_string(),
        ToolId::Python => {
            let parts: Vec<&str> = version
                .trim()
                .trim_start_matches('v')
                .split('.')
                .take(2)
                .collect();
            parts.join(".")
        }
        ToolId::Go => go::line_of(version.trim().trim_start_matches("go")),
        ToolId::PyPy => pypy::line_of(version.trim()),
    }
}

pub fn in_line(tool: ToolId, version: &str, line: &str) -> bool {
    line_key(tool, version) == line
}
