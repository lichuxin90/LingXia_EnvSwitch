use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use super::error::{Error, Result};
use super::model::{AvailableVersion, MirrorProbe, ToolId};
use super::paths;
use super::records::Registry;
use super::settings::Settings;
use super::tool::{Plugin, SourceInfo};
use super::util;

const PROBE_TIMEOUT: Duration = Duration::from_secs(6);
const AUTO_TTL: Duration = Duration::from_secs(6 * 3600);
const LIST_TTL: Duration = Duration::from_secs(6 * 3600);

pub fn build_client(settings: &Settings) -> Result<reqwest::Client> {
    let b = reqwest::Client::builder()
        .user_agent(concat!("EnvSwitch/", env!("CARGO_PKG_VERSION")))
        .connect_timeout(Duration::from_secs(10))
        .tcp_nodelay(true);
    with_proxy(b, settings)?
        .build()
        .map_err(|e| Error::Detail(format!("网络组件起不来：{e}")))
}

/// 手动代理对测速同样有效：不套代理去测国内源，结果全是"连不上"。
fn with_proxy(
    b: reqwest::ClientBuilder,
    settings: &Settings,
) -> Result<reqwest::ClientBuilder> {
    Ok(match settings.proxy_mode {
        super::settings::ProxyMode::System => b,
        super::settings::ProxyMode::Direct => b.no_proxy(),
        super::settings::ProxyMode::Custom => {
            let url = settings.proxy_url.clone().unwrap_or_default();
            if url.trim().is_empty() {
                return Err(Error::Detail("选了手动代理但地址是空的".to_string()));
            }
            b.proxy(
                reqwest::Proxy::all(url.trim())
                    .map_err(|e| Error::Config(format!("代理地址填得不对：{e}")))?,
            )
        }
    })
}

/// 只用来测速的客户端：短超时，不排队等大包。
pub fn probe_client(settings: &Settings) -> Result<reqwest::Client> {
    let b = reqwest::Client::builder()
        .user_agent(concat!("EnvSwitch/", env!("CARGO_PKG_VERSION")))
        .connect_timeout(Duration::from_secs(4))
        .timeout(PROBE_TIMEOUT);
    Ok(with_proxy(b, settings)?
        .build()
        .map_err(|e| Error::Detail(format!("测速组件起不来：{e}")))?)
}

/// Use the operating-system country setting, never the UI language or an IP lookup.
pub fn system_region() -> String {
    #[cfg(windows)]
    {
        use winreg::{RegKey, enums::HKEY_CURRENT_USER};
        if let Ok(geo) = RegKey::predef(HKEY_CURRENT_USER).open_subkey(r"Control Panel\International\Geo") {
            if let Ok(name) = geo.get_value::<String, _>("Name") {
                if !name.trim().is_empty() { return name.trim().to_ascii_uppercase(); }
            }
            if geo.get_value::<String, _>("Nation").ok().as_deref() == Some("45") {
                return "CN".into();
            }
        }
    }
    for key in ["LC_ALL", "LC_MESSAGES", "LANG"] {
        if let Ok(locale) = std::env::var(key) {
            if let Some(country) = locale.split(['_', '-']).nth(1) {
                return country.split(['.', '@']).next().unwrap_or("").to_ascii_uppercase();
            }
        }
    }
    "GLOBAL".into()
}

fn region_priority(country: &str, source_region: &str) -> u8 {
    if country == "CN" {
        match source_region { "国内" => 0, "第三方" => 1, _ => 2 }
    } else if source_region == "海外" { 0 } else { 1 }
}

fn best_probe<'a>(probes: &'a [MirrorProbe], country: &str) -> Option<&'a MirrorProbe> {
    probes.iter().filter(|p| p.ok)
        .min_by_key(|p| (region_priority(country, p.region), p.latency_ms.unwrap_or(u64::MAX)))
}

fn auto_key(tool: ToolId) -> String { format!("{}@{}:region-v1", tool.as_str(), system_region()) }

static AUTO: Mutex<Option<(Instant, Vec<(String, String)>)>> = Mutex::new(None);

