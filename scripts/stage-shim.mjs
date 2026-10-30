// tauri 只是把 externalBin 指过去的文件原样装进安装包，它不会替我们编译 envswitch-shim。
// 这一步在编译主程序之前把转发用的控制台二进制编出来，并写成 tauri 认的命名：<名字>-<target triple>[.exe]。
// 交叉编译时由 CI 用 ENVSWITCH_TARGET 把三元组递进来；本地不传就走 rustc 的 host，
// 顺带也不加 --target，产物直接落在 target/release 里，和主程序共用同一份编译缓存。
import { execFileSync } from "node:child_process";
import { chmodSync, copyFileSync, mkdirSync, statSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const root = dirname(dirname(fileURLToPath(import.meta.url)));
const tauriDir = join(root, "src-tauri");

const crossTarget = (process.env.ENVSWITCH_TARGET || "").trim();
const triple =
  crossTarget ||
  execFileSync("rustc", ["-vV"], { encoding: "utf8" }).match(/^host:\s*(.+)$/m)[1].trim();
const ext = triple.includes("windows") ? ".exe" : "";

const args = ["build", "--release", "--manifest-path", join(tauriDir, "Cargo.toml"), "--bin", "envswitch-shim", "--features", "custom-protocol"];
if (crossTarget) args.push("--target", crossTarget);
execFileSync("cargo", args, { stdio: "inherit" });

const built = join(tauriDir, "target", ...(crossTarget ? [crossTarget] : []), "release", `envswitch-shim${ext}`);
const binaries = join(tauriDir, "binaries");
const dst = join(binaries, `envswitch-shim-${triple}${ext}`);
mkdirSync(binaries, { recursive: true });
copyFileSync(built, dst);
chmodSync(dst, 0o755);

// build.rs 可能留了个空占位在同一个路径上；没被真实二进制覆盖就别说"已就位"。
const size = statSync(dst).size;
if (size <= 1024) {
  throw new Error(`${dst} 只有 ${size} 字节，不像编译出来的二进制`);
}
console.log(`转发用的二进制已就位：${dst}（${Math.round(size / 1024)} KiB）`);
