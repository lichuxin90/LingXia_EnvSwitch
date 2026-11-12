//! 转发程序：放在 shims 目录里的多入口小程序。
//!
//! 它是控制台程序：Windows 上 GUI 主程序没有控制台，用它转发才能保住 node 的输出。
//! 同一个文件按自己的文件名决定行为——叫 node.exe 就转发 Node，
//! 叫 envswitch-shim.exe 或 envswitch-forward.exe 就当作命令行工具。
use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::process::{ExitCode, Stdio};

use envswitch_lib::core::error::{Error, Result};
use envswitch_lib::core::model::{Resolution, ResolutionSource, ToolId};
use envswitch_lib::core::records::Registry;
use envswitch_lib::core::settings::Settings;
use envswitch_lib::core::tool::Plugin;
use envswitch_lib::core::{console, hook, install, paths, pathmod, shim, toolversions};

const EXIT_NOT_FOUND: u8 = 127;

fn main() -> ExitCode {
    let mut argv = std::env::args_os();
    let me = argv
        .next()
        .unwrap_or_else(|| OsStr::new("envswitch-shim").to_os_string());
    let rest: Vec<String> = argv.map(|s| s.to_string_lossy().to_string()).collect();
    let name = file_stem_lower(&me);
    // 转发别的命令时不碰代码页：那时屏幕上该显示的是 node、python 自己的输出。
    let _utf8 = if name == "envswitch-shim" || name == shim::FORWARD_STEM || name.is_empty() {
        console::use_utf8()
    } else {
        None
    };
    let code = match name.as_str() {
        "" => fail(&Error::Detail("认不出自己的文件名".to_string())),
        // 数据目录 bin/ 里释放出来的模板叫 envswitch-forward，提权写系统 PATH 用的就是它。
        "envswitch-shim" | shim::FORWARD_STEM => run_cli(&rest),
        _ => run_forward(&name, &rest),
    };
    ExitCode::from(code)
}

fn file_stem_lower(path: &OsStr) -> String {
    Path::new(path)
        .file_stem()
        .map(|s| s.to_string_lossy().to_lowercase())
        .unwrap_or_default()
}

/// 转发名属于哪个工具；npm、pip 这类附带命令也归各自工具管。
fn owner(name: &str) -> Option<ToolId> {
    if name == "composer" { return Some(ToolId::Php); }
    Plugin::all()
        .into_iter()
        .find(|p| p.shims().iter().any(|s| s.name == name))
        .map(|p| p.id())
}

fn run_forward(name: &str, args: &[String]) -> u8 {
    let Some(tool) = owner(name) else {
        return fail(&Error::Detail(format!("不认识 {name} 这个转发名")));
    };
    let settings = match Settings::load() {
        Ok(s) => s,
        Err(e) => return fail(&e),
    };
    let registry = match Registry::load() {
        Ok(r) => r,
        Err(e) => return fail(&e),
    };
    let cwd = match current_dir() {
        Ok(d) => d,
        Err(e) => return fail(&e),
    };
    let res = toolversions::resolve_with(&settings, &registry, tool, &cwd);
    let Some(version) = res.version else {
        eprintln!("EnvSwitch：{} 还没有可用的版本。", tool.display());
        if let Some(note) = res.note {
            eprintln!("EnvSwitch：{note}");
        }
        return EXIT_NOT_FOUND;
    };
    let fwd = if name == "composer" {
        let Some(phar) = find_composer_phar() else {
            eprintln!("EnvSwitch：没有在 PATH 中找到 Composer（composer.phar）；请先安装 Composer 并重试。");
            return EXIT_NOT_FOUND;
        };
        shim::Forward { program: registry.find(tool, &version).expect("resolved PHP version").exec_path.clone(), prefix_args: vec![phar.to_string_lossy().to_string()] }
    } else { match shim::forward_for(&registry, tool, &version, name) {
        Ok(f) => f,
        Err(e) => return fail(&e),
    }};
    let mut command = std::process::Command::new(&fwd.program);
    command
        .args(&fwd.prefix_args)
        .args(args)
        .current_dir(&cwd)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit());
    if let Err(e) = configure_package_sources(&mut command, name, tool, args, &settings, &fwd) {
        return fail(&e);
    }
    let status = command.status();
    match status {
        Ok(s) => s
            .code()
            .unwrap_or(EXIT_NOT_FOUND as i32)
            .clamp(0, 255) as u8,
        Err(e) => fail(&Error::Io(format!(
            "启动 {} 失败：{e}",
            fwd.program.display()
        ))),
    }
}

