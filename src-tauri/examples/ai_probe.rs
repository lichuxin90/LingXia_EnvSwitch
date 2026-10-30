//! 自检用：把本机检测到的 AI 客户端清单原样打出来，只看得到装了哪些、在哪个盘。
use envswitch_lib::core::aiclients::detect;

fn main() {
    for c in detect() {
        println!(
            "{}\tfound={}\tconfig={:?}\tinstall={:?}",
            c.id, c.found, c.path, c.install
        );
    }
}
