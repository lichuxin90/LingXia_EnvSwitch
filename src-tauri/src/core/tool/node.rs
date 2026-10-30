use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use serde::Deserialize;

use crate::core::error::{Error, Result};
use crate::core::model::{AvailableVersion, Companion, ToolId};
use crate::core::util;

use super::{Artifact, ArchiveKind, ShimSpec, ShimTarget, SourceInfo, apply_wrap, strip_v};

#[derive(Debug, Clone, Copy)]
pub struct NodePlugin;

const OFFICIAL: &str = "https://nodejs.org/dist";
const NPMMIRROR: &str = "https://cdn.npmmirror.com/binaries/node";

impl NodePlugin {
    pub fn sources(&self) -> Vec<SourceInfo> {
        vec![
            SourceInfo {
                id: "official".into(),
                label: "官方 nodejs.org".into(),
                region: "海外",
                base: OFFICIAL.into(),
                wrap: None,
                note: None,
                probe_url: format!("{OFFICIAL}/index.json"),
            },
            SourceInfo {
                id: "npmmirror".into(),
                label: "阿里 npmmirror".into(),
                region: "国内",
                base: NPMMIRROR.into(),
                wrap: None,
                note: Some("国内下载明显更快，包内容与官方一致（按 SHASUMS256 核对）".into()),
                probe_url: format!("{NPMMIRROR}/index.json"),
            },
        ]
    }

