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
    let best = probes
        .iter()
        .filter(|p| p.ok)
        .min_by_key(|p| p.latency_ms.unwrap_or(u64::MAX))
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
        ToolId::Python => {
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
        && let Some((_, id)) = list.iter().find(|(t, _)| t == tool.as_str())
    {
        return Some(id.clone());
    }
    // 内存里没有就看磁盘，重启后一小时内不必再测一遍。
    let file = auto_cache_path().ok()?;
    let stored: StoredChoice = util::read_json_opt(&file).ok()?.unwrap_or_default();
    let at = chrono::DateTime::parse_from_rfc3339(&stored.at).ok()?;
    let age = chrono::Local::now().signed_duration_since(at.with_timezone(&chrono::Local));
    if age.num_seconds() < AUTO_TTL.as_secs() as i64 {
        stored
            .chosen
            .iter()
            .find(|(t, _)| t == tool.as_str())
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
    list.retain(|(t, _)| t != tool.as_str());
    list.push((tool.as_str().to_string(), id.to_string()));
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

/// 逐个源量一次首字节时间。失败不算错误，只是这条不参与选择。
pub async fn probe_sources(client: &reqwest::Client, plugin: &Plugin) -> Vec<MirrorProbe> {
    let mut out = Vec::new();
    for src in plugin.sources() {
        let start = Instant::now();
        let result = client
            .get(&src.probe_url)
            .header(reqwest::header::RANGE, "bytes=0-1023")
            .send()
            .await
            .and_then(|r| Ok(r.status().as_u16()));
        let latency = start.elapsed().as_millis() as u64;
        match result {
            Ok(code) if code < 500 => out.push(MirrorProbe {
                id: src.id,
                label: src.label,
                url: src.probe_url,
                region: src.region,
                ok: code < 400 || code == 404 || code == 403,
                latency_ms: Some(latency),
                error: if code < 400 {
                    None
                } else {
                    Some(format!("远端回了 {code}"))
                },
                selected: false,
            }),
            Ok(code) => out.push(MirrorProbe {
                id: src.id,
                label: src.label,
                url: src.probe_url,
                region: src.region,
                ok: false,
                latency_ms: Some(latency),
                error: Some(format!("远端回了 {code}")),
                selected: false,
            }),
            Err(e) => out.push(MirrorProbe {
                id: src.id,
                label: src.label,
                url: src.probe_url,
                region: src.region,
                ok: false,
                latency_ms: None,
                error: Some(e.to_string()),
                selected: false,
            }),
        }
    }
    out
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
    let file = paths::layout()?
        .config_path(&format!("available-{}.json", plugin.id().as_str()));
    let installed: Vec<String> = Registry::load()
        .map(|r| r.list(plugin.id()).into_iter().map(|x| x.version).collect())
        .unwrap_or_default();

    let cached = read_cached(&file);
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
    fetched_at_ms: u64,
    versions: Vec<AvailableVersion>,
}

fn read_cached(file: &Path) -> Option<(u64, Vec<AvailableVersion>)> {
    let env: Envelope = util::read_json_opt(file).ok()?.unwrap_or(Envelope {
        fetched_at_ms: 0,
        versions: Vec::new(),
    });
    if env.versions.is_empty() {
        return None;
    }
    let age = now_ms().saturating_sub(env.fetched_at_ms);
    Some((age, env.versions))
}

fn write_cached(file: &Path, src: &SourceInfo, list: &[AvailableVersion]) -> Result<()> {
    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct WriteEnvelope<'a> {
        source_id: String,
        source_label: String,
        fetched_at_ms: u64,
        versions: &'a [AvailableVersion],
    }
    util::write_json(
        file,
        &WriteEnvelope {
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