fn auto_cache_path() -> Result<PathBuf> {
    Ok(paths::layout()?.config_path("source-choice.json"))
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct StoredChoice {
    pub chosen: Vec<(String, String)>,
    pub at: String,
}

/// 把设置里的源 id 变成真正可用的源，"auto" 走测速。
pub async fn resolve_source(
    client: &reqwest::Client,
    plugin: &Plugin,
    settings: &Settings,
    override_id: Option<&str>,
) -> Result<SourceInfo> {
    let tool_key = plugin.id().as_str();
    let id = override_id
        .map(|s| s.to_string())
        .unwrap_or_else(|| settings.source_for(tool_key));

    if id == "custom" {
        return custom_source(plugin, settings);
    }
    if id != "auto" {
        return plugin.source(&id);
    }

    if let Some(fresh) = cached_auto(plugin.id()) {
        if plugin.sources().iter().any(|s| s.id == fresh) {
            return plugin.source(&fresh);
        }
    }

    let probes = probe_sources(client, plugin).await;
    let best = best_probe(&probes, &system_region())
        .map(|p| p.id.clone())
        .ok_or_else(|| {
            Error::Network(
                "所有下载源都没连上，检查一下网络或者代理设置".to_string(),
            )
        })?;
    remember_auto(plugin.id(), &best);
    plugin.source(&best)
}

fn custom_source(plugin: &Plugin, settings: &Settings) -> Result<SourceInfo> {
    let raw = settings
        .custom_base
        .get(plugin.id().as_str())
        .cloned()
        .unwrap_or_default()
        .trim()
        .trim_end_matches('/')
        .to_string();
    if raw.is_empty() {
        return Err(Error::Detail("选了自定义源，但地址还没填".to_string()));
    }
    let mut base = plugin.sources().first().cloned().unwrap_or(SourceInfo {
        id: "custom".into(),
        label: "自定义".into(),
        region: "自定义",
        base: raw.clone(),
        wrap: None,
        note: None,
        probe_url: String::new(),
    });
    match plugin.id() {
        // Node 的镜像就是整套 dist 目录，直接换根地址。
        ToolId::Node => {
            base.base = raw.clone();
            base.wrap = None;
            base.probe_url = format!("{raw}/index.json");
        }
        // Python 的清单必须走 GitHub API，自定义地址只能当下载中转前缀。
        ToolId::Python | ToolId::Php | ToolId::MySql | ToolId::Bun | ToolId::Deno => {
            base.wrap = Some(format!("{raw}/{{url}}"));
        }
        // Go 和 PyPy 的清单就在下载目录本身，换根地址等于整套换掉。
        ToolId::Go => {
            base.base = raw.clone();
            base.wrap = None;
            base.probe_url = format!("{raw}/?mode=json");
        }
        ToolId::PyPy => {
            base.base = raw.clone();
            base.wrap = None;
            base.probe_url = format!("{raw}/");
        }
    }
    base.id = "custom".into();
    base.label = format!("自定义（{raw}）");
    base.region = "自定义";
    base.note = Some("你手动填的地址，出了问题是先核对这个地址".into());
    Ok(base)
}

fn cached_auto(tool: ToolId) -> Option<String> {
    if let Ok(guard) = AUTO.lock()
        && let Some((at, list)) = guard.as_ref()
        && at.elapsed() < AUTO_TTL
        && let Some((_, id)) = list.iter().find(|(t, _)| t == &auto_key(tool))
    {
        return Some(id.clone());
    }
    // 内存里没有就看磁盘，重启后六小时内不必再测一遍。
    let file = auto_cache_path().ok()?;
    let stored: StoredChoice = util::read_json_opt(&file).ok()?.unwrap_or_default();
    let at = chrono::DateTime::parse_from_rfc3339(&stored.at).ok()?;
    let age = chrono::Local::now().signed_duration_since(at.with_timezone(&chrono::Local));
    if age.num_seconds() >= 0 && age.num_seconds() < AUTO_TTL.as_secs() as i64 {
        stored
            .chosen
            .iter()
            .find(|(t, _)| t == &auto_key(tool))
            .map(|(_, id)| id.clone())
    } else {
        None
    }
}

fn remember_auto(tool: ToolId, id: &str) {
    let mut list: Vec<(String, String)> = Vec::new();
    if let Ok(guard) = AUTO.lock()
        && let Some((_, cached)) = guard.as_ref()
    {
        list = cached.clone();
    }
    list.retain(|(t, _)| t != &auto_key(tool));
    list.push((auto_key(tool), id.to_string()));
    if let Ok(mut guard) = AUTO.lock() {
        *guard = Some((Instant::now(), list.clone()));
    }
    if let Ok(file) = auto_cache_path() {
        let _ = util::write_json(
            &file,
            &StoredChoice {
                chosen: list,
                at: chrono::Local::now().to_rfc3339(),
            },
        );
    }
}

/// Probe sources concurrently with a strict timeout. HTTP failures must not win selection.
pub async fn probe_sources(client: &reqwest::Client, plugin: &Plugin) -> Vec<MirrorProbe> {
    futures_util::future::join_all(plugin.sources().into_iter().map(|src| async move {
        let start = Instant::now();
        let result = client.get(&src.probe_url)
            .header(reqwest::header::RANGE, "bytes=0-1023")
            .timeout(PROBE_TIMEOUT).send().await;
        let (ok, latency_ms, error) = match result {
            Ok(response) => {
                let status = response.status();
                (status.is_success(), Some(start.elapsed().as_millis() as u64),
                    if status.is_success() { None } else { Some(format!("远端回了 {}", status.as_u16())) })
            }
            Err(e) => (false, None, Some(e.to_string())),
        };
        MirrorProbe { id: src.id, label: src.label, url: src.probe_url, region: src.region,
            ok, latency_ms, error, selected: false }
    })).await
}

/// 清单结果连同"这份清单是哪来的、多久前的"一起给界面，
/// 拉新失败退回缓存时必须让用户看出来。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ListOutcome {
    pub versions: Vec<AvailableVersion>,
    pub source_id: String,
    pub source_label: String,
    /// 清单抓取时间（本地 ISO），用于"清单是 5 分钟前的"。
    pub fetched_at: Option<String>,
    /// 非空表示这次没能拉到新清单，界面要提示。
    pub stale_note: Option<String>,
}

