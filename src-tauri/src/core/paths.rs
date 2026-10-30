use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde::Serialize;

use super::error::{Error, Result};

/// 数据根目录默认紧贴 exe，绝不主动写系统盘。
pub const DATA_DIR_NAME: &str = "envswitch-data";
/// shim 副本所在的子目录名；程序本身也能靠它反推数据根目录在哪。
pub const SHIMS_DIR_NAME: &str = "shims";
/// 用户把数据目录挪到别处时，只在 exe 旁边留这一个几十字节的指路文件。
pub const LOCATION_FILE_NAME: &str = "envswitch-data.location";
pub const HOME_ENV: &str = "ENVSWITCH_HOME";

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Layout {
    pub root: PathBuf,
    pub installs: PathBuf,
    pub shims: PathBuf,
    pub cache: PathBuf,
    pub partial: PathBuf,
    pub backups: PathBuf,
    pub config: PathBuf,
    pub logs: PathBuf,
    /// 解压中间产物，装完即清。
    pub tmp: PathBuf,
    /// 数据目录是否就在 exe 旁边（未启用指路文件）。
    pub beside_exe: bool,
}

impl Layout {
    fn new(root: PathBuf, beside_exe: bool) -> Self {
        let root = super::util::strip_verbatim(&root);
        Self {
            installs: root.join("installs"),
            shims: root.join(SHIMS_DIR_NAME),
            cache: root.join("cache"),
            partial: root.join("cache").join("partial"),
            backups: root.join("backups"),
            config: root.join("config"),
            logs: root.join("logs"),
            tmp: root.join("tmp"),
            root,
            beside_exe,
        }
    }

    pub fn create_all(&self) -> Result<()> {
        for dir in [
            &self.root,
            &self.installs,
            &self.shims,
            &self.cache,
            &self.partial,
            &self.backups,
            &self.config,
            &self.logs,
            &self.tmp,
        ] {
            std::fs::create_dir_all(dir)
                .map_err(|e| Error::Io(format!("建目录 {} 失败：{e}", dir.display())))?;
        }
        Ok(())
    }

    pub fn version_dir(&self, tool: &str, version: &str) -> PathBuf {
        self.installs.join(tool).join(version)
    }

    /// 压缩包落盘位置：同版本重复下载直接复用，装完自动清掉。
    pub fn archive_path(&self, file_name: &str) -> PathBuf {
        self.cache.join(file_name)
    }

    pub fn partial_path(&self, file_name: &str) -> PathBuf {
        self.partial.join(file_name)
    }

    pub fn config_path(&self, name: &str) -> PathBuf {
        self.config.join(name)
    }
}

/// 首次设置里列出来的备选存放位置。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Candidate {
    /// 稳定标识：界面按它查多语言词条（exe / drive / programData / home），不认中文名字。
    pub id: &'static str,
    pub dir: String,
    pub label: String,
    /// 推荐顺序：0 最推荐（程序同级），数字越大越不推荐
    pub rank: u8,
    pub note: Option<String>,
}

/// 列出能当数据目录的候选位置：程序同级最优先，其次各个非系统盘，最后才是 C 盘公共目录。
pub fn root_candidates() -> Vec<Candidate> {
    let mut out = Vec::new();
    let exe = exe_dir();
    push_candidate(&mut out, "exe", exe.join(DATA_DIR_NAME), "程序同级".to_string(), 0, None);
    if cfg!(windows) {
        for letter in b'D'..=b'Z' {
            let root = PathBuf::from(format!("{}:\\", letter as char));
            if root.is_dir() {
                push_candidate(
                    &mut out,
                    "drive",
                    root.join(DATA_DIR_NAME),
                    format!("{} 盘", letter as char),
                    1,
                    None,
                );
            }
        }
        push_candidate(
            &mut out,
            "programData",
            PathBuf::from("C:\\ProgramData").join("EnvSwitch"),
            "C 盘公共目录".to_string(),
            3,
            Some("会占系统盘空间，除非只有 C 盘可用，否则不建议".to_string()),
        );
    } else if let Some(home) = dirs::home_dir() {
        push_candidate(
            &mut out,
            "home",
            home.join(".envswitch"),
            "用户主目录".to_string(),
            1,
            None,
        );
    }
    out.sort_by_key(|c| c.rank);
    out
}

