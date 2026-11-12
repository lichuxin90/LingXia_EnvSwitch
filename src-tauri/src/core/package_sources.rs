use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use serde::{Deserialize, Serialize};
use super::{error::{Error, Result}, mirror, paths, settings::Settings, util};

pub const MANAGERS: [&str; 5] = ["pip", "npm", "composer", "deno", "deno-jsr"];
const CACHE_TTL_MS: u64 = 6 * 3600 * 1000;
const FAILED_TTL_MS: u64 = 5 * 60 * 1000;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PackageSource {
    pub id: &'static str,
    pub label: &'static str,
    pub url: &'static str,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PackageProbe {
    pub id: String,
    pub ok: bool,
    pub latency_ms: Option<u64>,
    pub error: Option<String>,
}

pub fn sources(manager: &str) -> Vec<PackageSource> {
    let entries: &[(&str, &str, &str)] = match manager {
        "pip" => &[
            ("official", "PyPI", "https://pypi.org/simple/"),
            ("tsinghua", "清华大学 TUNA", "https://pypi.tuna.tsinghua.edu.cn/simple/"),
            ("aliyun", "阿里云 Aliyun", "https://mirrors.aliyun.com/pypi/simple/"),
            ("ustc", "中国科学技术大学 USTC", "https://pypi.mirrors.ustc.edu.cn/simple/"),
            ("douban", "豆瓣 Douban", "https://pypi.douban.com/simple/"),
            ("huawei", "华为云 Huawei Cloud", "https://repo.huaweicloud.com/repository/pypi/simple/"),
            ("bfsu", "北京外国语大学 BFSU", "https://mirrors.bfsu.edu.cn/pypi/web/simple/"),
        ],
        "npm" | "deno" => &[
            ("official", "npm", "https://registry.npmjs.org/"),
            ("npmmirror", "npmmirror", "https://registry.npmmirror.com/"),
            ("tencent", "腾讯云 Tencent Cloud", "https://mirrors.cloud.tencent.com/npm/"),
            ("huawei", "华为云 Huawei Cloud", "https://repo.huaweicloud.com/repository/npm/"),
            ("ustc", "中国科学技术大学 USTC", "https://npmreg.ustc.edu.cn/"),
        ],
        "composer" => &[
            ("official", "Packagist", "https://repo.packagist.org/"),
            ("aliyun", "阿里云 Aliyun", "https://mirrors.aliyun.com/composer/"),
            ("sjtug", "上海交通大学 SJTUG", "https://packagist.mirrors.sjtug.sjtu.edu.cn/"),
            ("tencent", "腾讯云 Tencent Cloud", "https://mirrors.tencent.com/composer/"),
        ],
        "deno-jsr" => &[
            ("official", "JSR", "https://jsr.io/"),
            ("deno-cn", "JSR 国内社区镜像", "https://jsr.deno.org.cn/"),
        ],
        _ => &[],
    };
    entries.iter().map(|(id, label, url)| PackageSource { id, label, url }).collect()
}

pub fn validate_url(raw: &str) -> Result<()> {
    let url = url::Url::parse(raw).map_err(|_| Error::Config("仓库地址必须是完整的 HTTP(S) URL".into()))?;
    if !matches!(url.scheme(), "http" | "https") || url.host_str().is_none()
        || !url.username().is_empty() || url.password().is_some() || url.query().is_some() || url.fragment().is_some()
    {
        return Err(Error::Config("仓库地址仅支持 HTTP(S)，请勿包含账号、密码、查询参数或片段".into()));
    }
    Ok(())
}

pub fn validate(settings: &Settings) -> Result<()> {
    for (manager, choice) in &settings.package_sources {
        if !MANAGERS.contains(&manager.as_str()) { continue; }
        if let Some(url) = choice.strip_prefix("url:") { validate_url(url)?; }
        else if choice != "auto" && !sources(manager).iter().any(|s| s.id == choice) {
            return Err(Error::Config(format!("{manager} 的源不存在：{choice}")));
        }
    }
    Ok(())
}

fn probe_suffix(manager: &str) -> &'static str {
    match manager {
        "pip" => "pip/",
        "composer" => "packages.json",
        "deno-jsr" => "@std/assert/meta.json",
        _ => "is-number",
    }
}

