// 把当前源码编译成可直接双击运行的一份，落在 publish/。
// 主程序必须带 custom-protocol，否则它是 dev 模式，会去连 devUrl 而不是内嵌的 dist。
// 转发程序由 stage-shim 先编出来再内嵌进主程序，所以发布目录里只有一个 exe。
import { execFileSync } from "node:child_process";
import { copyFileSync, mkdirSync, readFileSync, readdirSync, statSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const root = dirname(dirname(fileURLToPath(import.meta.url)));
const tauriDir = join(root, "src-tauri");
const out = join(root, "publish");
const run = (cmd, args) => execFileSync(cmd, args, { cwd: root, stdio: "inherit", shell: process.platform === "win32" });

run("npm", ["run", "build"]);
run("node", [join(root, "scripts", "stage-shim.mjs")]);
run("cargo", ["build", "--release", "--manifest-path", join(tauriDir, "Cargo.toml"), "--bin", "envswitch", "--features", "custom-protocol"]);

const targetRoot = (process.env.CARGO_TARGET_DIR || "").trim() || join(tauriDir, "target");
const release = join(targetRoot, "release");
mkdirSync(out, { recursive: true });

// Keep persistent user data and any prior release contents; only replace the app executable below.

copyFileSync(join(release, "envswitch.exe"), join(out, "envswitch.exe"));
for (const dll of readdirSync(release).filter((n) => n.toLowerCase().endsWith(".dll"))) {
  copyFileSync(join(release, dll), join(out, dll));
}

// 内嵌的转发字节必须和刚编出来的一致：cargo 若没因 blob 变化重编主程序，这里会抓住。
const staged = readdirSync(join(tauriDir, "binaries")).find((n) => /^envswitch-shim-.+\.exe$/.test(n));
if (!staged) throw new Error("src-tauri/binaries 里没有 stage-shim 的产物");
const blob = readFileSync(join(tauriDir, "binaries", staged));
if (!readFileSync(join(out, "envswitch.exe")).includes(blob)) {
  throw new Error("主程序里内嵌的转发程序不是最新编出来的那份，请重新构建");
}

for (const e of readdirSync(out, { withFileTypes: true })) {
  if (e.isFile()) console.log(`${e.name}\t${statSync(join(out, e.name)).size}`);
}
