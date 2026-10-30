use std::fs::File;
use std::path::Path;

fn main() {
    // 打包后的转发二进制在 bundle 里带着 target triple 命名，core 需要在运行时按同一个名字去找它。
    let target = std::env::var("TARGET").unwrap_or_default();
    if !target.is_empty() {
        println!("cargo:rustc-env=ENVSWITCH_TARGET={target}");
    }
    println!("cargo:rerun-if-env-changed=TARGET");

    // tauri-build 在 dev 和 release 两条路上都会先检查 externalBin 的文件在不在，
    // 不在就直接把整个 cargo build 判失败，而这个文件恰恰要靠 cargo build 才能产出来。
    // 所以这里先补一个空的占位让检查通过，真实内容由 scripts/stage-shim.mjs 在打包前覆盖上去。
    // 运行时也不会误用空文件：core::shim 只认有一定体积的候选。
    let ext = if target.contains("windows") { ".exe" } else { "" };
    let sidecar = Path::new("binaries").join(format!("envswitch-shim-{target}{ext}"));
    if !sidecar.exists() {
        if let Some(dir) = sidecar.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let _ = File::create(&sidecar);
    }

    tauri_build::build()
}