    fn platform_token() -> &'static str {
        if cfg!(target_os = "windows") {
            if cfg!(target_arch = "aarch64") {
                "win-arm64"
            } else {
                "win-x64"
            }
        } else if cfg!(target_os = "macos") {
            if cfg!(target_arch = "aarch64") {
                "darwin-arm64"
            } else {
                "darwin-x64"
            }
        } else if cfg!(target_arch = "aarch64") {
            "linux-arm64"
        } else {
            "linux-x64"
        }
    }

    /// 版本清单 files 数组用的写法，和包名里的 token 不是一套：
    /// Windows 的 zip 写成 `win-x64-zip`，macOS 写成 `osx-x64-tar`，只有 Linux 两边一致。
    fn files_token() -> &'static str {
        if cfg!(target_os = "windows") {
            if cfg!(target_arch = "aarch64") {
                "win-arm64-zip"
            } else {
                "win-x64-zip"
            }
        } else if cfg!(target_os = "macos") {
            if cfg!(target_arch = "aarch64") {
                "osx-arm64-tar"
            } else {
                "osx-x64-tar"
            }
        } else if cfg!(target_arch = "aarch64") {
            "linux-arm64"
        } else {
            "linux-x64"
        }
    }

    fn archive_ext() -> &'static str {
        if cfg!(target_os = "windows") {
            "zip"
        } else {
            "tar.gz"
        }
    }

    pub async fn list_versions(&self, client: &reqwest::Client, src: &SourceInfo) -> Result<Vec<AvailableVersion>> {
        let raw: String = client
            .get(format!("{}/index.json", src.base.trim_end_matches('/')))
            .send()
            .await
            .map_err(|e| Error::Network(format!("拿不到 Node 版本清单：{e}")))?
            .text()
            .await
            .map_err(|e| Error::Network(format!("版本清单读不完：{e}")))?;

        let items: Vec<IndexItem> = serde_json::from_str(&raw)
            .map_err(|e| Error::Config(format!("Node 版本清单格式看不懂：{e}")))?;

        let schedule = load_schedule(client).await;
        let token = Self::platform_token();
        let files_token = Self::files_token();
        let ext = Self::archive_ext();
        let installed: Vec<String> = crate::core::records::Registry::load()
            .map(|r| r.list(ToolId::Node).into_iter().map(|x| x.version).collect())
            .unwrap_or_default();

        let mut out: Vec<AvailableVersion> = Vec::new();
        for it in items {
            let version = strip_v(&it.version);
            let has_platform = it.files.iter().any(|f| f == files_token);
            let lts_codename = match &it.lts {
                LtsField::Str(s) if !s.is_empty() => Some(s.clone()),
                _ => None,
            };
            // 有的镜像把 LTS 版本写成 `lts: true` 而没有代号，这种也算 LTS。
            let is_lts = lts_codename.is_some() || matches!(&it.lts, LtsField::Flag(true));
            let line_key = version.split('.').next().unwrap_or("0").to_string();
            let row = schedule.as_ref().and_then(|s| s.get(&line_key));
            let lts_active = row.map(|r| r.in_support());
            let eol_date = row.and_then(|r| r.end.clone());
            let eol = eol_date.as_deref().map(super::eol::is_past);
            let note = if eol == Some(true) {
                eol_date
                    .as_ref()
                    .map(|d| format!("这条线已于 {d} 停止更新，只用它复现老项目"))
            } else {
                None
            };
            let file_name = format!("node-v{version}-{token}.{ext}");
            out.push(AvailableVersion {
                tool: ToolId::Node,
                line_key,
                released: Some(it.date.clone()),
                lts_codename: lts_codename.clone(),
                is_lts,
                lts_active,
                eol,
                eol_date,
                size_hint: None,
                installed: installed.iter().any(|v| v == &version),
                download_url: if has_platform {
                    Some(apply_wrap(
                        src,
                        &format!(
                            "{}/v{version}/{file_name}",
                            src.base.trim_end_matches('/')
                        ),
                    ))
                } else {
                    None
                },
                file_name: has_platform.then_some(file_name.clone()).or(Some(file_name)),
                version: version.clone(),
                sha256: None,
                origin_tag: None,
                unavailable_reason: if has_platform {
                    note.clone()
                } else {
                    Some("官方没有为这台电脑发布这个版本".to_string())
                }
                .or(note),
                is_line_latest: false,
                line_count: 1,
            });
        }

        // 主线内最新小版本用来折叠卡片，其余进"更多小版本"列表。
        out.sort_by(|a, b| util::version_desc(&a.version, &b.version));
        let mut seen: HashMap<String, usize> = HashMap::new();
        for v in &mut out {
            let idx = seen.entry(v.line_key.clone()).or_insert(0);
            *idx += 1;
            v.is_line_latest = *idx == 1;
            v.line_count = 0;
        }
        let counts: HashMap<String, usize> = seen.clone();
        for v in &mut out {
            v.line_count = counts.get(&v.line_key).copied().unwrap_or(1);
        }
        Ok(out)
    }

    pub async fn artifact(
        &self,
        client: &reqwest::Client,
        src: &SourceInfo,
        v: &AvailableVersion,
    ) -> Result<Artifact> {
        let file_name = v
            .file_name
            .clone()
            .ok_or_else(|| Error::Detail("没算出该下哪个文件".to_string()))?;
        let url = v.download_url.clone().ok_or_else(|| {
            Error::NotFound(format!(
                "官方没有为这台电脑发布 Node {}，换不了",
                v.version
            ))
        })?;
        let sha = fetch_sha256(client, src, &v.version, &file_name).await;
        Ok(Artifact {
            url,
            file_name,
            sha256: sha.ok().flatten(),
            size_hint: v.size_hint,
            kind: if cfg!(target_os = "windows") { ArchiveKind::Zip } else { ArchiveKind::TarGz },
        })
    }

    /// npm 版本从自带的 package.json 里读，不额外起进程。
    pub fn companions(&self, root: &Path) -> Vec<Companion> {
        let mut out = Vec::new();
        let npm_pkg = if cfg!(windows) {
            root.join("node_modules").join("npm").join("package.json")
        } else {
            root.join("lib").join("node_modules").join("npm").join("package.json")
        };
        if let Some(ver) = read_package_version(&npm_pkg) {
            out.push(Companion { name: "npm".into(), version: Some(ver), note: None });
        } else {
            out.push(Companion {
                name: "npm".into(),
                version: None,
                note: Some("没在预期位置找到 npm，可能是非官方包".into()),
            });
        }
        out.push(Companion {
            name: "npx".into(),
            version: out.first().and_then(|c| c.version.clone()),
            note: None,
        });
        out
    }
}