fn valid_metadata(manager: &str, body: &[u8]) -> bool {
    if manager == "pip" {
        let html = String::from_utf8_lossy(body).to_ascii_lowercase();
        return html.contains("<a ") && html.contains("pip-") && html.contains(".whl");
    }
    let Ok(value) = serde_json::from_slice::<serde_json::Value>(body) else { return false; };
    match manager {
        "composer" => value.get("packages").is_some() || value.get("metadata-url").is_some(),
        "deno-jsr" => value["versions"].is_object() && value["name"] == "assert",
        _ => value["name"] == "is-number" && value["versions"].is_object(),
    }
}

pub async fn probe_with(manager: &str, settings: &Settings) -> Result<Vec<PackageProbe>> {
    let sources = sources(manager);
    if sources.is_empty() { return Err(Error::NotFound(format!("不支持 {manager} 包源"))); }
    let client = mirror::probe_client(settings)?;
    Ok(futures_util::future::join_all(sources.into_iter().map(|source| {
        let client = client.clone();
        async move {
            let start = Instant::now();
            let result: Result<()> = async {
                let url = format!("{}/{}", source.url.trim_end_matches('/'), probe_suffix(manager));
                let requested = url::Url::parse(&url).map_err(|e| Error::Config(e.to_string()))?;
                let mut response = client.get(requested.clone()).timeout(Duration::from_secs(6)).send().await?.error_for_status()?;
                // Deno rejects registry redirects, even when a browser can follow them successfully.
                if manager == "deno-jsr" && response.url() != &requested {
                    return Err(Error::Network("此 JSR 源发生重定向，Deno 不支持；请使用官方或其他可用源".into()));
                }
                let mut body = Vec::new();
                while let Some(chunk) = response.chunk().await? {
                    if body.len() + chunk.len() > 1024 * 1024 {
                        return Err(Error::Network("源返回了过大的元数据".into()));
                    }
                    body.extend_from_slice(&chunk);
                }
                if !valid_metadata(manager, &body) { return Err(Error::Network("源未返回有效的包元数据".into())); }
                Ok(())
            }.await;
            PackageProbe {
                id: source.id.into(), ok: result.is_ok(),
                latency_ms: result.as_ref().ok().map(|_| start.elapsed().as_millis() as u64),
                error: result.err().map(|e| e.to_string()),
            }
        }
    })).await)
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CachedProbes {
    at_ms: u64,
    network: String,
    probes: Vec<PackageProbe>,
}

fn now_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_millis() as u64
}

fn network_key(settings: &Settings) -> String {
    // Invalidate on catalog/proxy changes without persisting proxy credentials.
    use sha2::{Digest, Sha256};
    hex::encode(Sha256::digest(format!("catalog-v3:{:?}:{:?}", settings.proxy_mode, settings.proxy_url).as_bytes()))
}

fn fresh(cache: &CachedProbes, network: &str, now: u64) -> bool {
    let ttl = if cache.probes.iter().any(|p| p.ok) { CACHE_TTL_MS } else { FAILED_TTL_MS };
    cache.network == network && now >= cache.at_ms && now - cache.at_ms < ttl
}

fn fastest_source<'a>(sources: &'a [PackageSource], probes: &[PackageProbe]) -> Option<&'a PackageSource> {
    sources.iter().filter_map(|s| {
        let p = probes.iter().find(|p| p.id == s.id && p.ok && p.latency_ms.is_some())?;
        Some((s, p.latency_ms?))
    }).min_by_key(|(_, latency)| *latency).map(|(source, _)| source)
}

fn choose_url(manager: &str, settings: &Settings, probes: &[PackageProbe]) -> Option<String> {
    let sources = sources(manager);
    let choice = settings.package_sources.get(manager).map(String::as_str).unwrap_or("auto");
    if let Some(url) = choice.strip_prefix("url:") { return validate_url(url).ok().map(|_| url.to_string()); }
    if choice != "auto" { return sources.iter().find(|s| s.id == choice).map(|s| s.url.to_string()); }
    fastest_source(&sources, probes).or_else(|| sources.iter().find(|s| s.id == "official")).map(|s| s.url.to_string())
}

async fn refresh_with(manager: &str, settings: &Settings) -> Result<Vec<PackageProbe>> {
    let probes = probe_with(manager, settings).await?;
    if let Ok(layout) = paths::layout() {
        // An unwritable cache must not stop the package command.
        let _ = util::write_json(&layout.config_path(&format!("package-source-{manager}.json")), &CachedProbes {
            at_ms: now_ms(), network: network_key(settings), probes: probes.clone(),
        });
    }
    Ok(probes)
}

