//! 开机自启：Windows 上只写当前用户那一档启动项。
//!
//! 官方插件在进程带管理员权限时会把启动项写到"整台电脑"那一档，以后以普通权限运行既看不见
//! 也删不掉，等于留下一条没人负责的开机项。所以 Windows 这条路自己走注册表，别的系统仍用插件。
use std::path::PathBuf;

use super::error::{Error, Result};

/// 任务管理器"启动"页里显示的名字。
pub const ENTRY_NAME: &str = "EnvSwitch";

/// 打开或关掉开机自启。关掉时本来没有启动项不算失败。
pub fn set(on: bool) -> Result<()> {
    #[cfg(windows)]
    {
        let run = run_key()?;
        if on {
            run.set_value(ENTRY_NAME, &command())
                .map_err(|e| Error::Permission(format!("写不进启动项：{e}")))?;
        } else {
            let _ = run.delete_value(ENTRY_NAME);
        }
    }
    #[cfg(not(windows))]
    {
        let _ = on;
        return Err(Error::Unsupported("这个系统的开机自启没走这条路".to_string()));
    }
    Ok(())
}

/// 该写进启动项的那条命令：带引号，并且额外加 --guard。
/// 开机正是最容易起不来的时刻，让内置守护负责确认与重试。
#[cfg(windows)]
fn command() -> String {
    let exe = std::env::current_exe().unwrap_or_else(|_| PathBuf::from("envswitch.exe"));
    // 路径里有空格和中文，整条要带引号，否则系统只执行到第一个空格。
    format!("\"{}\" --guard", exe.display())
}

/// 启动项还在，但登记的是别的文件（装过两处、或挪过目录），或还是没带 --guard 的老一条：
/// 按现在这个程序重写。开机时静默打不开是这套守护最难发现的问题，所以宁可每次开机都核对一遍。
/// 关掉自启时启动项本来就没了，这里什么都不做。
#[cfg(windows)]
pub fn refresh_if_stale() -> Result<()> {
    let run = run_key()?;
    let wanted = command();
    let recorded: String = match run.get_value(ENTRY_NAME) {
        Ok(v) => v,
        Err(_) => return Ok(()),
    };
    if recorded == wanted {
        return Ok(());
    }
    run.set_value(ENTRY_NAME, &wanted)
        .map_err(|e| Error::Permission(format!("写不进启动项：{e}")))?;
    Ok(())
}

#[cfg(not(windows))]
pub fn refresh_if_stale() -> Result<()> {
    Ok(())
}

#[cfg(windows)]
fn run_key() -> Result<winreg::RegKey> {
    use winreg::enums::{HKEY_CURRENT_USER, KEY_READ, KEY_SET_VALUE};
    use winreg::RegKey;
    const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
    let root = RegKey::predef(HKEY_CURRENT_USER);
    root.open_subkey_with_flags(RUN_KEY, KEY_READ | KEY_SET_VALUE)
        .or_else(|_| root.create_subkey(RUN_KEY).map(|(k, _)| k))
        .map_err(|e| Error::Permission(format!("打不开当前用户的启动项：{e}")))
}
