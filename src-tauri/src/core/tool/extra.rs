//! PHP/MySQL 官方 Windows 二进制，以及 Bun/Deno 官方 GitHub Releases。
use std::{collections::{HashMap, HashSet}, path::PathBuf};
use futures_util::{stream, StreamExt};
use serde_json::Value;
use super::{apply_wrap, line_key, ArchiveKind, Artifact, ShimSpec, ShimTarget, SourceInfo};
use crate::core::{error::{Error, Result}, model::{AvailableVersion, ToolId}, records::Registry, util};

#[derive(Debug, Clone, Copy)]
pub struct ExtraPlugin(pub ToolId);

fn field<'a>(v: &'a Value, key: &str) -> &'a str { v[key].as_str().unwrap_or_default() }
fn stable(raw: &str) -> bool {
    let parts: Vec<_> = raw.split('.').collect();
    parts.len() == 3 && parts.iter().all(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_digit()))
}
// Windows PHP 会对同一个上游版本重打包，例如 8.4.27-1；这也是正式版本。
fn php_stable(raw: &str) -> bool {
    let (base, revision) = raw.split_once('-').map_or((raw, None), |(base, revision)| (base, Some(revision)));
    stable(base) && revision.is_none_or(|r| !r.is_empty() && r.chars().all(|c| c.is_ascii_digit()))
}
async fn json(client: &reqwest::Client, url: &str) -> Result<Value> {
    Ok(client.get(url).header("Accept", "application/vnd.github+json")
        .timeout(std::time::Duration::from_secs(30))
        .send().await?.error_for_status()?.json().await?)
}
fn row(tool: ToolId, version: &str) -> AvailableVersion {
    AvailableVersion { tool, version: version.into(), line_key: line_key(tool, version), ..Default::default() }
}

