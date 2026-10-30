//! 启动可靠性：把"进程到底起来没有"留下证据，起不来得让人看见，守护模式负责重试。
//!
//! 界面程序没有控制台，release 又是 panic=abort，出问题双击一下就什么都不知道。
//! 这一层补三件事：落地的启动日志、看得见的失败提示、带重试的守护启动。

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use super::paths;

/// 启动日志的文件名；位置见 log_file()。
const LOG_NAME: &str = "envswitch-startup.log";
/// 守护模式给子进程留的便签路径：界面真跑起来了，子进程在这条路径上写一个字。
pub const READY_MARK_ENV: &str = "ENVSWITCH_READY_MARK";
/// 等不到便签就判定这次没拉起来。太短会把慢机器误杀，太长等于让用户干等。
const READY_TIMEOUT: Duration = Duration::from_secs(20);
/// 起来并活过这么久，之前攒的重试次数清零：那是正常使用中偶发崩溃，不是启动就炸。
const STABLE_AFTER: Duration = Duration::from_secs(60);
/// 连续拉不起来最多再试几次，超过就停下来报警，绝不无限拉起。
const MAX_RESTARTS: u32 = 3;
/// 轮询间隔：守护进程九成时间在睡觉，这个值只影响检测延迟。
const POLL: Duration = Duration::from_millis(150);
/// 临时目录里的便签超过这么久就当垃圾清掉（一轮便签最多活 20 秒）。
const MARK_TTL: Duration = Duration::from_secs(3600);

/// 日志位置：数据目录的 logs 优先，其次临时目录，最后 exe 同级；取第一个真写得进去的。
pub fn log_file() -> PathBuf {
    static CHOSEN: OnceLock<PathBuf> = OnceLock::new();
    CHOSEN
        .get_or_init(|| {
            let mut dirs = Vec::new();
            if let Ok(layout) = paths::layout() {
                dirs.push(layout.logs);
            }
            dirs.push(std::env::temp_dir().join("EnvSwitch"));
            dirs.push(paths::exe_dir().join("logs"));
            for dir in dirs {
                if fs::create_dir_all(&dir).is_err() {
                    continue;
                }
                let candidate = dir.join(LOG_NAME);
                if OpenOptions::new().create(true).append(true).open(&candidate).is_ok() {
                    return candidate;
                }
            }
            std::env::temp_dir().join("EnvSwitch").join(LOG_NAME)
        })
        .clone()
}

/// 往启动日志追加一行；日志写不进去不该影响程序本身，所以这里不报错。
pub fn note(msg: &str) {
    let line = format!("{}  {}\n", chrono::Local::now().format("%Y-%m-%d %H:%M:%S"), msg);
    if let Ok(mut f) = OpenOptions::new().create(true).append(true).open(log_file()) {
        let _ = f.write_all(line.as_bytes());
    }
}

/// 崩溃钩子：现场写进日志，再弹个框，别让"双击没反应"成为唯一的报错方式。
pub fn install_panic_hook() {
    static ONCE: OnceLock<()> = OnceLock::new();
    if ONCE.set(()).is_err() {
        return;
    }
    std::panic::set_hook(Box::new(|info| {
        let place = info
            .location()
            .map(|l| format!("{}:{}", l.file(), l.line()))
            .unwrap_or_else(|| "位置未知".to_string());
        let why = panic_text(info);
        note(&format!("异常退出 {place} {why}"));
        alert(
            "灵匣·EnvSwitch 出错了",
            &format!(
                "程序内部出错退出了。\n\n位置：{place}\n原因：{why}\n\n日志：{}\n\n点确定后请重新打开程序。",
                log_file().display()
            ),
        );
    }));
}

fn panic_text(info: &std::panic::PanicHookInfo<'_>) -> String {
    let any = info.payload();
    if let Some(s) = any.downcast_ref::<&str>() {
        (*s).to_string()
    } else if let Some(s) = any.downcast_ref::<String>() {
        s.clone()
    } else {
        "没带原因的退出".to_string()
    }
}

