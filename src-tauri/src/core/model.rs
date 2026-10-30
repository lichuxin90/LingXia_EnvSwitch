use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// 已落地的运行时插件。PyPy 是 Python 的附加分支：数据上独立成一套版本，界面上不单开导航。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum ToolId {
    #[serde(rename = "node")]
    Node,
    #[serde(rename = "python")]
    Python,
    #[serde(rename = "go")]
    Go,
    #[serde(rename = "pypy")]
    PyPy,
}

impl ToolId {
    pub const ALL: [ToolId; 4] = [ToolId::Node, ToolId::Python, ToolId::Go, ToolId::PyPy];

    /// 侧边栏「运行时」列表里单列出来的工具；PyPy 归在 Python 底下。
    pub const NAV: [ToolId; 3] = [ToolId::Node, ToolId::Python, ToolId::Go];

    pub fn as_str(self) -> &'static str {
        match self {
            ToolId::Node => "node",
            ToolId::Python => "python",
            ToolId::Go => "go",
            ToolId::PyPy => "pypy",
        }
    }

    pub fn display(self) -> &'static str {
        match self {
            ToolId::Node => "Node.js",
            ToolId::Python => "Python",
            ToolId::Go => "Go",
            ToolId::PyPy => "PyPy",
        }
    }

    /// 归到哪个导航项下面：PyPy 挂在 Python，其他工具各归各。
    pub fn nav_parent(self) -> ToolId {
        match self {
            ToolId::PyPy => ToolId::Python,
            other => other,
        }
    }

    /// 同一个导航项下面可以切换的分支，顺序就是界面上的顺序。
    pub fn siblings(self) -> Vec<ToolId> {
        match self.nav_parent() {
            ToolId::Python => vec![ToolId::Python, ToolId::PyPy],
            other => vec![other],
        }
    }

    pub fn from_key(key: &str) -> Option<ToolId> {
        match key.trim().to_lowercase().as_str() {
            "node" | "nodejs" | "node.js" => Some(ToolId::Node),
            "python" | "python3" | "py" => Some(ToolId::Python),
            "go" | "golang" => Some(ToolId::Go),
            "pypy" | "pypy3" => Some(ToolId::PyPy),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum InstallSource {
    /// 官方源
    Official,
    /// 国内镜像
    Mirror,
    /// 用户自己拖进来的离线包
    Offline,
    /// 接管电脑上已有的安装，没有重新下载
    Takeover,
}

impl Default for InstallSource {
    fn default() -> Self {
        InstallSource::Official
    }
}

/// 官方发布列表里的一条；插件在列清单时就把当前机器该下的包算好了。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct AvailableVersion {
    pub tool: ToolId,
    pub version: String,
    /// 官方发布日期原文，界面不再二次解析。
    pub released: Option<String>,
    /// LTS 代号，如 "Ironman"；非 LTS 为 None。
    pub lts_codename: Option<String>,
    pub is_lts: bool,
    /// 该 LTS 是否仍在维护期（Node 官方字段）。
    pub lts_active: Option<bool>,
    /// 这条版本线是否已过停止更新日期；None 表示拿不到时间表。
    pub eol: Option<bool>,
    /// 停止更新的日期原文，界面写成「维护到 2026-04-30」。
    pub eol_date: Option<String>,
    /// 同一主线的最新小版本，用来折叠卡片。
    pub is_line_latest: bool,
    /// 同主线下还有多少个小版本可以选。
    pub line_count: usize,
    pub size_hint: Option<u64>,
    pub installed: bool,
    /// 当前平台对应的下载地址（已按选定的源换算过）。
    pub download_url: Option<String>,
    pub file_name: Option<String>,
    pub sha256: Option<String>,
    /// python-build-standalone 的 release tag，换源时用来重算地址。
    pub origin_tag: Option<String>,
    /// 卡片分组用的主线号：Node 按大版本（20），Python 按小版本（3.12）。
    pub line_key: String,
    /// 这条是否没有本平台包（比如只发了源码包）。
    pub unavailable_reason: Option<String>,
}

impl Default for AvailableVersion {
    fn default() -> Self {
        Self {
            tool: ToolId::Node,
            version: String::new(),
            released: None,
            lts_codename: None,
            is_lts: false,
            lts_active: None,
            eol: None,
            eol_date: None,
            is_line_latest: false,
            line_count: 1,
            size_hint: None,
            installed: false,
            download_url: None,
            file_name: None,
            sha256: None,
            origin_tag: None,
            line_key: String::new(),
            unavailable_reason: None,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstalledVersion {
    pub tool: ToolId,
    pub version: String,
    /// installs/<tool>/<version> 根目录。
    pub dir: PathBuf,
    /// 真正被执行的可解释器绝对路径。
    pub exec_path: PathBuf,
    pub size_bytes: u64,
    pub installed_at: String,
    pub source: InstallSource,
    pub sha256: Option<String>,
    /// 离线包/接管场景记下原始位置，便于回滚。
    pub origin: Option<PathBuf>,
    /// 附带的包管理器版本，如 npm 10.8.2 / pip 24.2。
    pub companions: Vec<Companion>,
    /// 当前被 shim 指向（全局默认或唯一安装）。
    pub is_active: bool,
    pub is_global_default: bool,
    /// 被哪些项目目录通过 .tool-versions 绑定。
    pub bound_projects: Vec<PathBuf>,
    /// 接管来的版本可能缺文件，界面标出来。
    pub usable: bool,
    pub unusable_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Companion {
    pub name: String,
    pub version: Option<String>,
    pub note: Option<String>,
}

/// 一个主线的卡片。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VersionLine {
    pub tool: ToolId,
    pub major: String,
    pub newest: String,
    pub is_lts: bool,
    pub lts_active: Option<bool>,
    pub lts_codename: Option<String>,
    /// 这条版本线已过停止更新日期；None 表示拿不到时间表。
    pub eol: Option<bool>,
    pub eol_date: Option<String>,
    pub released: Option<String>,
    pub installed: bool,
    pub is_global_default: bool,
    pub size_bytes: u64,
    pub member_count: usize,
    pub members: Vec<AvailableVersion>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ResolutionSource {
    /// 项目里的 .tool-versions
    ProjectFile,
    /// exe 同级或上级目录的 .envswitch.toml? 目前只有前两种
    GlobalDefault,
    /// 只装了唯一一个版本，直接用它
    OnlyInstalled,
    /// 没解析出来，走系统里的原版
    None,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Resolution {
    pub tool: ToolId,
    pub version: Option<String>,
    pub source: ResolutionSource,
    /// 命中的 .tool-versions 所在目录，界面据此显示"当前目录将使用 X"。
    pub found_in: Option<PathBuf>,
    pub exec_path: Option<PathBuf>,
    /// shim 会拒绝接管的情况（例如锁定了全局）。
    pub note: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectLock {
    pub dir: PathBuf,
    pub file: PathBuf,
    pub entries: BTreeMap<String, String>,
    /// 文件里非本工具的其他行，改写时要保留。
    pub extra_lines: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JobEvent {
    pub job_id: String,
    pub tool: Option<ToolId>,
    pub version: Option<String>,
    pub phase: JobPhase,
    pub received: u64,
    pub total: Option<u64>,
    pub bytes_per_sec: u64,
    pub eta_secs: Option<u64>,
    pub percent: Option<f64>,
    pub message: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum JobPhase {
    Queued,
    Planning,
    Downloading,
    Verifying,
    Extracting,
    Linking,
    Done,
    Failed,
    Cancelled,
}

impl JobPhase {
    pub fn label(self) -> &'static str {
        match self {
            JobPhase::Queued => "排队中",
            JobPhase::Planning => "准备中",
            JobPhase::Downloading => "下载中",
            JobPhase::Verifying => "校验中",
            JobPhase::Extracting => "解压中",
            JobPhase::Linking => "登记中",
            JobPhase::Done => "完成",
            JobPhase::Failed => "失败",
            JobPhase::Cancelled => "已取消",
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JobHandle {
    pub job_id: String,
    pub tool: ToolId,
    pub version: String,
    pub phase: JobPhase,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PathScope {
    /// 只影响当前登录用户，不需要管理员。
    User,
    /// 写系统环境变量，所有账号生效，需要管理员。
    System,
}

impl PathScope {
    pub fn label(self) -> &'static str {
        match self {
            PathScope::User => "仅当前用户",
            PathScope::System => "整台电脑",
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PathStatus {
    pub scope: PathScope,
    pub shim_dir: PathBuf,
    /// shim 目录已在 PATH 里。
    pub installed: bool,
    /// shim 目录排在最前面。
    pub is_first: bool,
    pub position: Option<usize>,
    pub entries: Vec<String>,
    pub backup_count: usize,
    pub latest_backup: Option<String>,
    pub elevated: bool,
    pub note: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PathBackup {
    pub id: String,
    pub created_at: String,
    pub scope: PathScope,
    pub kind: BackupKind,
    pub entry_count: usize,
    pub preview: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum BackupKind {
    Path,
    Config,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ForeignRuntime {
    pub tool: Option<ToolId>,
    pub name: String,
    pub version: Option<String>,
    pub exe: PathBuf,
    pub root: PathBuf,
    /// nvm / pyenv / scoop / 系统自带 之类来源说明。
    pub manager: Option<String>,
    /// 能不能直接纳入管理（免下载）。
    pub takeable: bool,
    pub why_not: Option<String>,
    /// 纳入后预计占用的空间（接管为引用还是复制）。
    pub mode: TakeoverMode,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TakeoverMode {
    /// 版本目录原样引用，不动用户文件
    Reference,
    /// 复制到 envswitch-data/installs 下，之后原目录可删
    Copy,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Conflict {
    /// nvm / pyenv / volta / fnm / scoop / asdf / mise / conda
    pub name: String,
    pub kind: ConflictKind,
    pub path: Option<PathBuf>,
    pub detail: String,
    pub suggestion: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ConflictKind {
    /// 别的版本管理器也在往 PATH 里塞 shim
    Manager,
    /// 系统里已经有一个直接用 PATH 的 node/python
    Runtime,
    /// 我们自己的 PATH 记录被外部改过
    Stale,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanReport {
    pub conflicts: Vec<Conflict>,
    pub foreign: Vec<ForeignRuntime>,
    pub ours_shim_first: bool,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MirrorProbe {
    pub id: String,
    pub label: String,
    pub url: String,
    pub region: &'static str,
    pub ok: bool,
    pub latency_ms: Option<u64>,
    pub error: Option<String>,
    pub selected: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResetOutcome {
    pub shims_removed: usize,
    pub path_restored: Option<String>,
    pub path_scope: PathScope,
    pub installs_kept: usize,
    pub installs_removed: usize,
    pub hooks_removed: Vec<String>,
    pub failed: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TransferSummary {
    pub file: PathBuf,
    pub tools: BTreeMap<String, String>,
    pub project_count: usize,
    pub installed_count: usize,
}