/// First use works without visiting settings; subsequent invocations reuse a six-hour cache.
pub async fn resolve_url(manager: &str, settings: &Settings) -> Option<String> {
    let choice = settings.package_sources.get(manager).map(String::as_str).unwrap_or("auto");
    if choice != "auto" { return choose_url(manager, settings, &[]); }
    if let Ok(layout) = paths::layout() {
        let path = layout.config_path(&format!("package-source-{manager}.json"));
        if let Ok(Some(cache)) = util::read_json_opt::<CachedProbes>(&path)
            && fresh(&cache, &network_key(settings), now_ms()) {
            return choose_url(manager, settings, &cache.probes);
        }
    }
    let probes = refresh_with(manager, settings).await.unwrap_or_default();
    choose_url(manager, settings, &probes)
}

pub async fn refresh_auto(manager: &str) -> Result<Vec<PackageProbe>> {
    refresh_with(manager, &Settings::load()?).await
}

#[cfg(test)]
mod tests {
    use super::*;
    fn p(id: &str, ok: bool, ms: Option<u64>) -> PackageProbe {
        PackageProbe { id: id.into(), ok, latency_ms: ms, error: None }
    }

    #[test]
    fn auto_ignores_failed_unknown_and_unmeasured_sources() {
        let probes = vec![p("obsolete", true, Some(1)), p("aliyun", false, Some(2)),
            p("bfsu", true, None), p("official", true, Some(80)), p("tsinghua", true, Some(12))];
        assert_eq!(fastest_source(&sources("pip"), &probes).unwrap().id, "tsinghua");
        assert_eq!(choose_url("pip", &Settings::default(), &[]).as_deref(), Some("https://pypi.org/simple/"));
    }

    #[test]
    fn manual_choice_wins_and_custom_url_is_validated() {
        let mut s = Settings::default();
        s.package_sources.insert("pip".into(), "aliyun".into());
        assert_eq!(choose_url("pip", &s, &[p("tsinghua", true, Some(1))]).as_deref(), Some("https://mirrors.aliyun.com/pypi/simple/"));
        s.package_sources.insert("pip".into(), "url:https://example.test/simple/".into());
        assert_eq!(choose_url("pip", &s, &[]).as_deref(), Some("https://example.test/simple/"));
        for url in ["ftp://example.test/", "https://user:secret@example.test/", "https://example.test/#fragment", "not a url"] {
            s.package_sources.insert("pip".into(), format!("url:{url}"));
            assert!(validate(&s).is_err());
            assert!(choose_url("pip", &s, &[]).is_none());
        }
    }

    #[test]
    fn cache_expires_on_age_proxy_or_clock_change() {
        let cache = CachedProbes { at_ms: 10_000, network: "n".into(), probes: vec![p("official", true, Some(1))] };
        assert!(fresh(&cache, "n", 10_001));
        assert!(!fresh(&cache, "other", 10_001));
        assert!(!fresh(&cache, "n", 9_999));
        assert!(!fresh(&cache, "n", 10_000 + CACHE_TTL_MS));
        let failed = CachedProbes { probes: vec![p("official", false, None)], ..cache };
        assert!(!fresh(&failed, "n", 10_000 + FAILED_TTL_MS));
    }

    #[test]
    fn probe_rejects_html_challenges_and_wrong_registry_content() {
        assert!(!valid_metadata("npm", br#"<html>OK</html>"#));
        assert!(!valid_metadata("deno-jsr", br#"{"name":"is-number","versions":{}}"#));
        assert!(valid_metadata("npm", br#"{"name":"is-number","versions":{}}"#));
        assert!(valid_metadata("composer", br#"{"packages":[],"metadata-url":"/p2/%package%.json"}"#));
        assert!(valid_metadata("pip", br#"<a href="pip-1.whl">pip</a>"#));
    }

    #[tokio::test]
    #[ignore = "联网验证包仓库，不参与离线测试"]
    async fn live_package_sources() {
        for manager in MANAGERS {
            let probes = probe_with(manager, &Settings::default()).await.unwrap();
            for p in &probes { eprintln!("{manager}/{}: {} {:?} {:?}", p.id, p.ok, p.latency_ms, p.error); }
            assert!(fastest_source(&sources(manager), &probes).is_some(), "{manager} 没有可用仓库");
        }
    }
}