fn push_candidate(
    out: &mut Vec<Candidate>,
    id: &'static str,
    dir: PathBuf,
    label: String,
    rank: u8,
    note: Option<String>,
) {
    if dir.parent().is_none() {
        return;
    }
    out.push(Candidate {
        id,
        dir: dir.display().to_string(),
        label,
        rank,
        note,
    });
}

pub fn exe_dir() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.to_path_buf()))
        .unwrap_or_else(|| PathBuf::from("."))
}

fn location_file() -> PathBuf {
    exe_dir().join(LOCATION_FILE_NAME)
}

/// 开发态：exe 躺在 src-tauri/target/debug 里，按 exe 同级建目录会把安装物丢进 target，
/// 一次 `cargo clean` 全没。调试构建向上找工程根，把数据目录放在项目根。
fn dev_project_root() -> Option<PathBuf> {
    let mut dir = exe_dir().canonicalize().ok()?;
    for _ in 0..6 {
        if dir.join("src-tauri").is_dir() && dir.join("package.json").is_file() {
            return Some(dir);
        }
        if !dir.pop() {
            return None;
        }
    }
    None
}

/// 转发程序自己就在数据目录的 shims 下，靠这个位置就能找回根目录，
/// 不需要指路文件，也不依赖主程序在哪儿。
fn root_from_shim_dir() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let dir = exe.parent()?;
    if dir.file_name()?.to_str()? != SHIMS_DIR_NAME {
        return None;
    }
    let root = dir.parent()?;
    if root.join("installs").is_dir() || root.join("config").is_dir() {
        Some(root.to_path_buf())
    } else {
        None
    }
}

fn read_location() -> Option<PathBuf> {
    let text = std::fs::read_to_string(location_file()).ok()?;
    let line = text
        .lines()
        .map(str::trim)
        .find(|l| !l.is_empty() && !l.starts_with('#'))?;
    let p = PathBuf::from(line);
    Some(if p.is_absolute() {
        p
    } else {
        exe_dir().join(p)
    })
}

fn can_write(dir: &Path) -> bool {
    if !dir.exists() {
        let _ = std::fs::create_dir_all(dir);
    }
    let probe = dir.join(format!(".envswitch-write-probe-{}", std::process::id()));
    match std::fs::File::create(&probe) {
        Ok(_) => {
            let _ = std::fs::remove_file(&probe);
            true
        }
        Err(_) => false,
    }
}

/// 浏览器内核的缓存位置。Tauri 默认把它放去 `%LOCALAPPDATA%\<标识>`，一次会话就能攒上百 MB，
/// 那是往系统盘写缓存，跟"东西都放在运行目录的独立文件夹里"直接冲突，所以自己定一个。
/// 数据目录已经选好就跟着数据目录走；还没选（首次设置那一下）先放程序同级。
/// 两处都写不进去时返回 None，让 Tauri 用它自己的默认值，别为了省 C 盘把程序卡死。
pub fn webview_data_dir() -> Option<PathBuf> {
    // 只判断能不能写，不替它把目录建出来：首次设置前就建出 envswitch-data，
    // find_root 会以为数据目录早就就位，向导再也不会出现。目录由 Tauri 自己建。
    let (root, probe) = match layout() {
        Ok(l) => (l.root.clone(), l.root),
        Err(_) => {
            let beside = exe_dir().join(DATA_DIR_NAME);
            (beside, exe_dir())
        }
    };
    if !probe.is_dir() || !can_write(&probe) {
        return None;
    }
    Some(root.join("webview"))
}

