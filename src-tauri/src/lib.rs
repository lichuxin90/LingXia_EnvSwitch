//! Tauri 层：把核心逻辑暴露成界面能调的命令，外加托盘、窗口与项目目录监听。
//!
//! 这一层只做参数转换和事件转发，判定全在 core 里，命令行程序（envswitch-shim）复用同一套。
pub mod core;

use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::Serialize;
use tauri::menu::{Menu, MenuBuilder, MenuItemBuilder, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::webview::WebviewWindowBuilder;
use tauri::{AppHandle, Emitter, Manager, RunEvent, WebviewUrl, WindowEvent};
use tauri_plugin_dialog::DialogExt;
use tauri_plugin_opener::OpenerExt;

use crate::core::aiclients;
use crate::core::detect::{self, PathView};
use crate::core::error::{Error, Result};
use crate::core::guardian;
use crate::core::hook::{self, HookStatus};
use crate::core::install;
use crate::core::job::{self, JobKind, JobView};
use crate::core::mirror::{self, ListOutcome};
use crate::core::model::{
    AvailableVersion, InstalledVersion, JobEvent, JobHandle, JobPhase, MirrorProbe, PathBackup,
    PathScope, PathStatus, ProjectLock, Resolution, ResolutionSource, ResetOutcome, ScanReport,
    ToolId, TransferSummary, VersionLine,
};
use crate::core::offline::{self, Guess};
use crate::core::paths::{self, Layout};
use crate::core::records::Registry;
use crate::core::reset::{self, Preview};
use crate::core::settings::{ProxyMode, Settings, TrayBehavior};
use crate::core::shim::{self, ShimFile};
use crate::core::tool::{Plugin, SourceInfo};
use crate::core::{pathmod, toolversions, transfer, update};

/// 进度事件：下载、校验、解压都往这里发。
const EV_JOB: &str = "envswitch://job";
/// 装完/接管完一个新版本，界面刷新列表。
const EV_INSTALLED: &str = "envswitch://installed";
/// 设置变了（托盘里切了全局锁定等）。
const EV_SETTINGS: &str = "envswitch://settings";
/// 被盯住的项目目录里的版本声明变了。
const EV_PROJECT: &str = "envswitch://project";

/// 项目目录监听句柄，界面切目录时替换。
#[derive(Default)]
struct Watcher(std::sync::Mutex<Option<notify::RecommendedWatcher>>);

pub fn run() {
    // 先把日志和崩溃钩子架上：后面哪一步炸了都要留下证据，界面程序没控制台，不记就等于没发生。
    guardian::install_panic_hook();
    guardian::note(&format!(
        "主程序启动 v{} pid={}",
        core::VERSION,
        std::process::id()
    ));
    if !guardian::webview2_ok() {
        // 只记一笔，不拦：探测不到不代表真没装，拦下来会把能用的程序判死。
        guardian::note("启动前探测：没找到 WebView2 运行库的登记项");
    }
    match serve() {
        Ok(app) => app.run(|_app, event| {
            // 关窗只是隐藏，进程留着；真正退出走托盘菜单，PATH 与 shim 是持久配置，无需收尾。
            if let RunEvent::ExitRequested { .. } = event {
                // 留空：退出前没有需要清理的临时状态。
            }
        }),
        Err(e) => {
            let msg = e.to_string();
            guardian::note(&format!("主程序启动失败：{msg}"));
            guardian::alert(
                "灵匣·EnvSwitch 起不来",
                &format!(
                    "程序没能建出窗口。\n\n{}\n\n{}\n\n日志：{}\n\n如果是守护模式拉起的，它会再试几次后停下报警。",
                    msg,
                    guardian::hint(),
                    guardian::log_file().display()
                ),
            );
            std::process::exit(1);
        }
    }
}

/// 建窗口、装托盘、注册命令。失败一律往上交，由 run() 负责让人看见。
fn serve() -> tauri::Result<tauri::App> {
    let app = tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            show_main(app);
        }))
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            Some(vec![]),
        ))
        .manage(Watcher::default())
        .manage(ShellLang::default())
        .invoke_handler(tauri::generate_handler![
            boot,
            choose_data_root,
            data_root_candidates,
            reveal_path,
            open_link,
            list_versions,
            sources_of,
            probe_sources,
            installed_versions,
            install_version,
            import_offline,
            precheck_offline,
            adopt_runtime,
            uninstall_version,
            switch_version,
            cancel_job,
            active_jobs,
            get_settings,
            update_settings,
            test_proxy,
            set_autostart,
            path_status,
            path_apply,
            path_withdraw,
            path_reorder,
            path_backups,
            path_restore,
            shim_list,
            shim_sync,
            env_scan,
            path_view,
            hook_status,
            hook_set,
            dir_resolution,
            lock_project,
            unlock_project,
            registered_projects,
            forget_project,
            set_watch_dir,
            set_global_lock,
            pick_folder,
            pick_files,
            pick_save_path,
            reset_preview,
            reset_now,
            export_config,
            import_config,
            config_suggested_name,
            check_update,
            ai_detect,
            ai_write_file,
            ai_remove_file,
            ai_write_rules,
            ai_remove_rules,
            app_ready,
            startup_log,
            set_shell_lang
        ])
        .setup(|app| {
            build_tray(app)?;
            // 数据目录已就绪就把转发文件补齐：换过机器或挪过目录都不会留下半套 shim。
            if paths::layout().is_ok() {
                let _ = shim::sync_all();
            }
            // 启动项里还登记着别的路径（挪过安装位置、或老版本没带 --guard），按现在这个文件重写。
            // 开机时静默打不开是最难发现的问题，宁可每次启动都核对一遍。
            let _ = core::autostart::refresh_if_stale();
            make_main(app)?;
            Ok(())
        })
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                let quit = Settings::load()
                    .map(|s| s.tray_behavior == TrayBehavior::Quit)
                    .unwrap_or(true);
                if quit {
                    window.app_handle().exit(0);
                } else {
                    let _ = window.hide();
                    api.prevent_close();
                }
            }
        })
        .build(tauri::generate_context!())?;
    guardian::note("托盘与主窗口都建好了");
    Ok(app)
}

