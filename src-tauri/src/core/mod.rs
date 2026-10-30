//! 与 Tauri 无关的核心逻辑。shim 可执行文件也链接这一层，所以这里不许出现 UI 依赖。

pub mod archive;
pub mod aiclients;
pub mod autostart;
pub mod console;
pub mod detect;
pub mod download;
pub mod error;
pub mod guardian;
pub mod hook;
pub mod install;
pub mod job;
pub mod mirror;
pub mod model;
pub mod offline;
pub mod pathmod;
pub mod paths;
pub mod records;
pub mod reset;
pub mod settings;
pub mod shim;
pub mod tool;
pub mod toolversions;
pub mod transfer;
#[cfg(not(windows))]
pub mod unix;
pub mod update;
pub mod util;

pub use error::{Error, Result};

/// 界面与"关于"页展示的版本号，取自 Cargo 元数据。
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// 当前系统名，界面用来核对"下错包"这种情况。
pub fn platform() -> &'static str {
    if cfg!(windows) {
        "windows"
    } else if cfg!(target_os = "macos") {
        "macos"
    } else {
        "linux"
    }
}