/// 只做定位，不创建任何东西；None 表示还没完成首次设置。
fn find_root() -> Option<(PathBuf, bool)> {
    if let Ok(raw) = std::env::var(HOME_ENV) {
        if !raw.trim().is_empty() {
            let p = PathBuf::from(raw.trim());
            let abs = if p.is_absolute() { p } else { exe_dir().join(p) };
            return Some((abs, false));
        }
    }
    // shims/ 里的副本（终端转发用的那个小程序）靠自身位置定位数据目录。
    if let Some(root) = root_from_shim_dir() {
        return Some((root, false));
    }
    if let Some(p) = read_location() {
        return Some((p, false));
    }
    let beside = exe_dir().join(DATA_DIR_NAME);
    if beside.is_dir() {
        return Some((beside, true));
    }
    if cfg!(debug_assertions) && let Some(project) = dev_project_root() {
        return Some((project.join(DATA_DIR_NAME), false));
    }
    // exe 同级能写就用同级，这是"装完即用、零配置"的默认路径。
    if beside.parent().map(can_write).unwrap_or(false) {
        return Some((beside, true));
    }
    None
}

static CACHE: Mutex<Option<Layout>> = Mutex::new(None);

pub fn layout() -> Result<Layout> {
    if let Ok(guard) = CACHE.lock()
        && let Some(l) = guard.as_ref()
    {
        return Ok(l.clone());
    }
    let (root, beside_exe) = find_root().ok_or(Error::DataRootMissing)?;
    let l = Layout::new(root, beside_exe);
    if let Ok(mut guard) = CACHE.lock() {
        *guard = Some(l.clone());
    }
    Ok(l)
}

/// 用户挪动过数据目录后刷新缓存。
pub fn forget_layout_cache() {
    if let Ok(mut guard) = CACHE.lock() {
        *guard = None;
    }
}

/// 用户在首次向导里选好目录后调用：可写就落在该处，指路文件只在必要时写。
pub fn adopt_root(chosen: &Path) -> Result<Layout> {
    let chosen = chosen.to_path_buf();
    if !can_write(&chosen) {
        return Err(Error::Permission(format!(
            "{} 这个位置写不进去，换一个盘，或者先把文件夹建好",
            chosen.display()
        )));
    }
    let beside = exe_dir().join(DATA_DIR_NAME);
    let beside_exe = norm_eq(&chosen, &beside);
    let layout = Layout::new(chosen, beside_exe);
    layout.create_all()?;
    if beside_exe {
        let _ = std::fs::remove_file(location_file());
    } else {
        std::fs::write(location_file(), format!("{}\n", layout.root.display())).map_err(|e| {
            Error::Io(format!(
                "在 exe 旁边留指路文件失败：{e}；请把程序整体放到可写的目录里再用"
            ))
        })?;
    }
    forget_layout_cache();
    let _ = crate::core::paths::layout();
    Ok(layout)
}

/// PATH 里的同一路径常带尾斜杠/大小写差异，比较前先归一。
pub fn norm_eq(a: &Path, b: &Path) -> bool {
    fn clean(p: &Path) -> String {
        super::util::strip_verbatim(p)
            .to_string_lossy()
            .replace('\\', "/")
            .trim_end_matches('/')
            .to_lowercase()
    }
    if clean(a) == clean(b) {
        return true;
    }
    // 只在两边都真实存在时才做二次比对。都不存在时两次 canonicalize 都失败，
    // 若把"取不到"也当成相等，PATH 里任何一条已经不存在的旧目录都会被误认为
    // 是转发目录并从原值里删掉——那是真的会把环境搞坏。
    match (a.canonicalize().ok(), b.canonicalize().ok()) {
        (Some(x), Some(y)) => clean(&x) == clean(&y),
        _ => false,
    }
}