fn show_main(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.unminimize();
        let _ = w.show();
        let _ = w.set_focus();
    }
}

/// 主窗口在代码里建，不在配置里建：配置项的 `dataDirectory` 只收相对路径（相对 C 盘的
/// 本地数据目录），而浏览器内核的缓存要跟着数据目录走，只有这里能给绝对路径。
fn make_main(app: &tauri::App) -> tauri::Result<()> {
    let (w, h, min_w, min_h) = fit_to_screen(app);
    let mut builder = WebviewWindowBuilder::new(app, "main", WebviewUrl::App("index.html".into()))
        .title("灵匣·EnvSwitch")
        .inner_size(w, h)
        .min_inner_size(min_w, min_h)
        .resizable(true)
        // 成品不给开发者工具：右键菜单里的"检查"和 F12 一起关掉，开发构建照常留着。
        .devtools(cfg!(debug_assertions))
        .center();
    if let Some(dir) = paths::webview_data_dir() {
        builder = builder.data_directory(dir);
    }
    builder.build()?;
    Ok(())
}

/// 一块屏的可用地面换算成逻辑像素：`(宽, 高)`。
fn logical_work_area(m: &tauri::Monitor) -> (f64, f64) {
    let scale = m.scale_factor().max(1.0);
    (
        f64::from(m.work_area().size.width) / scale,
        f64::from(m.work_area().size.height) / scale,
    )
}

/// 首选 1180×760，最小 940×600，但都不许越过这块屏的可用地面。
/// `(宽, 高, 最小宽, 最小高)`
fn fit_to_screen(app: &tauri::App) -> (f64, f64, f64, f64) {
    // `center()` 把窗口开在主屏正中，所以按主屏算；拿不到主屏才退而求其次找最大的一块。
    let room = app
        .primary_monitor()
        .ok()
        .flatten()
        .as_ref()
        .map(logical_work_area)
        .or_else(|| {
            app.available_monitors().ok().and_then(|list| {
                list.iter()
                    .max_by_key(|m| {
                        u64::from(m.work_area().size.width) * u64::from(m.work_area().size.height)
                    })
                    .map(logical_work_area)
            })
        })
        // 读不到屏幕信息时按 1080p 无缩放兜底，别把窗口开成零点。
        .unwrap_or((1920.0, 1040.0));
    clamp_to_room(room.0, room.1)
}

/// 想要的尺寸撞上一块小屏时怎么退：默认值、最小值都往回收，但绝不收成一个不能拖的框。
///
/// 逻辑像素在缩放过 1080p 的机器上比想象中小：150% 缩放后整屏只剩 1280×720 逻辑，
/// 写死的 760 高一来就顶出屏幕，用户第一眼看到的是切掉的底边，写死的 600 最小值还会
/// 把窗口锁死在那个高度上调不回去。
fn clamp_to_room(avail_w: f64, avail_h: f64) -> (f64, f64, f64, f64) {
    const WANT_W: f64 = 1180.0;
    const WANT_H: f64 = 760.0;
    const MIN_W: f64 = 940.0;
    const MIN_H: f64 = 600.0;
    /// 兜底下限：再小就装不下侧边栏加一张卡了，宁可让它略微顶出屏幕也要保住能用的界面。
    const FLOOR_W: f64 = 480.0;
    const FLOOR_H: f64 = 360.0;
    /// 四周各留 24 逻辑像素，顶边不压任务栏，底边留一条能抓到的边框。
    const MARGIN: f64 = 48.0;

    let max_w = (avail_w - MARGIN).max(FLOOR_W);
    let max_h = (avail_h - MARGIN).max(FLOOR_H);
    let w = WANT_W.min(max_w);
    let h = WANT_H.min(max_h);
    // 最小值必须比默认值小，否则等于把窗口焊死。
    let min_w = MIN_W.min((w - 40.0).max(FLOOR_W)).min(w);
    let min_h = MIN_H.min((h - 40.0).max(FLOOR_H)).min(h);
    (w, h, min_w, min_h)
}

#[cfg(test)]
mod size_tests {
    use super::clamp_to_room;

    /// 大屏拿回原样的 1180×760：这次改动不该顺手把老机器的窗口也缩小。
    #[test]
    fn big_screen_keeps_the_wanted_size() {
        assert_eq!(clamp_to_room(2560.0, 1400.0), (1180.0, 760.0, 940.0, 600.0));
    }