impl ExtraPlugin {
    fn repo(&self) -> &'static str {
        match self.0 { ToolId::Bun => "oven-sh/bun", ToolId::Deno => "denoland/deno", _ => "mysql/mysql-server" }
    }
    pub fn sources(&self) -> Vec<SourceInfo> {
        if self.0 == ToolId::Deno {
            return vec![
                SourceInfo { id: "official".into(), label: "Deno 官方".into(), region: "海外", base: "https://github.com/denoland/deno".into(), wrap: None, note: None, probe_url: "https://api.github.com/repos/denoland/deno/releases?per_page=1".into() },
                SourceInfo { id: "deno-cn".into(), label: "Deno 国内下载镜像".into(), region: "国内", base: "https://dl.deno.js.cn".into(), wrap: None, note: Some("官方发行文件镜像；下载文件名和版本仍与上游一致".into()), probe_url: format!("https://dl.deno.js.cn/release/v2.5.6/{}", asset_name(ToolId::Deno)) },
            ];
        }
        let (base, probe) = match self.0 {
            ToolId::Php => ("https://windows.php.net/downloads/releases".to_string(), "https://windows.php.net/downloads/releases/releases.json".to_string()),
            ToolId::MySql => ("https://cdn.mysql.com".into(), "https://api.github.com/repos/mysql/mysql-server/git/matching-refs/tags/mysql-".into()),
            _ => (format!("https://github.com/{}", self.repo()), format!("https://api.github.com/repos/{}/releases?per_page=100", self.repo())),
        };
        vec![SourceInfo { id: "official".into(), label: format!("{} 官方", self.0.display()), region: "海外", base, wrap: None, note: None, probe_url: probe }]
    }
    pub fn exec_rel(&self) -> PathBuf {
        let name = match self.0 { ToolId::Php => "php", ToolId::MySql => "bin/mysql", ToolId::Bun => "bun", ToolId::Deno => "deno", _ => unreachable!() };
        PathBuf::from(format!("{name}{}", if cfg!(windows) { ".exe" } else { "" }))
    }
    pub fn shims(&self) -> Vec<ShimSpec> {
        let mut out = vec![ShimSpec { name: self.0.as_str().into(), target: ShimTarget::Bin(self.exec_rel()) }];
        if self.0 == ToolId::Bun { out.push(ShimSpec { name: "bunx".into(), target: ShimTarget::Args(vec!["x".into()]) }); }
        if self.0 == ToolId::Php {
            out.push(ShimSpec { name: "php-cgi".into(), target: ShimTarget::Bin(PathBuf::from(if cfg!(windows) { "php-cgi.exe" } else { "php-cgi" })) });
            out.push(ShimSpec { name: "composer".into(), target: ShimTarget::Args(Vec::new()) });
        }
        if self.0 == ToolId::MySql {
            for name in ["mysqld", "mysqldump", "mysqladmin", "mysqlcheck", "mysqlshow", "mysqlbinlog"] {
                out.push(ShimSpec { name: name.into(), target: ShimTarget::Bin(PathBuf::from(format!("bin/{name}{}", if cfg!(windows) { ".exe" } else { "" }))) });
            }
        }
        out
    }
    pub async fn list_versions(&self, client: &reqwest::Client, src: &SourceInfo) -> Result<Vec<AvailableVersion>> {
        if matches!(self.0, ToolId::Php | ToolId::MySql) && !cfg!(all(windows, target_arch="x86_64")) {
            return Err(Error::Unsupported(format!("{} 的官方预编译包目前仅支持 Windows x64", self.0.display())));
        }
        let mut out = match self.0 {
            ToolId::Php => self.php(client, src).await?,
            ToolId::MySql => self.mysql(client, src).await?,
            _ => parse_github(self.0, &json(client, &self.sources()[0].probe_url).await?, &asset_name(self.0)),
        };
        let installed: HashSet<String> = Registry::load().map(|r| r.list(self.0).into_iter().map(|v| v.version).collect()).unwrap_or_default();
        out.sort_by(|a,b| util::version_desc(&a.version, &b.version));
        let mut counts = HashMap::new();
        for v in &mut out {
            v.installed = installed.contains(&v.version);
            let n = counts.entry(v.line_key.clone()).or_insert(0);
            v.is_line_latest = *n == 0;
            *n += 1;
        }
        for v in &mut out { v.line_count = counts[&v.line_key]; }
        Ok(out)
    }
    pub fn artifact(&self, src: &SourceInfo, v: &AvailableVersion) -> Result<Artifact> {
        let url = v.download_url.as_ref().ok_or_else(|| Error::Unsupported(v.unavailable_reason.clone().unwrap_or_else(|| "没有当前平台的安装包".into())))?;
        let url = if self.0 == ToolId::Deno && src.id == "deno-cn" {
            let version = v.version.strip_prefix('v').unwrap_or(&v.version);
            let filename = asset_name(ToolId::Deno);
            format!("https://dl.deno.js.cn/release/v{version}/{filename}")
        } else { apply_wrap(src, url) };
        Ok(Artifact { url, file_name: v.file_name.clone().ok_or_else(|| Error::Detail("缺少安装包文件名".into()))?, sha256: v.sha256.clone(), size_hint: v.size_hint, kind: ArchiveKind::Zip })
    }
    async fn php(&self, client: &reqwest::Client, src: &SourceInfo) -> Result<Vec<AvailableVersion>> {
        let data = json(client, &self.sources()[0].probe_url).await?;
        let entries = parse_php(&data);
        Ok(stream::iter(entries).map(|mut v| async move {
            let file = v.file_name.as_deref().unwrap_or_default();
            let root = "https://windows.php.net/downloads/releases";
            let candidates = [format!("{root}/{file}"), format!("{root}/archives/{file}")];
            resolve_package(client, src, &mut v, &candidates).await;
            v
        }).buffer_unordered(6).collect().await)
    }
    async fn mysql(&self, client: &reqwest::Client, src: &SourceInfo) -> Result<Vec<AvailableVersion>> {
        let data = json(client, &self.sources()[0].probe_url).await?;
        let mut versions: Vec<String> = data.as_array().into_iter().flatten()
            .filter_map(|v| field(v, "ref").strip_prefix("refs/tags/mysql-"))
            .filter(|v| stable(v) && util::version_cmp(v, "8.0.0") != std::cmp::Ordering::Less)
            .map(str::to_string).collect();
        versions.sort_by(|a,b| util::version_desc(a,b));
        // 官方 Windows 发行线；每条线最近三个版本。旧版本也可离线导入。
        let mut counts = HashMap::new();
        versions.retain(|v| { let n = counts.entry(line_key(self.0,v)).or_insert(0); *n += 1; *n <= 3 });
        Ok(stream::iter(versions).map(|version| async move {
            let mut v = row(self.0, &version);
            let file = format!("mysql-{version}-winx64.zip");
            v.file_name = Some(file.clone());
            v.is_lts = matches!(v.line_key.as_str(), "8.0" | "8.4" | "9.7");
            let candidates = [format!("https://cdn.mysql.com/Downloads/MySQL-{}/{file}", v.line_key)];
            resolve_package(client, src, &mut v, &candidates).await;
            v
        }).buffer_unordered(6).collect().await)
    }
}

