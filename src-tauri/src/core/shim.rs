use std::path::{Path, PathBuf};

use serde::Serialize;

use super::error::{Error, Result};
use super::model::ToolId;
use super::paths;
use super::records::Registry;
use super::tool::{Plugin, ShimTarget};
use super::util;

pub const SHIM_BIN_NAME: &str = if cfg!(windows) {
    "envswitch-shim.exe"
} else {
    "envswitch-shim"
};

/// 生成 shim 用的模板二进制位置。
/// 它是控制端子系统的独立小程序，不能拿主程序顶替：Windows 上 GUI 程序没有控制台，
/// 用它转发 `node -e "console.log(1)"` 会把输出丢掉。
///
/// 安装包里它的名字带 target triple（tauri 的 externalBin 命名规则），位置也可能不在主程序旁边：
/// macOS 进 `.app/Contents/Resources`，Linux 的 deb 与 AppImage 进 `lib/envswitch`。
/// 所以按几个候选位置找，找到即用，别指望只有一种布局。
pub fn shim_bin() -> Result<PathBuf> {
    if let Ok(raw) = std::env::var(concat!("ENV", "SWITCH", "_SHIM_BIN")) {
        let p = PathBuf::from(raw.trim());
        if p.is_file() {
            return Ok(p);
        }
    }
    let exe = std::env::current_exe().map_err(|e| Error::Io(format!("认不出自己在哪：{e}")))?;
    let dir = exe.parent().ok_or_else(|| Error::Detail("程序路径不完整".to_string()))?;

    let mut names = vec![SHIM_BIN_NAME.to_string()];
    if let Some(target) = option_env!("ENVSWITCH_TARGET") {
        names.push(format!(
            "envswitch-shim-{target}{}",
            if cfg!(windows) { ".exe" } else { "" }
        ));
    }

    let mut dirs = vec![dir.to_path_buf()];
    if let Some(parent) = dir.parent() {
        dirs.push(parent.join("Resources"));
        dirs.push(parent.join("lib").join("envswitch"));
        dirs.push(parent.to_path_buf());
    }

    for d in &dirs {
        for name in &names {
            let p = d.join(name);
            // 打包前 build.rs 会在 src-tauri/binaries 里补一个空占位文件（见那里的注释），
            // 所以带 triple 的那个候选必须真有点体积才算数，别把占位当成模板拿去生成 shim。
            if p.is_file() && p.metadata().map(|m| m.len() > 1024).unwrap_or(false) {
                return Ok(p);
            }
        }
    }

    // 用主程序自己当模板不行（它没有控制台），但拿它的位置去找同级产物是可靠的。
    Err(Error::Detail(format!(
        "找不到转发用的 {SHIM_BIN_NAME}；它必须和主程序放在同一个文件夹里，重新装一次或重新构建一次就好"
    )))
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ShimFile {
    pub name: String,
    pub path: PathBuf,
    pub tool: Option<ToolId>,
    pub exists: bool,
}

pub fn shim_path(dir: &Path, name: &str) -> PathBuf {
    if cfg!(windows) {
        dir.join(format!("{name}.exe"))
    } else {
        dir.join(name)
    }
}

/// 为某个工具补齐 shim，返回新建的数量。已经存在的会换新版模板但不动配置。
pub fn ensure_tool_shims(tool: ToolId) -> Result<usize> {
    let layout = paths::layout()?;
    let plugin = Plugin::for_tool(tool)?;
    let bin = shim_bin()?;
    let mut created = 0usize;
    for spec in plugin.shims() {
        let dst = shim_path(&layout.shims, &spec.name);
        place(&bin, &dst)?;
        created += 1;
    }
    Ok(created)
}

pub fn remove_tool_shims(tool: ToolId) -> Result<usize> {
    let layout = paths::layout()?;
    let plugin = Plugin::for_tool(tool)?;
    let mut removed = 0usize;
    for spec in plugin.shims() {
        let dst = shim_path(&layout.shims, &spec.name);
        if remove_place(&dst).is_ok() {
            removed += 1;
        }
    }
    Ok(removed)
}

/// 全量补齐（含所有已装工具），并清掉不再需要的 shim。
pub fn sync_all() -> Result<Vec<ShimFile>> {
    let layout = paths::layout()?;
    let registry = Registry::load()?;
    let mut wanted: Vec<(String, ToolId)> = Vec::new();
    for plugin in Plugin::all() {
        if registry.list(plugin.id()).is_empty() {
            continue;
        }
        for spec in plugin.shims() {
            wanted.push((spec.name, plugin.id()));
        }
    }
    let mut out = Vec::new();
    for (name, tool) in &wanted {
        ensure_tool_shims(*tool)?;
        let path = shim_path(&layout.shims, name);
        out.push(ShimFile {
            name: name.clone(),
            path,
            tool: Some(*tool),
            exists: true,
        });
    }
    // 工具全部卸载后留下的空壳要收掉，改名让位留下的旧模板也一样。
    for entry in std::fs::read_dir(&layout.shims).into_iter().flatten().flatten() {
        let path = entry.path();
        let full = path
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default();
        let stem = path
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default();
        if stem == "envswitch-shim" || full.starts_with('.') {
            continue;
        }
        // `node.old-时间戳` 整体当成文件名看：只按 file_stem 判会把它读成 "node"，
        // 于是误认为还是当前 shim，永远清不掉。
        if full.contains(".old-") || !wanted.iter().any(|(n, _)| n == &stem) {
            let _ = remove_place(&path);
        }
    }
    Ok(out)
}

pub fn list() -> Result<Vec<ShimFile>> {
    let layout = paths::layout()?;
    let mut out = Vec::new();
    for plugin in Plugin::all() {
        for spec in plugin.shims() {
            let path = shim_path(&layout.shims, &spec.name);
            out.push(ShimFile {
                name: spec.name,
                path,
                tool: Some(plugin.id()),
                exists: false,
            });
        }
    }
    for item in &mut out {
        item.exists = item.path.is_file();
    }
    Ok(out)
}

/// 把 shim 目录整个清空，一键重置用。
pub fn wipe() -> Result<usize> {
    let layout = paths::layout()?;
    let mut removed = 0usize;
    if !layout.shims.is_dir() {
        return Ok(0);
    }
    for entry in std::fs::read_dir(&layout.shims).into_iter().flatten().flatten() {
        if remove_place(&entry.path()).is_ok() {
            removed += 1;
        }
    }
    Ok(removed)
}

/// 在 shim 目录里留一份自名字的可执行文件，供终端脚本和命令行直接调用：
/// `envswitch-shim explain`、提权时也是它被拿去写系统 PATH。
pub fn ensure_cli_copy() -> Result<PathBuf> {
    let layout = paths::layout()?;
    let dst = layout.shims.join(SHIM_BIN_NAME);
    place(&shim_bin()?, &dst)?;
    Ok(dst)
}

/// 放置模板：优先硬链接（不额外占空间），跨盘或不支持时退回复制。
fn place(src: &Path, dst: &Path) -> Result<()> {
    if let Some(parent) = dst.parent() {
        std::fs::create_dir_all(parent)?;
    }
    if dst.exists() {
        // Windows 上正在运行的 shim 删不掉，先改名让位，新文件占住原名。
        let tail = dst
            .extension()
            .map(|e| format!(".{}", e.to_string_lossy()))
            .unwrap_or_default();
        let stale = dst.with_file_name(format!(
            "{}.old-{}{}",
            dst.file_stem().and_then(|s| s.to_str()).unwrap_or("shim"),
            util::timestamp(),
            tail
        ));
        if std::fs::rename(dst, &stale).is_err() {
            // 改名也失败说明它正被占用。旧模板功能一致，留着不用它，不算失败。
            return Ok(());
        }
    }
    std::fs::hard_link(src, dst).or_else(|_| {
        std::fs::copy(src, dst).map(|_| {
            let _ = make_exec(dst);
        })
    })
        .map_err(|e| Error::Io(format!("生成 {} 失败：{e}", dst.display())))
}

fn remove_place(path: &Path) -> Result<()> {
    if !path.exists() {
        return Ok(());
    }
    std::fs::remove_file(path).map_err(|e| {
        Error::Io(format!("删不掉 {}（可能正在用）：{e}", path.display()))
    })?;
    Ok(())
}

#[cfg(unix)]
fn make_exec(path: &Path) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let mode = std::fs::metadata(path).map(|m| m.permissions().mode() | 0o111).unwrap_or(0o755);
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode))
}

