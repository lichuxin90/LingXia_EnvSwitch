//! 检查更新：读 GitHub Releases 的最新标签，和本程序版本比一下。
//! 只做只读请求，不自动下载，装不装由人决定。

use serde::Serialize;

use super::error::{Error, Result};
use super::mirror::probe_client;
use super::settings::Settings;
use super::util;
use super::VERSION;

/// 仓库地址：界面里的 GitHub 链接也拼这个。
pub const REPO: &str = "lichuxin90/LingXia_EnvSwitch";
const LATEST_API: &str = "https://api.github.com/repos/lichuxin90/LingXia_EnvSwitch/releases/latest";
const RELEASES_HTML: &str = "https://github.com/lichuxin90/LingXia_EnvSwitch/releases";

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateInfo {
    pub current: String,
    pub latest: String,
    pub title: String,
    pub notes: String,
    pub has_update: bool,
    pub url: String,
}

fn text(v: &serde_json::Value, key: &str) -> String {
    v.get(key)
        .and_then(|x| x.as_str())
        .unwrap_or_default()
        .to_string()
}

pub async fn check() -> Result<UpdateInfo> {
    let client = probe_client(&Settings::load()?)?;
    let resp = client
        .get(LATEST_API)
        .header("Accept", "application/vnd.github+json")
        .send()
        .await
        .map_err(|e| Error::Detail(format!("连不上仓库：{e}")))?;
    let status = resp.status();
    // GitHub 在"仓库有了、但一次 Release 都没发"时也回 404，跟仓库不存在是同一串数字。
    // 报状态码会让人以为是网络故障，这里按没发布处理。
    if status == reqwest::StatusCode::NOT_FOUND {
        return Err(Error::Detail("仓库还没发布任何版本".to_string()));
    }
    if !status.is_success() {
        return Err(Error::Detail(format!("仓库返回状态 {}", status.as_u16())));
    }
    let body: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| Error::Detail(format!("仓库返回的内容读不了：{e}")))?;
    let tag = text(&body, "tag_name");
    if tag.trim().is_empty() {
        return Err(Error::Detail("仓库还没发布任何版本".to_string()));
    }
    let url = {
        let u = text(&body, "html_url");
        if u.trim().is_empty() {
            RELEASES_HTML.to_string()
        } else {
            u
        }
    };
    let title = {
        let n = text(&body, "name");
        if n.trim().is_empty() {
            tag.clone()
        } else {
            n
        }
    };
    let has_update = util::version_cmp(&tag, VERSION) == std::cmp::Ordering::Greater;
    Ok(UpdateInfo {
        current: VERSION.to_string(),
        latest: tag,
        title,
        notes: text(&body, "body"),
        has_update,
        url,
    })
}

/// 公共仓库 Star 数，短时间切换首页/关于不重复占用 GitHub API 配额。
pub async fn stars() -> Result<u64> {
    use std::{sync::Mutex, time::{Duration, Instant}};
    static CACHE: Mutex<Option<(Instant, u64)>> = Mutex::new(None);
    if let Some((_, count)) = CACHE.lock().ok().and_then(|g| *g).filter(|(at,_)| at.elapsed() < Duration::from_secs(600)) {
        return Ok(count);
    }
    let client = probe_client(&Settings::load()?)?;
    let body: serde_json::Value = client.get(format!("https://api.github.com/repos/{REPO}"))
        .header("Accept", "application/vnd.github+json").send().await?.error_for_status()?.json().await?;
    let count = body["stargazers_count"].as_u64().ok_or_else(|| Error::Detail("仓库未返回 Star 数量".into()))?;
    if let Ok(mut cache) = CACHE.lock() { *cache = Some((Instant::now(), count)); }
    Ok(count)
}