    /// 1080p 挂 150% 缩放：逻辑工作区 1280×680，旧写法给的 760 高比屏幕还高。
    #[test]
    fn hdpi_1080p_fits_the_screen() {
        let (w, h, min_w, min_h) = clamp_to_room(1280.0, 680.0);
        assert!(h <= 680.0, "高 {h} 顶出屏幕");
        assert!(w <= 1280.0, "宽 {w} 顶出屏幕");
        assert!(min_h < h && min_w < w, "最小值把窗口焊死了：{min_w}×{min_h} vs {w}×{h}");
    }

    /// 1024×768 的老投影仪：两头都得收，但收完还得是能拖动的窗口。
    #[test]
    fn tiny_screen_stays_draggable() {
        let (w, h, min_w, min_h) = clamp_to_room(1024.0, 738.0);
        assert_eq!((w, h), (976.0, 690.0), "该跟着屏幕收一档");
        assert!(w - min_w >= 40.0 && h - min_h >= 40.0, "{w}×{h} 没有可拖的余量");
    }

    /// 从 640×480 扫到 4K：任何一块屏都不许出现「最小值 ≥ 默认值」。
    #[test]
    fn no_size_locks_itself() {
        let mut w_avail = 640.0;
        while w_avail <= 4000.0 {
            let h_avail = w_avail * 0.6;
            let (w, h, min_w, min_h) = clamp_to_room(w_avail, h_avail);
            assert!(min_w <= w && min_h <= h, "{w_avail} 时 {min_w}×{min_h} > {w}×{h}");
            assert!(w > 0.0 && h > 0.0, "{w_avail} 时窗口开成了零点");
            w_avail += 64.0;
        }
    }
}

/// 托盘：常驻 + 右键菜单（打开 / 全局锁定 / 退出）。
fn build_tray(app: &mut tauri::App) -> tauri::Result<()> {
    let menu = tray_menu(&*app, shell_text(app.handle()))?;
    let mut tray = TrayIconBuilder::with_id("main-tray")
        .menu(&menu)
        .tooltip("灵匣·EnvSwitch")
        .on_menu_event(|app, event| match event.id().as_ref() {
            "open" => show_main(app),
            "lock" => {
                let _ = toggle_lock_from_tray(app);
            }
            "quit" => app.exit(0),
            _ => {}
        })
        .show_menu_on_left_click(false)
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                show_main(tray.app_handle());
            }
        });
    if let Some(icon) = app.default_window_icon() {
        tray = tray.icon(icon.clone());
    }
    tray.build(app)?;
    Ok(())
}

/// 菜单里"锁定"那条按当前全局锁定状态选词：已经锁着就该给"解除"，两句不能并存。
fn tray_menu<R, M>(app: &M, txt: ShellText) -> tauri::Result<Menu<R>>
where
    R: tauri::Runtime,
    M: tauri::Manager<R>,
{
    let locked = Settings::load().map(|s| s.lock_global).unwrap_or(false);
    let open = MenuItemBuilder::with_id("open", txt.open).build(app)?;
    let lock =
        MenuItemBuilder::with_id("lock", if locked { txt.unlock } else { txt.lock }).build(app)?;
    let sep = PredefinedMenuItem::separator(app)?;
    let quit = MenuItemBuilder::with_id("quit", txt.quit).build(app)?;
    MenuBuilder::new(app)
        .items(&[&open, &sep, &lock, &sep, &quit])
        .build()
}

fn toggle_lock_from_tray(app: &AppHandle) -> Result<()> {
    let mut s = Settings::load()?;
    s.lock_global = !s.lock_global;
    s.save()?;
    let _ = app.emit(EV_SETTINGS, &s);
    // 菜单里那条是"锁/解锁"的动作，状态一变文字就得跟着变，不然下次点的是同一句假话。
    let _ = refresh_tray(app);
    Ok(())
}

// ---------------------------------------------------------------- 托盘与标题的语言

/// 托盘、窗口标题这类"界面外面"的文字，前端够不着，只能在这份小表里翻。
/// 语言由前端算好后通过 set_shell_lang 送进来，"跟随系统"这种情况也一并解决。
#[derive(Clone, Copy)]
struct ShellText {
    open: &'static str,
    lock: &'static str,
    unlock: &'static str,
    quit: &'static str,
    title: &'static str,
}

const SHELL_TEXTS: &[(&str, ShellText)] = &[
    (
        "zh-CN",
        ShellText {
            open: "打开主界面",
            lock: "锁定全局版本",
            unlock: "解除全局锁定",
            quit: "退出 EnvSwitch",
            title: "灵匣·EnvSwitch",
        },
    ),
    (
        "zh-TW",
        ShellText {
            open: "開啟主介面",
            lock: "鎖定全域版本",
            unlock: "解除全域鎖定",
            quit: "結束 EnvSwitch",
            title: "靈匣·EnvSwitch",
        },
    ),
    (
        "en",
        ShellText {
            open: "Open Main Window",
            lock: "Lock Global Version",
            unlock: "Unlock Global Version",
            quit: "Quit EnvSwitch",
            title: "LingXia EnvSwitch",
        },
    ),
    (
        "ja",
        ShellText {
            open: "メイン画面を開く",
            lock: "グローバル版をロック",
            unlock: "グローバル版のロックを解除",
            quit: "EnvSwitch を終了",
            title: "灵匣·EnvSwitch",
        },
    ),
];