fn read_package_version(path: &Path) -> Option<String> {
    let text = std::fs::read_to_string(path).ok()?;
    let value: serde_json::Value = serde_json::from_str(&text).ok()?;
    value.get("version")?.as_str().map(|s| s.to_string())
}

pub fn exec_rel() -> PathBuf {
    if cfg!(windows) {
        PathBuf::from("node.exe")
    } else {
        PathBuf::from("bin").join("node")
    }
}

pub fn payload_markers() -> Vec<PathBuf> {
    vec![exec_rel()]
}

pub fn shim_specs() -> Vec<ShimSpec> {
    let npm_cli = if cfg!(windows) {
        PathBuf::from("node_modules").join("npm").join("bin").join("npm-cli.js")
    } else {
        PathBuf::from("lib")
            .join("node_modules")
            .join("npm")
            .join("bin")
            .join("npm-cli.js")
    };
    let npx_cli = npm_cli.with_file_name("npx-cli.js");
    vec![
        ShimSpec { name: "node".into(), target: ShimTarget::Bin(exec_rel()) },
        ShimSpec { name: "npm".into(), target: ShimTarget::Script(npm_cli) },
        ShimSpec { name: "npx".into(), target: ShimTarget::Script(npx_cli) },
    ]
}

#[derive(Debug, Deserialize)]
struct IndexItem {
    version: String,
    date: String,
    files: Vec<String>,
    #[serde(default)]
    lts: LtsField,
}

#[derive(Debug, Deserialize, Default)]
#[serde(untagged)]
enum LtsField {
    #[default]
    Off,
    Str(String),
    /// 镜像站会把非 LTS 写成 `false`，官方写 `null`，两种都得认。
    Flag(bool),
}

// ---------- SHASUMS256 ----------

static SHA_CACHE: OnceLock<Mutex<HashMap<String, HashMap<String, String>>>> = OnceLock::new();
static SHA_FETCHED: OnceLock<Mutex<HashMap<String, Instant>>> = OnceLock::new();

fn sha_cache() -> &'static Mutex<HashMap<String, HashMap<String, String>>> {
    SHA_CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

fn sha_fetched() -> &'static Mutex<HashMap<String, Instant>> {
    SHA_FETCHED.get_or_init(|| Mutex::new(HashMap::new()))
}

/// 每个版本目录一份 SHASUMS256.txt，几 KB，取到就缓存，卸载重装不会重复请求。
async fn fetch_sha256(
    client: &reqwest::Client,
    src: &SourceInfo,
    version: &str,
    file_name: &str,
) -> Result<Option<String>> {
    let key = format!("{}/v{version}", src.base.trim_end_matches('/'));
    if let Some(h) = sha_cache()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .get(&key)
        .and_then(|m| m.get(file_name))
        .cloned()
    {
        return Ok(Some(h));
    }
    let fresh = sha_fetched()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .get(&key)
        .map(|t| t.elapsed() > Duration::from_secs(600))
        .unwrap_or(true);
    if !fresh {
        return Ok(None);
    }

    let url = format!("{key}/SHASUMS256.txt");
    let resp = client
        .get(&url)
        .send()
        .await
        .map_err(|e| Error::Network(format!("取校验值失败：{e}")))?;
    if !resp.status().is_success() {
        return Err(Error::NotFound(format!(
            "校验值文件不存在（{}）",
            resp.status().as_u16()
        )));
    }
    let text = resp.text().await?;
    let mut map = HashMap::new();
    for line in text.lines() {
        let mut parts = line.splitn(2, char::is_whitespace);
        let (Some(hash), Some(name)) = (parts.next(), parts.next()) else { continue };
        let name = name.trim_start_matches(['*', ' ', '\t']).trim().to_string();
        let hash = hash.trim().to_lowercase();
        if hash.len() == 64 {
            map.insert(name, hash);
        }
    }
    let found = map.get(file_name).cloned();
    sha_cache()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .insert(key.clone(), map);
    sha_fetched()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .insert(key, Instant::now());
    Ok(found)
}