async fn resolve_package(client: &reqwest::Client, src: &SourceInfo, v: &mut AvailableVersion, urls: &[String]) {
    for url in urls {
        if let Ok(resp) = client.head(apply_wrap(src,url)).timeout(std::time::Duration::from_secs(12)).send().await {
            if resp.status().is_success() {
                v.download_url = Some(url.clone());
                v.size_hint = resp.content_length().filter(|size| *size > 0);
                return;
            }
        }
    }
    v.unavailable_reason = Some("暂时无法访问该版本的官方安装包，请检查网络或更换下载中转地址后刷新".into());
}

fn parse_php(data: &Value) -> Vec<AvailableVersion> {
    data.as_object().into_iter().flat_map(|m| m.values()).filter_map(|release| {
        let version = field(release,"version");
        if !php_stable(version) { return None; }
        let builds = release.as_object()?;
        let build = builds.iter().filter(|(key, v)| key.ends_with("-x64") && v["zip"]["path"].is_string())
            .min_by_key(|(key,_)| !key.starts_with("nts-"))?.1;
        let mut v = row(ToolId::Php,version);
        v.file_name = Some(field(&build["zip"],"path").into());
        v.sha256 = build["zip"]["sha256"].as_str().map(str::to_string);
        v.released = build["mtime"].as_str().map(str::to_string);
        Some(v)
    }).collect()
}

