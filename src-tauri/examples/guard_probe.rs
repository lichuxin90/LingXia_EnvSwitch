//! 自检用：把"起不来"时的现场三件套（日志、判断文案、弹窗）在真机上过一遍。
//! 不带参数只打印；alert 演示弹窗；panic 故意崩，验证钩子既落日志又弹框。
use envswitch_lib::core::guardian::{alert, hint, install_panic_hook, log_file, note, webview2_ok};

fn main() {
    let arg = std::env::args().nth(1).unwrap_or_default();
    match arg.as_str() {
        "alert" => {
            note("自检：弹窗演示");
            alert(
                "灵匣·EnvSwitch 起不来",
                &format!(
                    "试了几次都没能把程序拉起来。\n\n{}\n\n日志：{}",
                    hint(),
                    log_file().display()
                ),
            );
        }
        "panic" => {
            install_panic_hook();
            note("自检：故意崩之前先记一行");
            panic!("自检：故意崩一下，看现场有没有落到日志和弹窗");
        }
        _ => {
            println!("log={}", log_file().display());
            println!("webview2={}", webview2_ok());
            println!("hint={}", hint());
        }
    }
}
