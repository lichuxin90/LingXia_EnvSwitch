use std::path::{Path, PathBuf};

use serde::Serialize;

use super::error::{Error, Result};
use super::model::ToolId;
use super::paths;
use super::records::Registry;
use super::tool::{Plugin, ShimTarget};
use super::util;

/// 终端命令行入口的名字，只出现在数据目录的 shims/ 里（文档与终端脚本都认它）。
pub const SHIM_BIN_NAME: &str = if cfg!(windows) {
    "envswitch-shim.exe"
} else {
    "envswitch-shim"
};

/// 内嵌的转发模板：build.rs 把 stage-shim 编好的二进制路径递进来，编译期整个读进主程序。
/// 发布包因此只需要一个 exe；这份字节在首次用到时释放到数据目录的 bin/ 下。
/// 转发程序自己（shim feature）不内嵌：它不需要再带一份自己，否则每编一次体积就涨一截。
#[cfg(not(feature = "shim"))]
const SHIM_BLOB: &[u8] = include_bytes!(env!("ENVSWITCH_SHIM_BLOB"));
#[cfg(feature = "shim")]
const SHIM_BLOB: &[u8] = b"";

/// 释放出来的模板叫这个：不跟主程序挤在同一个文件夹，也不叫 envswitch-shim，
/// 免得两个长得很像的 exe 摆在一起让人分不清该点哪个。
pub const FORWARD_STEM: &str = "envswitch-forward";

fn forward_file_name() -> String {
    format!(
        "{FORWARD_STEM}{}",
        if cfg!(windows) { ".exe" } else { "" }
    )
}

/// 把内嵌的转发模板释放到数据目录的 bin/ 下，返回它的位置。
/// 内容一致就不动文件；要换新版时先写临时文件再改名顶替——
/// Windows 上正在运行的旧模板删不掉，改名让位后新文件才能占住原名。
fn materialize() -> Result<PathBuf> {
    let layout = paths::layout()?;
    let dst = layout.bin.join(forward_file_name());
    if std::fs::read(&dst).ok().as_deref() == Some(SHIM_BLOB) {
        return Ok(dst);
    }
    std::fs::create_dir_all(&layout.bin)?;
    let tmp = layout
        .bin
        .join(format!(".{FORWARD_STEM}-new-{}", util::timestamp()));
    std::fs::write(&tmp, SHIM_BLOB)
        .map_err(|e| Error::Io(format!("写 {} 失败：{e}", tmp.display())))?;
    let _ = make_exec(&tmp);
    if std::fs::rename(&tmp, &dst).is_err() {
        // 顶替失败多半是旧模板正在运行：把它改名让位，再试一次。
        let stale = dst.with_file_name(format!(
            "{FORWARD_STEM}.old-{}{}",
            util::timestamp(),
            if cfg!(windows) { ".exe" } else { "" }
        ));
        if std::fs::rename(&dst, &stale).is_ok() {
            let _ = std::fs::rename(&tmp, &dst);
        }
    }
    if dst.is_file() {
        let _ = std::fs::remove_file(&tmp);
        return Ok(dst);
    }
    let _ = std::fs::remove_file(&tmp);
    Err(Error::Io(format!(
        "释放转发程序到 {} 失败",
        dst.display()
    )))
}