pub fn asset_name(tool: ToolId) -> String {
    let arch = if cfg!(target_arch="aarch64") { "aarch64" } else { "x64" };
    if tool == ToolId::Bun {
        let os = if cfg!(windows) { "windows" } else if cfg!(target_os="macos") { "darwin" } else { "linux" };
        let baseline = if arch == "x64" && os != "darwin" { "-baseline" } else { "" };
        format!("bun-{os}-{arch}{baseline}.zip")
    } else {
        let arch = if arch == "x64" { "x86_64" } else { arch };
        let os = if cfg!(windows) { "pc-windows-msvc" } else if cfg!(target_os="macos") { "apple-darwin" } else { "unknown-linux-gnu" };
        format!("deno-{arch}-{os}.zip")
    }
}
fn parse_github(tool: ToolId, data: &Value, asset: &str) -> Vec<AvailableVersion> {
    data.as_array().into_iter().flatten().filter_map(|r| {
        if r["prerelease"].as_bool().unwrap_or(false) || r["draft"].as_bool().unwrap_or(false) { return None; }
        let raw = field(r,"tag_name");
        let version = raw.strip_prefix("bun-v").or_else(|| raw.strip_prefix('v'))?;
        if !stable(version) { return None; }
        let mut v = row(tool,version);
        v.released = r["published_at"].as_str().map(str::to_string);
        v.origin_tag = Some(raw.into());
        if let Some(a) = r["assets"].as_array().into_iter().flatten().find(|a| field(a,"name")==asset) {
            v.download_url = Some(field(a,"browser_download_url").into());
            // GitHub 的 Bun/Deno 原文件名没有版本号；缓存及离线导入必须保留版本。
            v.file_name = Some(format!("{version}-{asset}"));
            v.size_hint = a["size"].as_u64();
            v.sha256 = field(a,"digest").strip_prefix("sha256:").filter(|h| h.len()==64 && h.chars().all(|c| c.is_ascii_hexdigit())).map(str::to_string);
        } else { v.unavailable_reason = Some("官方没有为此版本提供当前平台的安装包".into()); }
        Some(v)
    }).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    /// 单独进程运行，ENVSWITCH_HOME 必须指向专用验证目录；不写 PATH、开机启动或数据库服务。
    #[tokio::test]
    #[ignore = "需要联网下载安装四种环境，且需设置专用 ENVSWITCH_HOME"]
    async fn live_install_and_repair_forwarders() {
        use crate::core::{install, job::{self, JobKind}, mirror, paths, settings::Settings, shim, tool::Plugin};
        let root = PathBuf::from(std::env::var(paths::HOME_ENV).expect("必须指定隔离数据目录"));
        assert_eq!(root.file_name().unwrap(), "envswitch-e2e");
        let layout = paths::layout().unwrap();
        assert_eq!(layout.root,root);
        layout.create_all().unwrap();
        let mut settings = Settings::default();
        settings.autostart = false;
        settings.keep_archives = true;
        settings.save().unwrap();
        let client = mirror::build_client(&settings).unwrap();
        shim::repair().unwrap();
        let only = std::env::var("ENVSWITCH_E2E_TOOL").ok();
        for tool in [ToolId::Php, ToolId::Bun, ToolId::Deno, ToolId::MySql] {
            if only.as_deref().is_some_and(|name| name != tool.as_str()) { continue; }
            let plugin = Plugin::for_tool(tool).unwrap();
            let source = plugin.sources().remove(0);
            let catalog = tokio::time::timeout(std::time::Duration::from_secs(100), mirror::versions(&client,&plugin,&source,true)).await.expect("官方版本列表超时").unwrap();
            let version = &catalog.versions.iter().find(|v| v.download_url.is_some()).expect("官方列表缺少安装包").version;
            eprintln!("Installing {} {version}",tool.display());
            if Registry::load().unwrap().find(tool,version).is_none() {
                let job = job::register(tool,version,JobKind::Install);
                let last = std::sync::Mutex::new(std::time::Instant::now());
                let events: crate::core::download::OnEvent = std::sync::Arc::new(move |event| {
                    let mut at = last.lock().unwrap();
                    if event.phase != crate::core::model::JobPhase::Downloading || at.elapsed().as_secs() >= 10 {
                        eprintln!("{:?}: {:.1}% {:?}",event.phase,event.percent.unwrap_or_default(),event.message);
                        *at = std::time::Instant::now();
                    }
                });
                tokio::time::timeout(std::time::Duration::from_secs(240), install::install(&client,job,Some(events),tool,version,Some("official"),false)).await.expect("官方安装超时").unwrap();
            }
            let output = std::process::Command::new(shim::shim_path(&layout.shims,tool.as_str()))
                .arg("--version").current_dir(&layout.root).output().unwrap();
            let stdout = String::from_utf8_lossy(&output.stdout);
            assert!(output.status.success(),"{} failed: {stdout} {}",tool.display(),String::from_utf8_lossy(&output.stderr));
            assert!(stdout.contains(version.split('-').next().unwrap()),"{} wrong version: {stdout}",tool.display());
            eprintln!("{} forwarded: {}",tool.display(),stdout.lines().next().unwrap_or_default());
            if tool == ToolId::Bun {
                let fwd = shim::forward_for(&Registry::load().unwrap(),tool,version,"bunx").unwrap();
                assert_eq!(fwd.prefix_args,vec!["x"]);
                assert!(std::process::Command::new(shim::shim_path(&layout.shims,"bunx")).arg("--help").current_dir(&layout.root).status().unwrap().success());
            }
        }
        assert!(shim::missing().unwrap().is_empty());
        let deleted = shim::shim_path(&layout.shims,"php");
        std::fs::remove_file(&deleted).unwrap();
        assert!(shim::missing().unwrap().contains(&deleted.display().to_string()));
        shim::repair().unwrap();
        assert!(shim::missing().unwrap().is_empty());
        eprintln!("Deleted forwarder detected and repaired successfully");
    }
    #[test]
    fn github_selects_exact_asset_and_ignores_prereleases() {
        let data = serde_json::json!([
            {"tag_name":"bun-v1.2.3","prerelease":false,"assets":[
                {"name":"bun-windows-x64.zip","browser_download_url":"wrong"},
                {"name":"bun-windows-x64-baseline.zip","browser_download_url":"https://example.test/bun.zip","size":99,"digest":format!("sha256:{}","a".repeat(64))}]},
            {"tag_name":"bun-v1.3.0","prerelease":true,"assets":[]},
            {"tag_name":"bun-v1.2.2","assets":[]}
        ]);
        let rows = parse_github(ToolId::Bun,&data,"bun-windows-x64-baseline.zip");
        assert_eq!(rows.len(),2);
        assert_eq!(rows[0].download_url.as_deref(),Some("https://example.test/bun.zip"));
        assert!(rows[0].file_name.as_ref().unwrap().contains("1.2.3"));
        assert_eq!(rows[0].sha256.as_ref().unwrap().len(),64);
        assert!(rows[1].unavailable_reason.is_some());
    }
    /// 显式运行的联网检查，不作为离线单元测试的依赖。
    #[tokio::test]
    #[ignore = "需要访问国内运行时镜像"]
    async fn live_domestic_runtime_mirrors() {
        let client = crate::core::mirror::build_client(&crate::core::settings::Settings::default()).unwrap();
        let deno = ExtraPlugin(ToolId::Deno);
        let source = deno.sources().into_iter().find(|s| s.id == "deno-cn").unwrap();
        let response = client.head(&source.probe_url).timeout(std::time::Duration::from_secs(20)).send().await.unwrap();
        assert!(response.status().is_success());
        assert!(response.content_length().is_some_and(|n| n > 1_000_000));
        eprintln!("Deno: {} => {} bytes", source.probe_url, response.content_length().unwrap());
    }

    /// 显式运行的联网检查，不作为离线单元测试的依赖。
    #[tokio::test]
    #[ignore = "需要访问官方发布源"]
    async fn live_official_catalogs_have_downloadable_packages() {
        let client = crate::core::mirror::build_client(&crate::core::settings::Settings::default()).unwrap();
        for tool in [ToolId::Php, ToolId::MySql, ToolId::Bun, ToolId::Deno] {
            let p = ExtraPlugin(tool);
            let source = p.sources().remove(0);
            let rows = tokio::time::timeout(std::time::Duration::from_secs(100), p.list_versions(&client,&source)).await.unwrap().unwrap();
            let available: Vec<_> = rows.iter().filter(|v|v.download_url.is_some()).collect();
            assert!(!available.is_empty(), "{} 没有可下载的版本：{:?}",tool.display(),rows);
            let first = available[0];
            let artifact = p.artifact(&source,first).unwrap();
            let response = client.head(&artifact.url).timeout(std::time::Duration::from_secs(20)).send().await.unwrap();
            assert!(response.status().is_success(), "{} {}",artifact.url,response.status());
            eprintln!("{}: {} entries / {} available, latest {} => {}",tool.display(),rows.len(),available.len(),first.version,artifact.url);
        }
    }

    #[test]
    fn php_uses_x64_nts_binary_not_sources_or_x86() {
        let data = serde_json::json!({"8.4":{"version":"8.4.1-1",
            "source":{"path":"source.zip"},"ts-vs17-x64":{"zip":{"path":"ts.zip"}},
            "nts-vs17-x64":{"zip":{"path":"nts.zip","sha256":"hash"}},
            "nts-vs17-x86":{"zip":{"path":"x86.zip"}}}});
        let rows = parse_php(&data);
        assert_eq!(rows[0].version,"8.4.1-1");
        assert_eq!(rows[0].file_name.as_deref(),Some("nts.zip"));
        assert_eq!(rows[0].sha256.as_deref(),Some("hash"));
        assert!(!php_stable("8.5.0RC1"));
        assert!(!php_stable("8.5.0-rc1"));
    }
}