/// 界面活起来了：守护进程等的就是这个字。
pub fn mark_ready() {
    if let Ok(raw) = std::env::var(READY_MARK_ENV) {
        if !raw.trim().is_empty() {
            let _ = fs::write(raw.trim(), "ready\n");
        }
    }
    note("界面就绪");
}

/// 起不来时给人看的判断。只做提示，不拦启动：探测不到不等于真没装。
pub fn hint() -> String {
    if !webview2_ok() {
        return "这台电脑上没有 WebView2 运行库，界面画不出来。装一次就好，装完重新打开程序：\n\
https://developer.microsoft.com/microsoft-edge/webview2/"
            .to_string();
    }
    "运行库是齐的。常见原因是文件被安全软件拦下、程序所在盘读不了，或者数据目录出了问题。\n\
可以把上面那行日志的位置找到发给我们。"
        .to_string()
}

/// Windows 的 WebView2 运行库装了没有：读升级程序登记的版本号，顺带认一下 Edge 本体。
/// 非 Windows 走系统自带的内核，一律算有。
pub fn webview2_ok() -> bool {
    #[cfg(windows)]
    {
        pv_ok() || edge_present()
    }
    #[cfg(not(windows))]
    {
        true
    }
}

#[cfg(windows)]
fn pv_ok() -> bool {
    use winreg::enums::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, KEY_READ};
    use winreg::RegKey;
    // 这套 GUID 是 WebView2 运行库在 EdgeUpdate 下的登记名。
    // 独立安装多半登记在 32 位那一档，64 位与当前用户两档也要读，漏一档就会误判成"没装"。
    const GUID: &str = "{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}";
    let roots = [
        (HKEY_LOCAL_MACHINE, format!(r"SOFTWARE\WOW6432Node\Microsoft\EdgeUpdate\Clients\{GUID}")),
        (HKEY_LOCAL_MACHINE, format!(r"SOFTWARE\Microsoft\EdgeUpdate\Clients\{GUID}")),
        (HKEY_CURRENT_USER, format!(r"Software\Microsoft\EdgeUpdate\Clients\{GUID}")),
    ];
    roots.iter().any(|(hive, sub)| {
        RegKey::predef(*hive)
            .open_subkey_with_flags(sub, KEY_READ)
            .and_then(|k| k.get_value::<String, _>("pv"))
            .map(|pv| pv != "0.0.0.0" && !pv.trim().is_empty())
            .unwrap_or(false)
    })
}

#[cfg(windows)]
fn edge_present() -> bool {
    ["ProgramFiles(x86)", "ProgramFiles", "PROGRAMFILES"]
        .iter()
        .filter_map(|k| std::env::var(k).ok())
        .any(|root| Path::new(&root).join(r"Microsoft\Edge\Application\msedge.exe").is_file())
}

/// 弹窗：只在"起不来"这种界面已经没法用的场合用，平时一律走界面提示。
pub fn alert(title: &str, body: &str) {
    #[cfg(windows)]
    {
        use windows_sys::Win32::UI::WindowsAndMessaging::{
            MB_ICONERROR, MB_OK, MB_SETFOREGROUND, MessageBoxW,
        };
        let wide = |s: &str| -> Vec<u16> { s.encode_utf16().chain(std::iter::once(0)).collect() };
        let t = wide(title);
        let b = wide(body);
        // 传 NULL 宿主窗口：这时候窗口可能压根没建出来。
        unsafe { MessageBoxW(std::ptr::null_mut(), b.as_ptr(), t.as_ptr(), MB_OK | MB_ICONERROR | MB_SETFOREGROUND) };
    }
    #[cfg(not(windows))]
    {
        let _ = (title, body);
    }
}

/// 一次守护看护的结果。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Outcome {
    /// 界面起来了，之后正常退出（含托盘里点退出）。
    Normal,
    /// 界面还没起来就退 0：单实例插件发现已有实例，直接让位，这不算失败。
    OtherInstance,
    /// 到点还没起来：窗口多半卡住了。
    Timeout,
    /// 非零退出或被系统干掉（崩溃、结束任务）。
    Crash(Option<i32>),
}