/// 前端算出来的当前语言；没送进来之前按简体中文显示。
#[derive(Default)]
struct ShellLang(std::sync::Mutex<String>);

fn shell_text(app: &AppHandle) -> ShellText {
    let lang = app
        .state::<ShellLang>()
        .0
        .lock()
        .map(|g| g.clone())
        .unwrap_or_default();
    SHELL_TEXTS
        .iter()
        .find(|(k, _)| *k == lang.as_str())
        .map(|(_, v)| *v)
        .unwrap_or(SHELL_TEXTS[0].1)
}

fn refresh_tray(app: &AppHandle) -> tauri::Result<()> {
    let txt = shell_text(app);
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.set_title(txt.title);
    }
    let menu = tray_menu(app, txt)?;
    if let Some(tray) = app.tray_by_id("main-tray") {
        tray.set_menu(Some(menu))?;
    }
    Ok(())
}

#[tauri::command]
fn set_shell_lang(app: AppHandle, lang: String) -> Result<()> {
    if let Ok(mut cur) = app.state::<ShellLang>().0.lock() {
        *cur = lang;
    }
    refresh_tray(&app).map_err(|e| Error::Detail(e.to_string()))
}

/// 界面跑通了（首屏出来了）由前端来报这一笔：守护模式等的就是它，日志里也靠它分段。
/// 报晚了不亏，报早了才亏，所以这条只管记，不做任何判断。
#[tauri::command]
fn app_ready() {
    guardian::mark_ready();
}

/// 启动日志在哪儿，界面上的"打开启动日志"用它。
#[tauri::command]
fn startup_log() -> String {
    guardian::log_file().display().to_string()
}

// ---------------------------------------------------------------- 首屏与数据目录

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct Boot {
    version: String,
    platform: &'static str,
    needs_setup: bool,
    layout: Option<Layout>,
    candidates: Vec<paths::Candidate>,
    settings: Settings,
    path: Vec<PathStatus>,
    elevated: bool,
    jobs: Vec<JobView>,
    hooks: Vec<HookStatus>,
    shims: Vec<ShimFile>,
    /// 首屏提示语：数据目录没选好、终端集成没开，直接写在这里。
    note: Option<String>,
}

#[tauri::command]
fn boot() -> Boot {
    let layout = paths::layout().ok();
    let settings = Settings::load().unwrap_or_default();
    let path = path_statuses().unwrap_or_default();
    let note = if layout.is_none() {
        Some("先选一个存放运行时的文件夹；系统盘和桌面都不合适".to_string())
    } else if settings.auto_switch && !hooks_any_installed() {
        Some("终端集成还没开，新开终端里的 node、python 暂时还不走 EnvSwitch".to_string())
    } else {
        None
    };
    Boot {
        version: crate::core::VERSION.to_string(),
        platform: crate::core::platform(),
        needs_setup: layout.is_none(),
        candidates: data_root_candidates(),
        shims: layout.as_ref().and_then(|_| shim::list().ok()).unwrap_or_default(),
        hooks: hook::status().unwrap_or_default(),
        path,
        elevated: pathmod::is_elevated(),
        jobs: job::list(),
        note,
        layout,
        settings,
    }
}

fn hooks_any_installed() -> bool {
    hook::status()
        .map(|list| list.iter().any(|h| h.installed))
        .unwrap_or(false)
}

fn path_statuses() -> Result<Vec<PathStatus>> {
    let (user, system) = pathmod::both_statuses()?;
    let mut out = Vec::new();
    if let Some(s) = system {
        out.push(s);
    }
    out.push(user);
    Ok(out)
}

#[tauri::command]
fn data_root_candidates() -> Vec<paths::Candidate> {
    paths::root_candidates()
}

#[tauri::command]
fn choose_data_root(dir: String) -> Result<Layout> {
    let path = PathBuf::from(dir.trim().trim_matches('"'));
    if path.as_os_str().is_empty() {
        return Err(Error::Detail("请先填一个文件夹路径".to_string()));
    }
    let layout = paths::adopt_root(&path)?;
    let _ = shim::sync_all();
    let _ = hook::refresh_scripts();
    Ok(layout)
}

#[tauri::command]
fn reveal_path(app: AppHandle, path: Option<String>) -> Result<()> {
    let target = match path {
        Some(p) => p,
        None => paths::layout()?.root.display().to_string(),
    };
    app.opener()
        .open_path(target, None::<&str>)
        .map_err(|e| Error::Detail(format!("打不开那个文件夹：{e}")))
}

#[tauri::command]
fn open_link(app: AppHandle, url: String) -> Result<()> {
    app.opener()
        .open_url(url, None::<&str>)
        .map_err(|e| Error::Detail(format!("打不开链接：{e}")))
}

// ---------------------------------------------------------------- 版本清单

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct VersionPage {
    lines: Vec<VersionLine>,
    /// 拉平的版本列表，搜索框按它过滤
    flat: Vec<AvailableVersion>,
    source_id: String,
    source_label: String,
    fetched_at: Option<String>,
    stale_note: Option<String>,
    installed: Vec<InstalledVersion>,
    default_version: Option<String>,
}