// ---------- 维护时间表（尽力而为） ----------

type Schedule = HashMap<String, ScheduleRow>;
static SCHEDULE: Mutex<Option<(Instant, Schedule)>> = Mutex::new(None);

/// nodejs/Release 的 schedule.json 是 `{"v20": {...}}` 这种对象，不是数组。
#[derive(Debug, Clone, Deserialize)]
struct RawRow {
    /// 彻底停止更新的日期。
    #[serde(default)]
    end: Option<String>,
}

#[derive(Debug, Clone)]
struct ScheduleRow {
    end: Option<String>,
}

impl ScheduleRow {
    /// 还在维护期内 = 没到今天为止的那个日期。
    fn in_support(&self) -> bool {
        let today = chrono::Local::now().format("%Y-%m-%d").to_string();
        match self.end.as_deref() {
            Some(d) => d > today.as_str(),
            None => true,
        }
    }
}

async fn load_schedule(client: &reqwest::Client) -> Option<Schedule> {
    if let Ok(guard) = SCHEDULE.lock()
        && let Some((at, map)) = guard.as_ref()
        && at.elapsed() < Duration::from_secs(86400)
    {
        return Some(map.clone());
    }
    let resp = client
        .get("https://raw.githubusercontent.com/nodejs/Release/main/schedule.json")
        .send()
        .await
        .ok()?;
    if !resp.status().is_success() {
        return None;
    }
    let rows: HashMap<String, RawRow> = serde_json::from_str(&resp.text().await.ok()?).ok()?;
    let map: Schedule = rows
        .into_iter()
        .filter_map(|(key, r)| {
            // 键是 "v20"，界面与数据里的线号是 "20"。
            let major = key.strip_prefix('v')?;
            Some((
                major.to_string(),
                ScheduleRow { end: r.end.filter(|s| !s.is_empty()) },
            ))
        })
        .collect();
    if map.is_empty() {
        return None;
    }
    if let Ok(mut guard) = SCHEDULE.lock() {
        *guard = Some((Instant::now(), map.clone()));
    }
    Some(map)
}

#[cfg(test)]
mod tests {
    use super::{IndexItem, LtsField, RawRow, ScheduleRow};
    use std::collections::HashMap;

    /// 镜像站把非 LTS 写成 false，官方写成 null，两种都要能读。
    #[test]
    fn lts_字段三种写法都认() {
        for json in [
            r#"{"version":"v26.1.0","date":"2026-01-01","files":[],"lts":"Lithium"}"#,
            r#"{"version":"v26.1.0","date":"2026-01-01","files":[],"lts":false}"#,
            r#"{"version":"v26.1.0","date":"2026-01-01","files":[],"lts":null}"#,
            r#"{"version":"v26.1.0","date":"2026-01-01","files":[]}"#,
        ] {
            let it: IndexItem = serde_json::from_str(json).expect(json);
            match it.lts {
                LtsField::Str(s) => assert_eq!(s, "Lithium"),
                LtsField::Flag(b) => assert!(!b),
                LtsField::Off => {}
            }
        }
    }

    /// 官方的写法是按 vNN 索引的对象，早先当成数组读，结果永远读不到。
    #[test]
    fn 时间表按版本号索引() {
        let raw = r#"{"v20":{"start":"2023-04-18","maintenance":"2025-04-01","end":"2026-04-30"},
                       "v22":{"start":"2024-04-24","maintenance":"2026-04-01","end":"2027-04-30"}}"#;
        let rows: HashMap<String, RawRow> = serde_json::from_str(raw).expect("时间表");
        let e20 = rows.get("v20").expect("有 20 这条线");
        assert_eq!(e20.end.as_deref(), Some("2026-04-30"));
        let old = ScheduleRow { end: e20.end.clone() };
        let fresh = ScheduleRow { end: rows["v22"].end.clone() };
        assert!(!old.in_support());
        assert!(fresh.in_support());
    }
}
