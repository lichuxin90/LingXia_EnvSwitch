//! 命令行自检：不开界面也能验证下载/安装/登记这条链路。
//! 用法：cargo run --example probe_versions -- list node
//!       cargo run --example probe_versions -- install node 22.23.4
//!       cargo run --example probe_versions -- resolve <某个目录>
use std::sync::Arc;

use envswitch_lib::core::install;
use envswitch_lib::core::job::{self, JobKind};
use envswitch_lib::core::mirror;
use envswitch_lib::core::model::{JobEvent, ToolId};
use envswitch_lib::core::settings::Settings;
use envswitch_lib::core::tool::Plugin;

fn tool_of(name: &str) -> ToolId {
    match name {
        "python" => ToolId::Python,
        _ => ToolId::Node,
    }
}

fn show(ev: &JobEvent) {
    println!(
        "  [ {:<11} ] {} {} {}% {} {}",
        format!("{:?}", ev.phase),
        ev.tool.map(|t| format!("{t:?}")).unwrap_or_default(),
        ev.version.clone().unwrap_or_default(),
        ev.percent.map(|p| p as i64).unwrap_or(-1),
        envswitch_lib::core::util::human_bytes(ev.received),
        ev.message.clone().unwrap_or_default(),
    );
}

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let cmd = args.first().map(String::as_str).unwrap_or("list");
    println!("数据目录 = {:?}", envswitch_lib::core::paths::layout().map(|l| l.root));

    let settings = match Settings::load() {
        Ok(s) => s,
        Err(e) => return println!("Settings::load 失败: {e:?}"),
    };
    let client = match mirror::build_client(&settings) {
        Ok(c) => c,
        Err(e) => return println!("build_client 失败: {e:?}"),
    };

    match cmd {
        "install" => {
            let tool = tool_of(args.get(1).map(String::as_str).unwrap_or("node"));
            let version = args.get(2).map(String::as_str).unwrap_or("22.23.4");
            let job = job::register(tool, version, JobKind::Install);
            let on: envswitch_lib::core::download::OnEvent = Arc::new(show);
            match install::install(&client, Arc::clone(&job), Some(on), tool, version, None, false).await {
                Ok(v) => println!(
                    "装好了：{} {} 在 {}\n可执行 {}",
                    v.tool.display(),
                    v.version,
                    v.dir.display(),
                    v.exec_path.display()
                ),
                Err(e) => println!("安装失败: {e:?}"),
            }
        }
        "switch" => {
            let tool = tool_of(args.get(1).map(String::as_str).unwrap_or("node"));
            let version = args.get(2).map(String::as_str).unwrap_or("");
            match install::switch_default(tool, version) {
                Ok(note) => println!("已设为全局默认，附带说明 {note:?}"),
                Err(e) => println!("切换失败: {e:?}"),
            }
        }
        "shims" => {
            let l = envswitch_lib::core::paths::layout().unwrap();
            let count = |tag: &str| match std::fs::read_dir(&l.shims) {
                Ok(rd) => println!("  {tag}: {:?} 条目 {} 个", l.shims, rd.flatten().count()),
                Err(e) => println!("  {tag}: {:?} 打不开：{e}", l.shims),
            };
            count("同步前");
            match envswitch_lib::core::shim::sync_all() {
                Ok(files) => {
                    for f in files {
                        println!("  {:<12} {:<8} {}", f.name, if f.exists { "已生成" } else { "缺" }, f.path.display());
                    }
                }
                Err(e) => println!("生成转发文件失败: {e:?}"),
            }
            count("同步后");
        }
        "pathcheck" => {
            use envswitch_lib::core::model::PathScope;
            for scope in [PathScope::User, PathScope::System] {
                let tag = match scope {
                    PathScope::User => "当前用户",
                    PathScope::System => "整台电脑",
                };
                let Ok(raw) = envswitch_lib::core::pathmod::read_raw(scope) else {
                    println!("  {tag}: 读不到 PATH");
                    continue;
                };
                let pieces: Vec<&str> = raw.split(';').collect();
                let empties = pieces.iter().filter(|p| p.trim().is_empty()).count();
                println!(
                    "  {tag}: 原值 {} 字符 / 有效条目 {} / 空条目 {}（写入时逐字保留）",
                    raw.chars().count(),
                    envswitch_lib::core::util::split_path_list(&raw).len(),
                    empties,
                );
                match envswitch_lib::core::pathmod::status(scope) {
                    Ok(st) => println!(
                        "    shim 在位={} 排最前={} 位置={:?} 备份数={} 管理员={}",
                        st.installed, st.is_first, st.position, st.backup_count, st.elevated
                    ),
                    Err(e) => println!("    status 失败: {e:?}"),
                }
            }
        }
        "pathtest" => {
            use envswitch_lib::core::model::PathScope;
            use envswitch_lib::core::pathmod;

            let scope = PathScope::User;
            let original = pathmod::read_raw(scope).expect("读不到当前用户 PATH");
            let shim_dir = envswitch_lib::core::paths::layout().unwrap().shims;
            let head = shim_dir.to_string_lossy().to_string();
            println!("原值 {} 字符，转发目录 {}", original.chars().count(), head);

            let mut bad = 0usize;
            let mut say = |cond: bool, label: &str| {
                println!("  [{}] {label}", if cond { "通过" } else { "失败" });
                if !cond {
                    bad += 1;
                }
            };

            let st = pathmod::apply(scope).expect("写入失败");
            let after = pathmod::read_raw(scope).unwrap();
            let expect = format!("{head};{original}");
            say(st.installed, "写入后 PATH 里有转发目录");
            say(after == expect, "原值逐字保留，只在最前面多了一条");

            let backups = pathmod::list_backups().unwrap();
            let newest = backups.iter().find(|b| b.scope == scope).expect("没有备份");
            let expect_prev: Vec<String> =
                envswitch_lib::core::util::split_path_list(&original).into_iter().take(6).collect();
            say(newest.preview == expect_prev, "改动前自动存下的备份就是原来的值");

            // 新终端拿到的是"整台电脑 + 当前用户"合并后的顺序，界面报的名次要跟它对得上。
            let view = envswitch_lib::core::detect::path_view().expect("读不到合并后的 PATH");
            let flat = head.trim_end_matches(['\\', '/']).to_lowercase();
            let idx = view
                .entries
                .iter()
                .position(|e| e.trim_end_matches(['\\', '/']).to_lowercase() == flat);
            say(view.shim_present, "合并后的 PATH 里能找到转发目录");
            say(st.position == idx, &format!("界面名次与合并后的真实顺序一致（合并表第 {:?} 位）", idx));

            let eff = view.entries.join(";");
            match std::process::Command::new("node")
                .args(["-e", "console.log(process.execPath)"])
                .env("PATH", &eff)
                .output()
            {
                Ok(out) => {
                    let got = String::from_utf8_lossy(&out.stdout).trim().to_string();
                    let ours = got.to_lowercase().contains("envswitch-data");
                    println!("    按合并顺序起进程，node 落在 {got}（{}）", if ours { "我们的转发目录" } else { "别的安装" });
                    say(view.shim_first == ours, "只有转发目录真排第一时才会由我们接管，界面结论与实际一致");
                }
                Err(e) => println!("    起不了探测进程：{e}"),
            }

            let st2 = pathmod::withdraw(scope).expect("摘掉失败");
            let back = pathmod::read_raw(scope).unwrap();
            say(!st2.installed, "摘掉后 PATH 里已经没有转发目录");
            say(back == original, "摘掉后逐字等于原值");
            if back != original {
                println!("    原 {} 字符 / 现 {} 字符", original.chars().count(), back.chars().count());
            }

            // 再写一次，然后用备份回滚，验证回滚这条路也落回原值。
            let st3 = pathmod::apply(scope).expect("再次写入失败");
            say(st3.installed, "第二次写入成功");
            let id = pathmod::list_backups()
                .unwrap()
                .into_iter()
                .find(|b| b.scope == scope)
                .map(|b| b.id)
                .expect("没有备份可用");
            let st4 = pathmod::restore(&id).expect("回滚失败");
            let rolled = pathmod::read_raw(scope).unwrap();
            say(rolled == original, &format!("用备份 {id} 回滚后逐字等于原值"));
            say(!st4.installed, "回滚后转发目录已不在 PATH");
            say(pathmod::list_backups().unwrap().len() >= 2, "回滚自己也留了一份备份");

            println!("收尾：当前用户 PATH {} 字符（基线 {} 字符），备份 {} 份，失败项 {bad}", rolled.chars().count(), original.chars().count(), st4.backup_count);
            if bad > 0 {
                std::process::exit(1);
            }
        }
        "hookcheck" => {
            for h in envswitch_lib::core::hook::status().unwrap() {
                println!(
                    "  {:<16} 认得出={} 已装上={} 脚本就绪={} 登录文件={}",
                    h.label,
                    h.detected,
                    h.installed,
                    h.script_ready,
                    h.rc_file.map(|p| p.display().to_string()).unwrap_or_else(|| "找不到位置".to_string())
                );
                if let Some(n) = h.note {
                    println!("      提示：{n}");
                }
            }
        }
        "hookapply" => {
            use envswitch_lib::core::hook;
            let what = args.get(1).map(String::as_str).unwrap_or("on");
            let id = args.get(2).map(String::as_str).unwrap_or("powershell");
            let h = match what {
                "on" => hook::install(id),
                "off" => hook::remove(id),
                other => {
                    println!("  只认 on / off，收到 {other}");
                    return;
                }
            };
            match h {
                Ok(h) => {
                    println!(
                        "  {} 已装上={} 脚本就绪={} 登录文件={}",
                        h.label,
                        h.installed,
                        h.script_ready,
                        h.rc_file.map(|p| p.display().to_string()).unwrap_or_default()
                    );
                    if let Some(n) = h.note {
                        println!("      提示：{n}");
                    }
                }
                Err(e) => println!("  失败：{e}"),
            }
        }
        "roots" => {
            for c in envswitch_lib::core::paths::root_candidates() {
                println!(
                    "  [{}] {} {}{}",
                    c.rank,
                    c.dir,
                    c.label,
                    c.note.map(|n| format!("　{n}")).unwrap_or_default()
                );
            }
            if args.get(1).map(String::as_str) != Some("adopt") {
                return println!("  只列候选；要试落盘：roots adopt <目录>");
            }
            let chosen = match args.get(2) {
                Some(d) => d,
                None => return println!("  adopt 后面要跟一个目录"),
            };
            let before = std::fs::metadata(chosen).is_err();
            let layout = match envswitch_lib::core::paths::adopt_root(std::path::Path::new(chosen)) {
                Ok(l) => l,
                Err(e) => return println!("  adopt 失败：{e}"),
            };
            println!("  落在：{} 同级={}", layout.root.display(), layout.beside_exe);
            for d in [
                &layout.installs,
                &layout.shims,
                &layout.cache,
                &layout.backups,
                &layout.config,
                &layout.logs,
            ] {
                println!("  {} 在={}", d.display(), d.is_dir());
            }
            let loc = envswitch_lib::core::paths::exe_dir()
                .join(envswitch_lib::core::paths::LOCATION_FILE_NAME);
            println!(
                "  指路文件 {} 写了={} 内容={:?}",
                loc.display(),
                loc.is_file(),
                std::fs::read_to_string(&loc).ok().map(|s| s.trim().to_string())
            );
            if before {
                println!("  （这次新建了 {}，验完请自己删）", chosen);
            }
        }
        "offline" => {
            use envswitch_lib::core::offline;
            let sub = args.get(1).map(String::as_str).unwrap_or("precheck");
            let files: Vec<String> = args.iter().skip(2).cloned().collect();
            for raw in &files {
                let path = std::path::PathBuf::from(raw);
                let g = match offline::guess(&path) {
                    Ok(g) => g,
                    Err(e) => {
                        println!("  {raw} 认不出：{e}");
                        continue;
                    }
                };
                println!(
                    "  {} → {:?} {:?} 包型={} 能装={} {}",
                    g.file_name,
                    g.tool,
                    g.version,
                    g.kind,
                    g.ok,
                    g.why_not.unwrap_or_default()
                );
                if sub != "import" {
                    continue;
                }
                let (Some(tool), Some(version)) = (g.tool, g.version.clone()) else {
                    println!("    工具或版本没认出来，不硬装");
                    continue;
                };
                let job = job::register(tool, &version, job::JobKind::Import);
                let on: envswitch_lib::core::download::OnEvent = Arc::new(show);
                match offline::import(Some(on), Arc::clone(&job), &path, None, None, None).await {
                    Ok(v) => println!(
                        "    导入完成：{} {} 在 {}\n    可执行 {}",
                        v.tool.display(),
                        v.version,
                        v.dir.display(),
                        v.exec_path.display()
                    ),
                    Err(e) => println!("    导入失败：{e:?}"),
                }
            }
        }
        "consolecheck" => {
            // 用法：Start-Process 起一个新控制台跑 "consolecheck <结果文件>"，
            // 再在管道里跑一次，对比两种场合的行为差别。
            use envswitch_lib::core::console;
            let out = args.get(1).cloned().unwrap_or_default();
            let (before, on_console) = unsafe {
                use windows_sys::Win32::System::Console::{
                    GetConsoleMode, GetConsoleOutputCP, GetStdHandle, STD_OUTPUT_HANDLE,
                };
                let mut mode = 0;
                (
                    GetConsoleOutputCP(),
                    GetConsoleMode(GetStdHandle(STD_OUTPUT_HANDLE), &mut mode) != 0,
                )
            };
            let guard = console::use_utf8();
            let got = guard.is_some();
            let during = unsafe { windows_sys::Win32::System::Console::GetConsoleOutputCP() };
            drop(guard);
            let restored = unsafe { windows_sys::Win32::System::Console::GetConsoleOutputCP() };
            let text = format!(
                "on_console={on_console} 切前={before} 切后={during} 还原后={restored} 拿到句柄={got}\n",
            );
            if out.is_empty() {
                print!("{text}");
            } else {
                std::fs::write(&out, text).expect("写结果文件失败");
            }
        }
        "resolve" => {
            let dir = std::path::Path::new(args.get(1).map(String::as_str).unwrap_or("."));
            for tool in [ToolId::Node, ToolId::Python] {
                match envswitch_lib::core::toolversions::resolve(tool, dir) {
                    Ok(r) => println!(
                        "  {} => {:?} 依据 {:?}",
                        tool.display(),
                        r.version,
                        r.source
                    ),
                    Err(e) => println!("  {} 解析失败: {e:?}", tool.display()),
                }
            }
        }
        _ => {
            let tool = tool_of(args.get(1).map(String::as_str).unwrap_or("node"));
            let plugin = Plugin::for_tool(tool).unwrap();
            println!("source 配置 = {:?}", settings.sources);
            let src = match mirror::resolve_source(&client, &plugin, &settings, None).await {
                Ok(s) => s,
                Err(e) => return println!("resolve_source 失败: {e:?}"),
            };
            println!("选中源 = {} {}", src.id, src.base);
            match mirror::versions(&client, &plugin, &src, false).await {
                Ok(out) => {
                    println!("版本条数 = {}，清单时间 = {:?}", out.versions.len(), out.fetched_at);
                    let registry = envswitch_lib::core::records::Registry::load().unwrap();
                    let lines = install::group_lines(tool, &out.versions, &registry);
                    println!("分组 = {}", lines.len());
                    for l in lines.iter().take(6) {
                        let lts_flag = match l.lts_active {
                            Some(true) => "在维护",
                            Some(false) => "已过维护期",
                            None => "未知",
                        };
                        println!(
                            "  线 {} 最新 {} lts={} 代号={:?} 在维护={} 成员={} 已装={}",
                            l.major,
                            l.newest,
                            l.is_lts,
                            l.lts_codename,
                            lts_flag,
                            l.member_count,
                            l.installed
                        );
                    }
                }
                Err(e) => println!("versions 失败: {e:?}"),
            }
        }
    }
}