/// 转发用的模板二进制在哪。
/// 它是控制端子系统的独立小程序，不能拿主程序顶替：Windows 上 GUI 程序没有控制台，
/// 用它转发 `node -e "console.log(1)"` 会把输出丢掉。
///
/// 发布版把它内嵌在主程序里，这里释放到数据目录的 bin/ 下；
/// 开发构建内嵌的可能是空占位，退回按老位置找 stage-shim 的产物。
pub fn shim_bin() -> Result<PathBuf> {
    if let Ok(raw) = std::env::var(concat!("ENV", "SWITCH", "_SHIM_BIN")) {
        let p = PathBuf::from(raw.trim());
        if p.is_file() {
            return Ok(p);
        }
    }
    if SHIM_BLOB.len() > 1024 {
        return materialize();
    }
    // 开发态兜底：主程序在 src-tauri/target/debug，stage-shim 的产物在 src-tauri/binaries。
    let exe = std::env::current_exe().map_err(|e| Error::Io(format!("认不出自己在哪：{e}")))?;
    // 命令行副本自己就是可用的模板：shims/envswitch-shim.exe、bin/envswitch-forward.exe
    // 被直接调去 shims sync 时，拿自己生成其余 shim 就行。
    let stem = exe
        .file_stem()
        .map(|s| s.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    if (stem == "envswitch-shim" || stem == FORWARD_STEM)
        && exe.metadata().map(|m| m.len() > 1024).unwrap_or(false)
    {
        return Ok(exe);
    }
    let dir = exe.parent().ok_or_else(|| Error::Detail("程序路径不完整".to_string()))?;

    let mut names = vec![SHIM_BIN_NAME.to_string()];
    if let Some(target) = option_env!("ENVSWITCH_TARGET") {
        names.push(format!(
            "envswitch-shim-{target}{}",
            if cfg!(windows) { ".exe" } else { "" }
        ));
    }

    let mut dirs = vec![dir.to_path_buf()];
    if cfg!(debug_assertions) {
        dirs.push(Path::new(env!("CARGO_MANIFEST_DIR")).join("binaries"));
    }

    for d in &dirs {
        for name in &names {
            let p = d.join(name);
            // 带 triple 的候选可能是 build.rs 补的空占位，必须真有点体积才算数。
            if p.is_file() && p.metadata().map(|m| m.len() > 1024).unwrap_or(false) {
                return Ok(p);
            }
        }
    }

    Err(Error::Detail(format!(
        "找不到转发用的 {SHIM_BIN_NAME}；先跑 node scripts/stage-shim.mjs 再重新构建一次就好"
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

/// 只读检查，不因打开程序而生成转发文件。只检查已安装工具。
/// 内置模板有更新时也需要补齐，避免旧命令行入口不认识新工具。
pub fn missing() -> Result<Vec<String>> {
    let layout = paths::layout()?;
    let registry = Registry::load()?;
    Ok(missing_in(&layout.bin, &layout.shims, &registry))
}

fn missing_in(bin: &Path, shims: &Path, registry: &Registry) -> Vec<String> {
    let mut required = vec![shims.join(SHIM_BIN_NAME)];
    if SHIM_BLOB.len() > 1024 { required.push(bin.join(forward_file_name())); }
    for plugin in Plugin::all() {
        if !registry.list(plugin.id()).is_empty() {
            required.extend(plugin.shims().iter().map(|s| shim_path(shims, &s.name)));
        }
    }
    required.into_iter().filter(|p| {
        if SHIM_BLOB.len() > 1024 {
            std::fs::read(p).ok().as_deref() != Some(SHIM_BLOB)
        } else {
            !p.metadata().map(|m| m.is_file() && m.len() > 1024).unwrap_or(false)
        }
    })
        .map(|p| p.display().to_string()).collect()
}

#[cfg(test)]
mod repair_tests {
    use super::*;

    #[test]
    fn missing_check_is_read_only_and_ignores_uninstalled_tools() {
        let root = std::env::temp_dir().join(format!("envswitch-shim-test-{}-{}", std::process::id(), util::timestamp()));
        let bin = root.join("bin");
        let shims = root.join("shims");
        let registry = Registry::default();
        assert!(!missing_in(&bin, &shims, &registry).is_empty());
        assert!(!root.exists(), "只读检测不能生成目录或文件");
        std::fs::create_dir_all(&bin).unwrap();
        std::fs::create_dir_all(&shims).unwrap();
        let fallback = vec![1; 2048];
        let bytes = if SHIM_BLOB.len() > 1024 { SHIM_BLOB } else { &fallback };
        std::fs::write(shims.join(SHIM_BIN_NAME), bytes).unwrap();
        if SHIM_BLOB.len() > 1024 { std::fs::write(bin.join(forward_file_name()), bytes).unwrap(); }
        assert!(missing_in(&bin, &shims, &registry).is_empty());
        let mut installed = registry;
        installed.put(super::super::records::Record { tool: ToolId::Deno, version: "2.0.0".into(), ..Default::default() });
        let deno = shim_path(&shims, "deno");
        assert_eq!(missing_in(&bin, &shims, &installed), vec![deno.display().to_string()]);
        std::fs::write(&deno, bytes).unwrap();
        assert!(missing_in(&bin, &shims, &installed).is_empty());
        std::fs::write(&deno, b"broken").unwrap();
        assert_eq!(missing_in(&bin, &shims, &installed), vec![deno.display().to_string()]);
        std::fs::remove_dir_all(root).unwrap();
    }
}

/// 用户确认后一次补齐，失败时必须报告，不能返回伪成功。
pub fn repair() -> Result<Vec<ShimFile>> {
    ensure_cli_copy()?;
    let files = sync_all()?;
    let left = missing()?;
    if !left.is_empty() { return Err(Error::Io(format!("以下转发文件仍不可用：{}", left.join("、")))); }
    Ok(files)
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
    if std::fs::read(dst).ok().zip(std::fs::read(src).ok()).is_some_and(|(a,b)| a == b) {
        return Ok(());
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
            return Err(Error::Io(format!("转发文件 {} 被占用，请关闭相关终端后重试", dst.display())));
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
        ShimTarget::Args(prefix_args) => Ok(Forward { program: record.exec_path.clone(), prefix_args }),
        ShimTarget::Module(module) => Ok(Forward {
            program: record.exec_path.clone(),
            prefix_args: vec!["-m".to_string(), module],
        }),
    }
}