/// 版本清单落盘缓存：网络抖的时候界面照样能装已经列出来的版本。
pub async fn versions(
    client: &reqwest::Client,
    plugin: &Plugin,
    src: &SourceInfo,
    refresh: bool,
) -> Result<ListOutcome> {
    // The Windows-only catalog must not reuse old archive download URLs.
    let catalog = if plugin.id() == ToolId::MySql { "mysql-windows-v2" } else { plugin.id().as_str() };
    let file = paths::layout()?.config_path(&format!("available-{catalog}.json"));
    let installed: Vec<String> = Registry::load()
        .map(|r| r.list(plugin.id()).into_iter().map(|x| x.version).collect())
        .unwrap_or_default();

    let cached = read_cached(&file, src);
    if !refresh {
        if let Some((age_ms, mut list)) = cached.clone()
            && Duration::from_millis(age_ms) < LIST_TTL
        {
            mark_installed(&mut list, &installed);
            return Ok(outcome(list, src, age_ms, None));
        }
    }

    match plugin.list_versions(client, src).await {
        Ok(mut list) => {
            list.iter_mut().for_each(|v| v.tool = plugin.id());
            let _ = write_cached(&file, src, &list);
            Ok(outcome(list, src, 0, None))
        }
        Err(e) => match cached {
            Some((age_ms, mut list)) => {
                mark_installed(&mut list, &installed);
                Ok(outcome(
                    list,
                    src,
                    age_ms,
                    Some(format!("没能连上源拉到最新清单，先按 {} 分钟的旧清单显示：{e}", age_ms / 60000 + 1),
                )))
            }
            None => Err(e),
        },
    }
}