#[cfg(windows)]
fn make_exec(_path: &Path) -> std::io::Result<()> {
    Ok(())
}

/// shim 进程启动后用得到的转发行事：程序路径 + 需要插在前面的参数。
#[derive(Debug, Clone)]
pub struct Forward {
    pub program: PathBuf,
    pub prefix_args: Vec<String>,
}

pub fn forward_for(
    registry: &Registry,
    tool: ToolId,
    version: &str,
    shim_name: &str,
) -> Result<Forward> {
    let record = registry
        .find(tool, version)
        .ok_or_else(|| Error::NotInstalled(tool.display().to_string(), version.to_string()))?;
    let plugin = Plugin::for_tool(tool)?;
    let spec = plugin
        .shims()
        .into_iter()
        .find(|s| s.name == shim_name)
        .or_else(|| plugin.shims().first().cloned())
        .ok_or_else(|| Error::Detail(format!("{} 没定义任何转发规则", tool.display())))?;
    let root = &record.dir;
    match spec.target {
        ShimTarget::Bin(rel) => {
            let program = root.join(rel);
            if !program.is_file() {
                return Err(Error::NotFound(format!("{} 不见了", program.display())));
            }
            Ok(Forward { program, prefix_args: Vec::new() })
        }
        ShimTarget::Script(rel) => {
            let script = root.join(&rel);
            if !script.is_file() {
                return Err(Error::NotFound(format!(
                    "找不到 {}，这个版本的 npm 目录结构不一样",
                    script.display()
                )));
            }
            Ok(Forward {
                program: record.exec_path.clone(),
                prefix_args: vec![script.to_string_lossy().to_string()],
            })
        }
        ShimTarget::Module(module) => Ok(Forward {
            program: record.exec_path.clone(),
            prefix_args: vec!["-m".to_string(), module],
        }),
    }
}
