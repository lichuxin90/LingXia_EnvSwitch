//! 非 Windows 的 PATH 落地。
//!
//! macOS/Linux 没有注册表这种统一入口：当前用户的 PATH 写进 `~/.profile` 的托管片段，
//! 全体用户的 PATH 写进 `/etc/paths.d/`（需要 root）。读的时候一律以进程实际拿到的 PATH 为准，
//! 界面显示"现在生效的是什么"比"配置文件里写了什么"更有用。
use std::io::Write as _;
use std::path::PathBuf;

use super::error::{Error, Result};
use super::model::PathScope;

const MARK_BEGIN: &str = "# >>> 灵匣·EnvSwitch PATH >>>";
const MARK_END: &str = "# <<< 灵匣·EnvSwitch PATH <<<";
const SYSTEM_FILE: &str = "/etc/paths.d/envswitch";

pub fn is_root() -> bool {
    std::process::Command::new("id")
        .arg("-u")
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim() == "0")
        .unwrap_or(false)
}

pub fn read_path(scope: PathScope) -> Result<String> {
    let _ = scope;
    std::env::var("PATH")
        .or_else(|_| std::env::var("Path"))
        .map_err(|_| Error::NotFound("这个进程没拿到 PATH 变量".to_string()))
}

pub fn write_path(scope: PathScope, raw: &str) -> Result<()> {
    match scope {
        PathScope::User => write_profile(raw),
        PathScope::System => write_paths_d(raw),
    }
}

fn profile_path() -> Result<PathBuf> {
    dirs::home_dir()
        .map(|h| h.join(".profile"))
        .ok_or_else(|| Error::Config("找不到当前用户的目录".to_string()))
}

/// 用托管标记替换整段，标记外的内容一个字都不碰。
fn write_profile(raw: &str) -> Result<()> {
    let path = profile_path()?;
    let old = std::fs::read_to_string(&path).unwrap_or_default();
    let kept = cut_block(&old);
    let mut text = kept.trim_end().to_string();
    if !text.is_empty() {
        text.push('\n');
    }
    text.push_str(&format!("\n{MARK_BEGIN}\nexport PATH=\"{raw}\"\n{MARK_END}\n"));
    let mut file = std::fs::File::create(&path)
        .map_err(|e| Error::Permission(format!("写不进 {}：{e}", path.display())))?;
    file.write_all(text.as_bytes())
        .map_err(|e| Error::Permission(format!("写不进 {}：{e}", path.display())))?;
    Ok(())
}

fn write_paths_d(raw: &str) -> Result<()> {
    if !is_root() {
        return Err(Error::Permission(
            "改全体用户的 PATH 需要管理员，请用 sudo 启动一次 EnvSwitch".to_string(),
        ));
    }
    let body = raw.replace(':', "\n");
    std::fs::write(SYSTEM_FILE, format!("{body}\n"))
        .map_err(|e| Error::Permission(format!("写不进 {SYSTEM_FILE}：{e}")))
}

fn cut_block(text: &str) -> String {
    let mut out = String::new();
    let mut inside = false;
    for line in text.lines() {
        if line.trim_end() == MARK_BEGIN {
            inside = true;
            continue;
        }
        if line.trim_end() == MARK_END {
            inside = false;
            continue;
        }
        if !inside {
            out.push_str(line);
            out.push('\n');
        }
    }
    out
}
