use std::fs::File;
use std::path::PathBuf;

fn main() {
    // 打包后的转发二进制在 bundle 里带着 target triple 命名，core 需要在运行时按同一个名字去找它。
    let target = std::env::var("TARGET").unwrap_or_default();
    if !target.is_empty() {
        println!("cargo:rustc-env=ENVSWITCH_TARGET={target}");
    }
    println!("cargo:rerun-if-env-changed=TARGET");

    // 转发用的控制台小程序不再随安装包单独带一份：scripts/stage-shim.mjs 先把它编出来放进
    // binaries/，这里把路径交给 include_bytes! 整个内嵌进主程序，运行时再释放到数据目录。
    // 发布包因此只有一个 exe，用户不会看见两个长得很像的程序摆在一起。
    let ext = if target.contains("windows") { ".exe" } else { "" };
    let manifest = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap_or_default());
    let blob = manifest.join("binaries").join(format!("envswitch-shim-{target}{ext}"));
    println!("cargo:rustc-env=ENVSWITCH_SHIM_BLOB={}", blob.display());
    // include_bytes! 不会自动盯着文件变化：不声明这一条，stage-shim 换了新内容而主程序不重编，
    // 内嵌的就还是旧字节。
    println!("cargo:rerun-if-changed={}", blob.display());

    // dev 构建允许空占位：include_bytes! 总得有文件可读，运行时发现是空的会退回按位置搜索。
    if !blob.exists() {
        if let Some(dir) = blob.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let _ = File::create(&blob);
    }
    let size = std::fs::metadata(&blob).map(|m| m.len()).unwrap_or(0);
    // 编转发程序自己时不卡这道门：它不内嵌 blob，而且全新克隆上第一份产物恰恰要靠它编出来。
    let is_shim_build = std::env::var("CARGO_FEATURE_SHIM").is_ok();
    if !is_shim_build && std::env::var("PROFILE").as_deref() == Ok("release") && size <= 1024 {
        panic!(
            "binaries/ 里没有编好的转发程序（{}，{size} 字节）；先跑 node scripts/stage-shim.mjs 再打包",
            blob.display()
        );
    }

    tauri_build::build()
}