#[tauri::command]
async fn list_versions(
    tool: ToolId,
    refresh: Option<bool>,
    source: Option<String>,
) -> Result<VersionPage> {
    let settings = Settings::load()?;
    let plugin = Plugin::for_tool(tool)?;
    let client = mirror::build_client(&settings)?;
    let src = mirror::resolve_source(&client, &plugin, &settings, source.as_deref()).await?;
    let outcome: ListOutcome =
        mirror::versions(&client, &plugin, &src, refresh.unwrap_or(false)).await?;
    let registry = Registry::load()?;
    Ok(VersionPage {
        lines: install::group_lines(tool, &outcome.versions, &registry),
        flat: outcome.versions,
        source_id: outcome.source_id,
        source_label: outcome.source_label,
        fetched_at: outcome.fetched_at,
        stale_note: outcome.stale_note,
        installed: install::installed(tool)?,
        default_version: registry.default_of(tool),
    })
}

#[tauri::command]
fn sources_of(tool: ToolId) -> Result<Vec<SourceInfo>> {
    Ok(Plugin::for_tool(tool)?.sources())
}

#[tauri::command]
async fn probe_sources(tool: ToolId) -> Result<Vec<MirrorProbe>> {
    let settings = Settings::load()?;
    let plugin = Plugin::for_tool(tool)?;
    let client = mirror::probe_client(&settings)?;
    Ok(mirror::probe_sources(&client, &plugin).await)
}

// ---------------------------------------------------------------- 安装与卸载

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct InstalledPage {
    tool: ToolId,
    versions: Vec<InstalledVersion>,
    default_version: Option<String>,
    resolution: Resolution,
}

#[tauri::command]
fn installed_versions(tool: ToolId, dir: Option<String>) -> Result<InstalledPage> {
    let settings = Settings::load()?;
    let registry = Registry::load()?;
    let cwd = dir_of(dir);
    Ok(InstalledPage {
        tool,
        versions: install::installed(tool)?,
        default_version: registry.default_of(tool),
        resolution: toolversions::resolve_with(&settings, &registry, tool, &cwd),
    })
}

fn dir_of(dir: Option<String>) -> PathBuf {
    dir.filter(|d| !d.trim().is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_default())
}

fn emitter(app: &AppHandle) -> Arc<dyn Fn(&JobEvent) + Send + Sync + 'static> {
    let handle = app.clone();
    Arc::new(move |ev: &JobEvent| {
        let _ = handle.emit(EV_JOB, ev.clone());
    })
}

/// 任务失败时补发一条只带说明的事件，界面直接把这句话显示在进度条位置。
fn emit_failure(app: &AppHandle, id: &str, tool: ToolId, version: &str, e: &Error) {
    let phase = if matches!(e, Error::Cancelled) {
        JobPhase::Cancelled
    } else {
        JobPhase::Failed
    };
    let _ = app.emit(
        EV_JOB,
        JobEvent {
            job_id: id.to_string(),
            tool: Some(tool),
            version: Some(version.to_string()),
            phase,
            received: 0,
            total: None,
            bytes_per_sec: 0,
            eta_secs: None,
            percent: None,
            message: Some(e.to_string()),
        },
    );
}

#[tauri::command]
async fn install_version(
    app: AppHandle,
    tool: ToolId,
    version: String,
    source: Option<String>,
    force: Option<bool>,
) -> Result<JobHandle> {
    let job = job::register(tool, &version, JobKind::Install);
    let handle = JobHandle {
        job_id: job.id.clone(),
        tool,
        version: version.clone(),
        phase: JobPhase::Queued,
    };
    let settings = Settings::load()?;
    let client = mirror::build_client(&settings)?;
    let id = job.id.clone();
    tauri::async_runtime::spawn(async move {
        match install::install(
            &client,
            Arc::clone(&job),
            Some(emitter(&app)),
            tool,
            &version,
            source.as_deref(),
            force.unwrap_or(false),
        )
        .await
        {
            Ok(item) => {
                job.finish(JobPhase::Done, None);
                let _ = app.emit(EV_INSTALLED, &item);
            }
            Err(e) => {
                job.finish(
                    if matches!(e, Error::Cancelled) { JobPhase::Cancelled } else { JobPhase::Failed },
                    Some(e.to_string()),
                );
                emit_failure(&app, &id, tool, &version, &e);
            }
        }
    });
    Ok(handle)
}

#[tauri::command]
async fn import_offline(app: AppHandle, files: Vec<String>) -> Result<Vec<JobHandle>> {
    let mut out = Vec::new();
    for raw in files {
        let path = PathBuf::from(&raw);
        let g = offline::guess(&path)?;
        let tool = g.tool.ok_or_else(|| {
            Error::Detail(
                g.why_not
                    .clone()
                    .unwrap_or_else(|| "认不出这个包属于哪个工具".to_string()),
            )
        })?;
        let version = g
            .version
            .clone()
            .ok_or_else(|| Error::Detail("文件名里读不出版本号".to_string()))?;
        let job = job::register(tool, &version, JobKind::Import);
        let id = job.id.clone();
        out.push(JobHandle {
            job_id: id.clone(),
            tool,
            version: version.clone(),
            phase: JobPhase::Queued,
        });
        let events = emitter(&app);
        let handle = app.clone();
        tauri::async_runtime::spawn(async move {
            match offline::import(Some(events), Arc::clone(&job), &path, None, None, None).await {
                Ok(item) => {
                    job.finish(JobPhase::Done, None);
                    let _ = handle.emit(EV_INSTALLED, &item);
                }
                Err(e) => {
                    job.finish(
                        if matches!(e, Error::Cancelled) { JobPhase::Cancelled } else { JobPhase::Failed },
                        Some(e.to_string()),
                    );
                    emit_failure(&handle, &id, tool, &version, &e);
                }
            }
        });
    }
    Ok(out)
}