/// 退出码 + "起来过没有"→ 结论。这条判定是整个守护的核心，单独拎出来才能测。
fn classify(ready: bool, code: Option<i32>) -> Outcome {
    match (ready, code) {
        (false, Some(0)) => Outcome::OtherInstance,
        (true, Some(0)) => Outcome::Normal,
        (true, None) | (false, None) => Outcome::Crash(None),
        (_, Some(_)) => Outcome::Crash(code),
    }
}

/// 守护：拉起主程序，确认它真起来了，中途挂了按次数重来。
/// 返回给操作系统的退出码。
pub fn run_guard() -> i32 {
    install_panic_hook();
    note(&format!(
        "守护启动 v{} pid={}",
        crate::core::VERSION,
        std::process::id()
    ));
    let exe = match std::env::current_exe() {
        Ok(p) => p,
        Err(e) => {
            let msg = format!("认不出自己在哪个文件：{e}");
            note(&msg);
            alert("灵匣·EnvSwitch 起不来", &msg);
            return 2;
        }
    };
    let nonce = chrono::Local::now().timestamp_nanos_opt().unwrap_or_default();
    sweep_stale_marks();
    let mut restarts: u32 = 0;
    loop {
        let mark = mark_path(restarts + 1, nonce);
        note(&format!("拉起主程序（第 {} 次）", restarts + 1));
        let spawned = Command::new(&exe)
            .env(READY_MARK_ENV, &mark)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn();
        let mut child = match spawned {
            Ok(c) => c,
            Err(e) => {
                note(&format!("拉起失败：{e}"));
                restarts += 1;
                if restarts > MAX_RESTARTS {
                    give_up(&format!("主程序连都起不来：{e}"));
                    return 1;
                }
                std::thread::sleep(Duration::from_secs(2));
                continue;
            }
        };
        let (outcome, stable) = supervise(&mut child, &mark);
        let _ = fs::remove_file(&mark);
        if stable {
            restarts = 0;
        }
        match outcome {
            Outcome::Normal => {
                note("主程序正常退出，守护结束");
                return 0;
            }
            Outcome::OtherInstance => {
                note("已经有程序在跑，守护结束");
                return 0;
            }
            Outcome::Timeout => {
                let _ = child.kill();
                let _ = child.wait();
                note("界面在限定时间内没起来");
                restarts += 1;
                if restarts > MAX_RESTARTS {
                    give_up("窗口建出来但界面一直没响应，重启几次都一样。");
                    return 1;
                }
            }
            Outcome::Crash(code) => {
                note(&format!("主程序异常退出，退出码 {:?}", code));
                restarts += 1;
                if restarts > MAX_RESTARTS {
                    give_up(&format!("主程序连续异常退出（最近退出码 {:?}）。", code));
                    return 1;
                }
            }
        }
        std::thread::sleep(Duration::from_secs(2));
    }
}

/// 盯着一轮：先等"界面就绪"的字，等字期间同时看进程有没有提前跑掉。
/// 第二个返回值是"这轮活够久"，用来决定要不要把重试次数清零。
fn supervise(child: &mut Child, mark: &Path) -> (Outcome, bool) {
    let started = Instant::now();
    let mut ready = false;
    loop {
        if !ready && mark.is_file() {
            ready = true;
            note("收到界面就绪的标记");
        }
        if let Ok(Some(status)) = child.try_wait() {
            return (classify(ready, status.code()), ready && started.elapsed() >= STABLE_AFTER);
        }
        if !ready && started.elapsed() >= READY_TIMEOUT {
            return (Outcome::Timeout, false);
        }
        std::thread::sleep(POLL);
    }
}

