// 界面程序一律不占控制台：双击 exe（包括 debug 版）都不该弹黑窗。
// 核心逻辑都在 envswitch_lib 里，要在终端看输出请用 envswitch-shim，它是控制台程序。
#![windows_subsystem = "windows"]

use std::process::exit;

fn main() {
    // --guard 是内置守护：把自己当父进程，拉主程序、确认真起来了、挂了按次数重来。
    // 开机自启走这条，双击 exe 仍旧直接开主程序，任务管理器里不会平白多一个进程。
    if std::env::args().any(|a| a == "--guard") {
        exit(envswitch_lib::core::guardian::run_guard());
    }
    envswitch_lib::run();
}