/// 拖进来先预检，界面据此决定"装"这个按钮能不能点。
#[tauri::command]
fn precheck_offline(files: Vec<String>) -> Vec<Guess> {
    let paths: Vec<PathBuf> = files.iter().map(PathBuf::from).collect();
    offline::precheck(&paths)
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct Adopted {
    installed: InstalledVersion,
    referenced: bool,
    note: String,
}

/// 接管现成安装：copy=false 只登记原目录，copy=true 复制进数据目录。
#[tauri::command]
async fn adopt_runtime(
    tool: ToolId,
    exe: String,
    version: Option<String>,
    copy: bool,
) -> Result<Adopted> {
    let path = PathBuf::from(&exe);
    let result = tauri::async_runtime::spawn_blocking(move || {
        detect::adopt(tool, &path, version.as_deref(), copy)
    })
    .await
    .map_err(|e| Error::Detail(format!("接管任务中断：{e}")))?;
    let installed = result?;
    Ok(Adopted {
        referenced: !copy,
        note: if copy {
            "已复制进数据目录，原目录确认没用后可以自行删除".to_string()
        } else {
            "只登记了原目录，原文件保持不动；在 EnvSwitch 里卸载不会删你的东西".to_string()
        },
        installed,
    })
}

#[tauri::command]
fn uninstall_version(tool: ToolId, version: String, purge: Option<bool>) -> Result<String> {
    install::uninstall(tool, &version, purge.unwrap_or(false))
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct Switched {
    tool: ToolId,
    version: String,
    resolution: Resolution,
    note: Option<String>,
}

#[tauri::command]
fn switch_version(tool: ToolId, version: String) -> Result<Switched> {
    let note = install::switch_default(tool, &version)?;
    let settings = Settings::load()?;
    let registry = Registry::load()?;
    let cwd = std::env::current_dir().unwrap_or_default();
    Ok(Switched {
        tool,
        version,
        resolution: toolversions::resolve_with(&settings, &registry, tool, &cwd),
        note,
    })
}

#[tauri::command]
fn cancel_job(job_id: String) -> Result<()> {
    job::cancel(&job_id)
}

#[tauri::command]
fn active_jobs() -> Vec<JobView> {
    job::list()
}

// ---------------------------------------------------------------- 设置

#[tauri::command]
fn get_settings() -> Result<Settings> {
    Settings::load()
}

#[tauri::command]
fn update_settings(app: AppHandle, patch: Settings) -> Result<Settings> {
    let old = Settings::load().unwrap_or_default();
    patch.save()?;
    if old.path_scope != patch.path_scope {
        let _ = pathmod::apply(patch.path_scope);
    } else if old.shim_priority != patch.shim_priority {
        let _ = pathmod::reorder(patch.path_scope);
    }
    if old.autostart != patch.autostart {
        apply_autostart(&app, patch.autostart)?;
    }
    if old.auto_switch != patch.auto_switch {
        let _ = hook::apply_setting(patch.auto_switch);
    }
    let _ = app.emit(EV_SETTINGS, &patch);
    Ok(patch)
}

#[tauri::command]
async fn test_proxy(mode: ProxyMode, url: Option<String>) -> Result<String> {
    let mut s = Settings::load()?;
    s.proxy_mode = mode;
    s.proxy_url = url.filter(|u| !u.trim().is_empty());
    let client = mirror::probe_client(&s)?;
    let plugin = Plugin::for_tool(ToolId::Node)?;
    let src = plugin.source("official")?;
    let started = std::time::Instant::now();
    client
        .get(src.probe_url)
        .send()
        .await
        .map_err(|e| Error::Network(format!("按这个代理设置连不上官方清单：{e}")))?;
    Ok(format!("{} 毫秒", started.elapsed().as_millis()))
}

fn apply_autostart(app: &AppHandle, on: bool) -> Result<()> {
    #[cfg(windows)]
    {
        // Windows 自己写当前用户的启动项：插件在有管理员权限时会写到整台电脑那一档，
        // 那条目以后普通权限既读不到也删不掉。
        let _ = app;
        return core::autostart::set(on);
    }
    #[cfg(not(windows))]
    {
        use tauri_plugin_autostart::ManagerExt;
        let manager = app.autolaunch();
        if on {
            manager
                .enable()
                .map_err(|e| Error::Detail(format!("开不了开机自启：{e}")))?;
        } else {
            // 本来就没开时 disable 会报"找不到项"，那不算失败。
            let _ = manager.disable();
        }
        Ok(())
    }
}

#[tauri::command]
fn set_autostart(app: AppHandle, on: bool) -> Result<bool> {
    apply_autostart(&app, on)?;
    let mut s = Settings::load()?;
    s.autostart = on;
    s.save()?;
    Ok(on)
}

// ---------------------------------------------------------------- PATH 与 shim

#[tauri::command]
fn path_status() -> Result<Vec<PathStatus>> {
    path_statuses()
}

#[tauri::command]
fn path_apply(scope: PathScope) -> Result<PathStatus> {
    pathmod::apply(scope)
}

#[tauri::command]
fn path_withdraw(scope: PathScope) -> Result<PathStatus> {
    pathmod::withdraw(scope)
}

#[tauri::command]
fn path_reorder(scope: PathScope) -> Result<PathStatus> {
    pathmod::reorder(scope)
}

#[tauri::command]
fn path_backups() -> Result<Vec<PathBackup>> {
    pathmod::list_backups()
}

#[tauri::command]
fn path_restore(id: String) -> Result<PathStatus> {
    pathmod::restore(&id)
}

#[tauri::command]
fn shim_list() -> Result<Vec<ShimFile>> {
    shim::list()
}

#[tauri::command]
fn shim_sync() -> Result<Vec<ShimFile>> {
    shim::sync_all()
}

// ---------------------------------------------------------------- 体检与终端集成

#[tauri::command]
fn env_scan() -> Result<ScanReport> {
    detect::scan()
}

#[tauri::command]
fn path_view() -> Result<PathView> {
    detect::path_view()
}

#[tauri::command]
fn hook_status() -> Result<Vec<HookStatus>> {
    hook::status()
}

#[tauri::command]
fn hook_set(shell: String, on: bool) -> Result<Vec<HookStatus>> {
    if on {
        hook::install(&shell)?;
    } else {
        hook::remove(&shell)?;
    }
    hook::status()
}

// ---------------------------------------------------------------- 项目目录

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct DirContext {
    dir: PathBuf,
    exists: bool,
    lock: Option<ProjectLock>,
    resolutions: Vec<Resolution>,
    /// 界面提示语，逐行列出这个目录里各工具会用哪个版本
    summary: Vec<String>,
}

#[tauri::command]
fn dir_resolution(dir: Option<String>) -> Result<DirContext> {
    let settings = Settings::load()?;
    let registry = Registry::load()?;
    let raw = dir
        .filter(|d| !d.trim().is_empty())
        .map(PathBuf::from)
        .or_else(|| settings.last_project_dir.as_ref().map(PathBuf::from))
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_default());
    let exists = raw.is_dir();
    let lock = if exists {
        toolversions::read_project(&raw)?
    } else {
        None
    };
    let mut resolutions = Vec::new();
    let mut summary = Vec::new();
    for tool in ToolId::ALL {
        let r = toolversions::resolve_with(&settings, &registry, tool, &raw);
        match (&r.version, r.source) {
            (Some(v), ResolutionSource::ProjectFile) => {
                summary.push(format!("在这个目录里 {} 用 {}", tool.display(), v))
            }
            (Some(v), ResolutionSource::OnlyInstalled) => {
                summary.push(format!("{} 用 {}（本机只装了这一个）", tool.display(), v))
            }
            (Some(v), _) => summary.push(format!("{} 用 {}（全局默认）", tool.display(), v)),
            (None, _) => summary.push(format!("{} 还没有可用版本", tool.display())),
        }
        resolutions.push(r);
    }
    Ok(DirContext {
        dir: raw,
        exists,
        lock,
        resolutions,
        summary,
    })
}

#[tauri::command]
fn lock_project(dir: String, tool: ToolId, version: String) -> Result<ProjectLock> {
    let path = PathBuf::from(&dir);
    if !path.is_dir() {
        return Err(Error::NotFound(format!("{} 不是个文件夹", path.display())));
    }
    let registry = Registry::load()?;
    if registry.find(tool, &version).is_none() {
        return Err(Error::NotInstalled(tool.display().to_string(), version));
    }
    let lock = toolversions::lock(&path, tool, &version)?;
    toolversions::register_project(&path)?;
    let mut s = Settings::load()?;
    s.last_project_dir = Some(path.display().to_string());
    s.save()?;
    Ok(lock)
}

#[tauri::command]
fn unlock_project(dir: String, tool: ToolId) -> Result<Option<ProjectLock>> {
    toolversions::unlock(Path::new(&dir), tool)
}

#[tauri::command]
fn registered_projects() -> Result<Vec<ProjectLock>> {
    let mut out = Vec::new();
    for dir in toolversions::registered_projects()? {
        if let Some(lock) = toolversions::read_project(&dir)? {
            out.push(lock);
        }
    }
    Ok(out)
}

#[tauri::command]
fn forget_project(dir: String) -> Result<()> {
    toolversions::unregister_project(Path::new(&dir))
}

/// 盯住某个项目目录：版本声明一变就通知界面，配合托盘常驻用。
#[tauri::command]
fn set_watch_dir(
    app: AppHandle,
    watcher: tauri::State<Watcher>,
    dir: Option<String>,
) -> Result<()> {
    let mut guard = watcher
        .0
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    *guard = None;
    let Some(raw) = dir.filter(|d| !d.trim().is_empty()) else {
        return Ok(());
    };
    let path = PathBuf::from(raw);
    if !path.is_dir() {
        return Err(Error::NotFound(format!("{} 不是个文件夹", path.display())));
    }
    use notify::{Event, RecommendedWatcher, RecursiveMode, Watcher as _};
    let handle = app.clone();
    let watched = path.clone();
    let mut w: RecommendedWatcher = notify::recommended_watcher(
        move |res: std::result::Result<Event, notify::Error>| {
            let Ok(event) = res else { return };
            let touched = event.paths.iter().any(|p| {
                p.file_name()
                    .map(|f| f.to_string_lossy().starts_with(".tool-versions"))
                    .unwrap_or(false)
            });
            if touched {
                let _ = handle.emit(EV_PROJECT, &watched);
            }
        },
    )
    .map_err(|e| Error::Detail(format!("盯不住这个目录：{e}")))?;
    w.watch(&path, RecursiveMode::NonRecursive)
        .map_err(|e| Error::Detail(format!("盯不住这个目录：{e}")))?;
    *guard = Some(w);
    Ok(())
}

#[tauri::command]
fn set_global_lock(locked: bool) -> Result<()> {
    install::set_global_lock(locked)
}

// ---------------------------------------------------------------- 文件选择

/// 原生对话框在主线程外调用，用一次性通道把结果送回命令。
async fn ask_folder(app: AppHandle) -> Option<String> {
    let dialog = app.dialog().clone();
    let (tx, rx) = tokio::sync::oneshot::channel();
    std::thread::spawn(move || {
        let _ = tx.send(dialog.file().blocking_pick_folder());
    });
    rx.await.ok().flatten().and_then(plain_path)
}

async fn ask_save(app: AppHandle, name: String) -> Option<String> {
    let dialog = app.dialog().clone();
    let (tx, rx) = tokio::sync::oneshot::channel();
    std::thread::spawn(move || {
        let _ = tx.send(dialog.file().set_file_name(&name).blocking_save_file());
    });
    rx.await.ok().flatten().and_then(plain_path)
}

fn plain_path(fp: tauri_plugin_dialog::FilePath) -> Option<String> {
    match fp {
        tauri_plugin_dialog::FilePath::Path(p) => Some(p.display().to_string()),
        tauri_plugin_dialog::FilePath::Url(u) => Some(u.to_string()),
    }
}

#[tauri::command]
async fn pick_folder(app: AppHandle) -> Option<String> {
    ask_folder(app).await
}

#[tauri::command]
async fn pick_save_path(app: AppHandle, name: String) -> Option<String> {
    ask_save(app, name).await
}

#[tauri::command]
async fn pick_files(app: AppHandle) -> Vec<String> {
    let dialog = app.dialog().clone();
    let (tx, rx) = tokio::sync::oneshot::channel();
    std::thread::spawn(move || {
        let picked = dialog
            .file()
            .add_filter("运行时装载包", &["zip", "gz", "tar", "tgz"])
            .blocking_pick_files();
        let _ = tx.send(picked);
    });
    rx.await
        .ok()
        .flatten()
        .map(|list| list.into_iter().filter_map(plain_path).collect())
        .unwrap_or_default()
}

// ---------------------------------------------------------------- 重置与配置搬迁

#[tauri::command]
fn reset_preview() -> Result<Preview> {
    reset::preview()
}

#[tauri::command]
fn reset_now(keep_installs: bool) -> Result<ResetOutcome> {
    reset::reset(keep_installs)
}

#[tauri::command]
fn config_suggested_name() -> String {
    transfer::suggested_name()
}

#[tauri::command]
fn export_config(dest: String) -> Result<TransferSummary> {
    transfer::export(Path::new(&dest))
}

#[tauri::command]
fn import_config(src: String, apply_settings: Option<bool>) -> Result<transfer::ImportOutcome> {
    transfer::import(Path::new(&src), apply_settings.unwrap_or(true))
}

// ---------------------------------------------------------------- 检查更新与 AI 客户端

#[tauri::command]
async fn check_update() -> Result<update::UpdateInfo> {
    update::check().await
}

#[tauri::command]
fn ai_detect() -> Vec<aiclients::AiClient> {
    aiclients::detect()
}

/// 写我们自己的独立规则文件：默认走这条，绝不碰用户配好的说明文件。
/// `head` 是客户端硬要求的文件头部（Cursor/Kiro 那类只认 frontmatter 的），没有就传空串。
#[tauri::command]
fn ai_write_file(dir: String, file: String, head: String, content: String) -> Result<String> {
    aiclients::write_standalone(&dir, &file, &head, &content)
}

/// 撤掉我们写的那份独立文件：只认自己的记号，别人的文件删不掉。
#[tauri::command]
fn ai_remove_file(dir: String, file: String) -> Result<String> {
    aiclients::remove_standalone(&dir, &file)
}

/// 往客户端的主说明文件里加/换一段，用标记包住；只有用户明确要求时才走这条。
#[tauri::command]
fn ai_write_rules(dir: String, file: String, content: String) -> Result<String> {
    aiclients::write_rules(&dir, &file, &content)
}

/// 撤掉主说明文件里我们那一段，用户写的内容原样留着。
#[tauri::command]
fn ai_remove_rules(dir: String, file: String) -> Result<String> {
    aiclients::remove_rules(&dir, &file)
}