fn give_up(reason: &str) {
    note(&format!("放弃守护：{reason}"));
    alert(
        "灵匣·EnvSwitch 起不来",
        &format!(
            "试了几次都没能把程序拉起来。\n\n{}\n\n{}\n\n日志：{}",
            reason,
            hint(),
            log_file().display()
        ),
    );
}

/// 便签路径：进程号 + 本轮序号 + 一次守护启动的随机尾缀。
/// 光用进程号不够——PID 会被系统复用，上一任被硬杀的守护留下的便签，
/// 正好能让新一轮一眼读成"界面已经就绪"，把真起不来的情况放过去。
fn mark_named(pid: u32, nonce: i64, round: u32) -> PathBuf {
    std::env::temp_dir().join(format!("envswitch-ready-{pid}-{nonce}-{round}.ok"))
}

fn mark_path(round: u32, nonce: i64) -> PathBuf {
    mark_named(std::process::id(), nonce, round)
}

/// 一轮便签最多活 20 秒，超时就被删；临时目录里超过一小时的，只能是被硬杀的守护丢下的，删掉。
/// 界限放宽到一小时是为了绝不误伤同时在跑的另一任守护。
fn sweep_stale_marks() {
    let Ok(entries) = fs::read_dir(std::env::temp_dir()) else {
        return;
    };
    let cutoff = std::time::SystemTime::now()
        .checked_sub(MARK_TTL)
        .unwrap_or(std::time::SystemTime::UNIX_EPOCH);
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        if !name.starts_with("envswitch-ready-") || !name.ends_with(".ok") {
            continue;
        }
        let stale = entry
            .metadata()
            .and_then(|m| m.modified())
            .map(|t| t < cutoff)
            .unwrap_or(false);
        if stale {
            let _ = fs::remove_file(entry.path());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 后来者让位是退出 0，跟"正常用完了再退"只差在有没有过就绪标记；判反了会把在用着的程序杀掉。
    #[test]
    fn exit_code_zero_means_different_things_before_and_after_ready() {
        assert_eq!(classify(false, Some(0)), Outcome::OtherInstance);
        assert_eq!(classify(true, Some(0)), Outcome::Normal);
    }

    /// 同一任守护的两轮必须各用各的便签：名字撞了，上一轮没删净的字会让这一轮秒判"界面起来了"。
    #[test]
    fn each_round_gets_its_own_mark() {
        assert_ne!(mark_named(1234, 99, 1), mark_named(1234, 99, 2));
        assert_ne!(mark_named(1234, 99, 1), mark_named(1234, 100, 1));
        assert_ne!(mark_named(1234, 99, 1), mark_named(4321, 99, 1));
        let name = mark_named(1234, 99, 1).file_name().unwrap().to_string_lossy().to_string();
        assert!(name.starts_with("envswitch-ready-1234-"), "{name}");
        assert!(name.ends_with(".ok"), "{name}");
    }

    #[test]
    fn abnormal_exit_is_always_a_crash() {
        assert_eq!(classify(true, Some(1)), Outcome::Crash(Some(1)));
        // 访问违例在 Windows 上按有符号读回来就是负数，别把它当成"让位的后来者"。
        assert_eq!(classify(false, Some(-1073741819)), Outcome::Crash(Some(-1073741819)));
        assert_eq!(classify(true, None), Outcome::Crash(None));
        assert_eq!(classify(false, None), Outcome::Crash(None));
    }

    /// 这台机器上跑过一次就该有结果：日志路径必须落在一个真能建的目录里，且不能因为探测把数据目录建出来。
    #[test]
    fn log_file_is_a_path_under_a_directory_that_exists_or_can_be_made() {
        let p = log_file();
        assert_eq!(p.file_name().unwrap(), LOG_NAME);
        assert!(p.parent().unwrap().is_dir(), "日志目录没建出来：{}", p.display());
    }

    #[cfg(windows)]
    #[test]
    fn webview2_probe_reads_the_real_key() {
        // 判定为"有"时不能拦住启动；判定为"没有"时只在报错里出现，这条只保证探测本身不炸。
        let _ = webview2_ok();
    }
}