fn find_composer_phar() -> Option<PathBuf> {
    let search = std::env::var_os("PATH")?;
    let managed_shims = paths::layout().ok().map(|layout| layout.shims);
    std::env::split_paths(&search)
        .filter(|dir| managed_shims.as_ref() != Some(dir))
        .map(|dir| dir.join("composer.phar"))
        .find(|file| file.is_file())
}

fn package_managers(name: &str, tool: ToolId, args: &[String]) -> Vec<&'static str> {
    if name == "composer" { return vec!["composer"]; }
    if matches!(name, "pip" | "pip3")
        || (matches!(tool, ToolId::Python | ToolId::PyPy)
            && args.windows(2).any(|p| p[0] == "-m" && p[1] == "pip"))
    { return vec!["pip"]; }
    match name {
        "npm" | "npx" | "bun" | "bunx" => vec!["npm"],
        "deno" => vec!["deno", "deno-jsr"],
        _ => vec![],
    }
}

fn configure_package_sources(
    command: &mut std::process::Command, name: &str, tool: ToolId, args: &[String],
    settings: &Settings, forward: &shim::Forward,
) -> Result<()> {
    let managers = package_managers(name, tool, args);
    if managers.is_empty() { return Ok(()); }
    let runtime = tokio::runtime::Builder::new_current_thread().enable_all().build()?;
    let urls = runtime.block_on(futures_util::future::join_all(managers.iter().map(|m|
        envswitch_lib::core::package_sources::resolve_url(m, settings)
    )));
    for (manager, url) in managers.iter().zip(urls) {
        let Some(url) = url else { continue; };
        match *manager {
            "pip" => { command.env("PIP_INDEX_URL", &url); }
            "npm" | "deno" => { command.env("NPM_CONFIG_REGISTRY", &url); }
            "deno-jsr" => { command.env("JSR_URL", &url); }
            "composer" => {
                // Ask Composer itself for its home; keep its auth, plugins and global packages.
                let output = std::process::Command::new(&forward.program)
                    .args(&forward.prefix_args).args(["config", "--global", "home", "--absolute"])
                    .current_dir(command.get_current_dir().unwrap_or(Path::new(".")))
                    .output()?;
                if !output.status.success() {
                    return Err(Error::Config(format!("读取 Composer 配置目录失败：{}", String::from_utf8_lossy(&output.stderr))));
                }
                let home = String::from_utf8_lossy(&output.stdout);
                let home = home.trim();
                if home.is_empty() { return Err(Error::Config("Composer 返回了空配置目录".into())); }
                configure_composer_home(Path::new(home), &url)?;
            }
            _ => {}
        }
    }
    Ok(())
}