fn outcome(
    versions: Vec<AvailableVersion>,
    src: &SourceInfo,
    age_ms: u64,
    stale_note: Option<String>,
) -> ListOutcome {
    ListOutcome {
        source_id: src.id.clone(),
        source_label: src.label.clone(),
        fetched_at: chrono::Local::now()
            .checked_sub_signed(chrono::TimeDelta::milliseconds(age_ms as i64))
            .map(|t| t.to_rfc3339()),
        versions,
        stale_note,
    }
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        // 系统时间被调到 1970 之前时不至于 panic，当作"很旧"处理。
        .unwrap_or(u64::MAX)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Envelope {
    #[serde(default)]
    source_key: String,
    fetched_at_ms: u64,
    versions: Vec<AvailableVersion>,
}

fn source_key(src: &SourceInfo) -> String { format!("{}|{}|{:?}", src.id, src.base, src.wrap) }

fn read_cached(file: &Path, src: &SourceInfo) -> Option<(u64, Vec<AvailableVersion>)> {
    let env: Envelope = util::read_json_opt(file).ok()?.unwrap_or(Envelope {
        source_key: String::new(),
        fetched_at_ms: 0,
        versions: Vec::new(),
    });
    if env.versions.is_empty() || env.source_key != source_key(src) {
        return None;
    }
    let age = now_ms().saturating_sub(env.fetched_at_ms);
    Some((age, env.versions))
}

fn write_cached(file: &Path, src: &SourceInfo, list: &[AvailableVersion]) -> Result<()> {
    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct WriteEnvelope<'a> {
        source_key: String,
        source_id: String,
        source_label: String,
        fetched_at_ms: u64,
        versions: &'a [AvailableVersion],
    }
    util::write_json(
        file,
        &WriteEnvelope {
            source_key: source_key(src),
            source_id: src.id.clone(),
            source_label: src.label.clone(),
            fetched_at_ms: now_ms(),
            versions: list,
        },
    )
}

fn mark_installed(list: &mut [AvailableVersion], installed: &[String]) {
    for v in list {
        v.installed = installed.iter().any(|i| i == &v.version);
    }
}

#[cfg(test)]
mod region_tests {
    use super::*;
    #[test]
    fn changing_mirror_never_reuses_another_catalog() {
        let root = std::env::temp_dir().join(format!("envswitch-catalog-{}-{}", std::process::id(), now_ms()));
        std::fs::create_dir_all(&root).unwrap();
        let file = root.join("versions.json");
        let plugin = Plugin::for_tool(ToolId::Deno).unwrap();
        let official = plugin.sources().remove(0);
        let mirror = plugin.sources().remove(1);
        write_cached(&file, &official, &[AvailableVersion { tool: ToolId::Deno, version: "2.5.6".into(), ..Default::default() }]).unwrap();
        assert!(read_cached(&file, &official).is_some());
        assert!(read_cached(&file, &mirror).is_none());
        let _ = std::fs::remove_dir_all(root);
    }
    fn probe(id: &str, region: &'static str, latency: u64, ok: bool) -> MirrorProbe {
        MirrorProbe { id: id.into(), label: id.into(), url: String::new(), region,
            ok, latency_ms: Some(latency), error: None, selected: false }
    }
    #[test]
    fn china_prefers_domestic_but_falls_back_when_unavailable() {
        let mut probes = vec![probe("official", "海外", 10, true), probe("mirror", "国内", 100, true)];
        assert_eq!(best_probe(&probes, "CN").unwrap().id, "mirror");
        probes[1].ok = false;
        assert_eq!(best_probe(&probes, "CN").unwrap().id, "official");
        probes[0].ok = false;
        assert!(best_probe(&probes, "CN").is_none());
    }
    #[test]
    fn overseas_prefers_official_and_latency_breaks_ties() {
        let probes = vec![probe("cn", "国内", 5, true), probe("slow", "海外", 100, true), probe("fast", "海外", 20, true)];
        assert_eq!(best_probe(&probes, "US").unwrap().id, "fast");
        assert_eq!(best_probe(&probes, "GLOBAL").unwrap().id, "fast");
    }
    #[test]
    fn china_uses_available_relay_before_overseas() {
        let probes = vec![probe("official", "海外", 20, true), probe("relay", "第三方", 100, true)];
        assert_eq!(best_probe(&probes, "CN").unwrap().id, "relay");
        assert_eq!(best_probe(&probes, "TW").unwrap().id, "official");
    }
}