fn configure_composer_home(home: &Path, packagist_url: &str) -> Result<()> {
    let path = home.join("config.json");
    let original = std::fs::read(&path).or_else(|e| if e.kind() == std::io::ErrorKind::NotFound { Ok(Vec::new()) } else { Err(e) })?;
    let mut config: serde_json::Value = if original.is_empty() { serde_json::json!({}) }
        else { serde_json::from_slice(&original).map_err(|e| Error::Config(format!("Composer 配置解析失败：{e}")))? };
    let repositories = config.as_object_mut().ok_or_else(|| Error::Config("Composer 配置必须是 JSON 对象".into()))?
        .entry("repositories").or_insert_with(|| serde_json::json!({}));
    let mirror = serde_json::json!({"type":"composer","url":packagist_url});
    match repositories {
        serde_json::Value::Object(repos) => {
            repos.remove("packagist");
            repos.insert("packagist.org".into(), mirror);
        }
        serde_json::Value::Array(repos) => {
            repos.retain(|repo| repo.get("packagist.org").is_none() && repo.get("packagist").is_none()
                && !repo.get("name").is_some_and(|n| n == "envswitch-packagist"));
            let mut mirror = mirror;
            mirror["name"] = "envswitch-packagist".into();
            repos.insert(0, mirror);
            repos.push(serde_json::json!({"packagist.org":false}));
        }
        _ => return Err(Error::Config("Composer repositories 配置必须是对象或数组".into())),
    }
    let updated = serde_json::to_vec_pretty(&config)?;
    if updated == original { return Ok(()); }
    std::fs::create_dir_all(home)?;
    let backup = home.join("config.json.envswitch-backup");
    if !original.is_empty() && !backup.exists() { std::fs::write(backup, &original)?; }
    envswitch_lib::core::util::write_json(&path, &config)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_injection_is_limited_to_package_commands() {
        assert!(package_managers("python", ToolId::Python, &["script.py".into()]).is_empty());
        assert_eq!(package_managers("python", ToolId::Python, &["-m".into(),"pip".into(),"install".into()]), vec!["pip"]);
        assert_eq!(package_managers("deno", ToolId::Deno, &[]), vec!["deno", "deno-jsr"]);
    }

    #[test]
    fn composer_changes_only_packagist_and_preserves_auth_and_custom_repositories() {
        let root = std::env::temp_dir().join(format!("envswitch-composer-test-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join("config.json");
        let auth = root.join("auth.json");
        let original = r#"{"config":{"allow-plugins":{"a/b":true}},"repositories":{"custom":{"type":"composer","url":"https://example.test"}}}"#;
        std::fs::write(&path, original).unwrap();
        std::fs::write(&auth, "unchanged").unwrap();
        configure_composer_home(&root, "https://mirror.example/").unwrap();
        let config: serde_json::Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        assert_eq!(config["repositories"]["custom"]["url"], "https://example.test");
        assert_eq!(config["repositories"]["packagist.org"]["url"], "https://mirror.example/");
        assert_eq!(config["config"]["allow-plugins"]["a/b"], true);
        assert_eq!(std::fs::read_to_string(&auth).unwrap(), "unchanged");
        assert_eq!(std::fs::read_to_string(root.join("config.json.envswitch-backup")).unwrap(), original);
        std::fs::write(&path, r#"{"repositories":[{"type":"vcs","url":"https://custom.test"}]}"#).unwrap();
        configure_composer_home(&root, "https://one.example/").unwrap();
        configure_composer_home(&root, "https://two.example/").unwrap();
        let config: serde_json::Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        assert_eq!(config["repositories"].as_array().unwrap().len(), 3);
        assert_eq!(config["repositories"][0]["url"], "https://two.example/");
        assert_eq!(config["repositories"][1]["url"], "https://custom.test");
        let _ = std::fs::remove_dir_all(root);
    }
}

const HELP: &str = "\
EnvSwitch 命令行（和图形界面共用同一份配置）

  envswitch-shim                 播报当前目录会用到哪个版本（终端脚本调它）
  envswitch-shim status          列出每个工具的当前版本与生效原因
  envswitch-shim use <工具> [版本]   在当前目录锁定版本，写 .tool-versions
  envswitch-shim unuse <工具>    取消当前目录里某个工具的锁定
  envswitch-shim global <工具> <版本>  设置全局默认版本
  envswitch-shim shims sync      重新生成转发文件
  envswitch-shim hooks [on|off]  查看或开关终端集成
";

fn run_cli(args: &[String]) -> u8 {
    let verb = args.first().map(String::as_str).unwrap_or("explain");
    let tail = &args[1..];
    let out: Result<u8> = match verb {
        "explain" => explain(&current_dir_or_default(), tail.first().map(Path::new)),
        "status" => status(),
        "use" => use_project(tail),
        "unuse" => unuse_project(tail),
        "global" => global(tail),
        "path-apply" => apply_path(tail),
        "shims" => sync_shims(tail),
        "hooks" => hooks(tail),
        "version" | "-v" | "--version" => {
            println!("envswitch-shim {}", envswitch_lib::core::VERSION);
            Ok(0)
        }
        "help" | "-h" | "--help" => {
            print!("{HELP}");
            Ok(0)
        }
        other => Err(Error::NotFound(format!(
            "没有 {other} 这个命令，试试 envswitch-shim help"
        ))),
    };
    match out {
        Ok(code) => code,
        Err(e) => fail(&e),
    }
}

fn current_dir() -> Result<PathBuf> {
    std::env::current_dir().map_err(|e| Error::Io(format!("读不到当前目录：{e}")))
}

fn current_dir_or_default() -> PathBuf {
    std::env::current_dir().unwrap_or_default()
}

/// 只在版本真由项目声明决定时才出声，否则每次 cd 都刷屏。
fn explain(cwd: &Path, override_dir: Option<&Path>) -> Result<u8> {
    let dir = override_dir.map(PathBuf::from).unwrap_or_else(|| cwd.to_path_buf());
    let settings = Settings::load()?;
    let registry = Registry::load()?;
    if settings.lock_global && toolversions::find_declaration_dir(&dir).is_some() {
        println!("EnvSwitch：全局锁定已开启，本目录的版本声明暂不生效（设置里可以关掉）");
        return Ok(0);
    }
    for tool in ToolId::ALL {
        let res = toolversions::resolve_with(&settings, &registry, tool, &dir);
        if res.source != ResolutionSource::ProjectFile {
            continue;
        }
        println!(
            "EnvSwitch  {} {}  来自 {}",
            tool.display(),
            res.version.clone().unwrap_or_default(),
            res.found_in
                .as_ref()
                .map(|d| d.display().to_string())
                .unwrap_or_default()
        );
    }
    Ok(0)
}

fn status() -> Result<u8> {
    let settings = Settings::load()?;
    let layout = paths::layout()?;
    let registry = Registry::load()?;
    println!("数据目录：{}", layout.root.display());
    println!("转发目录：{}", layout.shims.display());
    // 名次分两头说：当前终端这份 PATH 才是这条命令真正会撞上的顺序，
    // 注册表里那份决定没装终端集成的新窗口认不认得。只报设置里的开关会骗人。
    let session = std::env::var_os("PATH")
        .map(|r| r.to_string_lossy().to_string())
        .unwrap_or_default();
    match pathmod::rank_in(&session, &layout.shims) {
        Some(1) => println!("  当前终端：排第 1，命令会先撞上它"),
        Some(n) => println!("  当前终端：排第 {n}，前面还有别的版本可能抢先"),
        None => println!("  当前终端：没在里面（终端集成没开或没生效）"),
    }
    for (label, scope) in [
        ("整台电脑", envswitch_lib::core::model::PathScope::System),
        ("当前用户", envswitch_lib::core::model::PathScope::User),
    ] {
        let raw = pathmod::read_raw(scope).unwrap_or_default();
        match pathmod::rank_in(&raw, &layout.shims) {
            Some(n) => println!("  注册表·{label}：第 {n} 位"),
            None => println!("  注册表·{label}：没写进去"),
        }
    }
    println!(
        "自动切换：{}　全局锁定：{}",
        yes_no(settings.auto_switch),
        yes_no(settings.lock_global)
    );
    let cwd = current_dir_or_default();
    for tool in ToolId::ALL {
        let res = toolversions::resolve_with(&settings, &registry, tool, &cwd);
        println!(
            "{}：{}（{}）",
            tool.display(),
            res.version.clone().unwrap_or_else(|| "没有可用版本".to_string()),
            source_label(&res)
        );
        if let Some(exec) = &res.exec_path {
            println!("  可执行文件：{}", exec.display());
        }
        if let Some(note) = &res.note {
            println!("  提示：{note}");
        }
        let installed = registry.list(tool);
        if !installed.is_empty() {
            let names: Vec<String> = installed
                .iter()
                .map(|r| {
                    if registry.default_of(tool).as_deref() == Some(r.version.as_str()) {
                        format!("{}（默认）", r.version)
                    } else {
                        r.version.clone()
                    }
                })
                .collect();
            println!("  已装：{}", names.join("、"));
        }
    }
    Ok(0)
}

fn source_label(res: &Resolution) -> String {
    match res.source {
        ResolutionSource::ProjectFile => "本目录的版本声明".to_string(),
        ResolutionSource::GlobalDefault => "全局默认".to_string(),
        ResolutionSource::OnlyInstalled => "只装了一个版本，直接用".to_string(),
        ResolutionSource::None => "未接管，走系统里的".to_string(),
    }
}

fn yes_no(v: bool) -> &'static str {
    if v { "已开" } else { "已关" }
}

fn use_project(tail: &[String]) -> Result<u8> {
    let (tool, version) = parse_tool_and_optional(tail)?;
    let cwd = current_dir()?;
    let registry = Registry::load()?;
    let version = match version {
        Some(v) => v,
        None => toolversions::resolve_with(&Settings::load()?, &registry, tool, &cwd)
            .version
            .ok_or_else(|| {
                Error::Detail(format!(
                    "请写明版本号，{} 现在没有能自动选出的版本",
                    tool.display()
                ))
            })?,
    };
    if registry.find(tool, &version).is_none() {
        return Err(Error::NotInstalled(tool.display().to_string(), version));
    }
    let lock = toolversions::lock(&cwd, tool, &version)?;
    println!(
        "已在 {} 里把 {} 定为 {}",
        lock.file.display(),
        tool.display(),
        version
    );
    Ok(0)
}

fn unuse_project(tail: &[String]) -> Result<u8> {
    let (tool, _) = parse_tool_and_optional(tail)?;
    let cwd = current_dir()?;
    match toolversions::unlock(&cwd, tool)? {
        Some(lock) => println!(
            "已取消 {} 里关于 {} 的锁定",
            lock.dir.display(),
            tool.display()
        ),
        None => println!("{} 附近没有需要改的版本声明", cwd.display()),
    }
    Ok(0)
}

fn global(tail: &[String]) -> Result<u8> {
    let (tool, version) = parse_tool_and_optional(tail)?;
    let version = version.ok_or_else(|| Error::Detail("请写明要设为默认的版本号".to_string()))?;
    let note = install::switch_default(tool, &version)?;
    println!("{} 的全局默认版本已设为 {version}", tool.display());
    if let Some(n) = note {
        println!("{n}");
    }
    Ok(0)
}

/// 提权分支：界面里选了"整台电脑"后，系统 PATH 由这个控制台程序代写。
fn apply_path(tail: &[String]) -> Result<u8> {
    let raw = tail
        .first()
        .ok_or_else(|| Error::Detail("path-apply 需要一个参数文件".to_string()))?;
    pathmod::apply_from_file(Path::new(raw))?;
    Ok(0)
}

fn sync_shims(tail: &[String]) -> Result<u8> {
    if tail.first().map(String::as_str) != Some("sync") {
        return Err(Error::Detail("shims 后面要跟 sync".to_string()));
    }
    let files = shim::sync_all()?;
    println!("转发文件已备齐：{} 个", files.len());
    Ok(0)
}

fn hooks(tail: &[String]) -> Result<u8> {
    match tail.first().map(String::as_str) {
        Some("on") => {
            for h in hook::install_all()? {
                println!(
                    "已写入 {}：{}",
                    h.label,
                    h.rc_file.map(|p| p.display().to_string()).unwrap_or_default()
                );
            }
        }
        Some("off") => {
            let removed = hook::remove_all()?;
            println!("已取消 {} 个终端的集成", removed.len());
        }
        _ => {
            for h in hook::status()? {
                println!(
                    "{}：{}{}",
                    h.label,
                    if h.installed { "已开启" } else { "未开启" },
                    h.note.map(|n| format!("（{n}）")).unwrap_or_default()
                );
            }
        }
    }
    Ok(0)
}

fn parse_tool_and_optional(tail: &[String]) -> Result<(ToolId, Option<String>)> {
    let raw = tail
        .first()
        .ok_or_else(|| Error::Detail(format!("要指明工具，可选：{}", tool_names())))?;
    let tool = ToolId::from_key(raw)
        .ok_or_else(|| Error::NotFound(format!("不认识 {raw}，可选：{}", tool_names())))?;
    Ok((tool, tail.get(1).cloned()))
}

fn tool_names() -> String {
    ToolId::ALL
        .iter()
        .map(|t| t.as_str())
        .collect::<Vec<_>>()
        .join("、")
}

fn fail(e: &Error) -> u8 {
    eprintln!("EnvSwitch：{e}");
    if let Some(d) = e.detail() {
        eprintln!("  {d}");
    }
    match e {
        Error::Cancelled => 130,
        Error::NotInstalled(..) | Error::NotFound(_) => EXIT_NOT_FOUND,
        _ => 1,
    }
}
