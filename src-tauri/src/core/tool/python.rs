use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::core::error::{Error, Result};
use crate::core::model::{AvailableVersion, Companion, ToolId};
use crate::core::util;

use super::{Artifact, ArchiveKind, ShimSpec, ShimTarget, SourceInfo, apply_wrap, eol};

/// Python 用 python-build-standalone 的预编译包，避免要求用户本地编译。
#[derive(Debug, Clone, Copy)]
pub struct PythonPlugin;

const API_RELEASES: &str =
    "https://api.github.com/repos/astral-sh/python-build-standalone/releases";
const GITHUB_DOWNLOAD: &str = "https://github.com/astral-sh/python-build-standalone/releases/download";

impl PythonPlugin {
    pub fn sources(&self) -> Vec<SourceInfo> {
        vec![
            SourceInfo {
                id: "github".into(),
                label: "官方 GitHub".into(),
                region: "海外",
                base: GITHUB_DOWNLOAD.into(),
                wrap: None,
                note: None,
                probe_url: format!("{API_RELEASES}?per_page=1"),
            },
            SourceInfo {
                id: "gh-relay".into(),
                label: "第三方中转加速".into(),
                region: "第三方",
                base: GITHUB_DOWNLOAD.into(),
                wrap: Some("https://gh-proxy.com/{url}".into()),
                note: Some(
                    "转发 GitHub 文件，速度通常快很多；包仍按官方 sha256 核对，不一致就拒绝安装"
                        .into(),
                ),
                probe_url: "https://gh-proxy.com/https://github.com/astral-sh/python-build-standalone/releases/download/20261026/cpython-3.12.16+20261026-x86_64-pc-windows-msvc-install_only.tar.gz".into(),
            },
        ]
    }

    fn triple() -> &'static str {
        if cfg!(target_os = "windows") {
            if cfg!(target_arch = "aarch64") {
                "aarch64-pc-windows-msvc"
            } else {
                "x86_64-pc-windows-msvc"
            }
        } else if cfg!(target_os = "macos") {
            if cfg!(target_arch = "aarch64") {
                "aarch64-apple-darwin"
            } else {
                "x86_64-apple-darwin"
            }
        } else if cfg!(target_arch = "aarch64") {
            "aarch64-unknown-linux-gnu"
        } else {
            "x86_64-unknown-linux-gnu"
        }
    }

    pub async fn list_versions(&self, client: &reqwest::Client, src: &SourceInfo) -> Result<Vec<AvailableVersion>> {
        let url = format!("{API_RELEASES}?per_page=12");
        let resp = client
            .get(&url)
            .header("User-Agent", "envswitch/0.1")
            .header("Accept", "application/vnd.github+json")
            .send()
            .await
            .map_err(|e| Error::Network(format!("拿不到 Python 版本清单：{e}")))?;
        if resp.status().as_u16() == 403 {
            return Err(Error::Network(
                "GitHub 说请求太多，等一小时再刷新，或者先装已列出的版本".to_string(),
            ));
        }
        if !resp.status().is_success() {
            return Err(Error::NotFound(format!(
                "GitHub 返回了 {}",
                resp.status().as_u16()
            )));
        }
        let releases: Vec<Release> = resp
            .json()
            .await
            .map_err(|e| Error::Config(format!("GitHub 的清单读不懂：{e}")))?;

        let triple = Self::triple();
        let installed: Vec<String> = crate::core::records::Registry::load()
            .map(|r| r.list(ToolId::Python).into_iter().map(|x| x.version).collect())
            .unwrap_or_default();

        let mut by_version: HashMap<String, AvailableVersion> = HashMap::new();
        let mut skipped: Vec<String> = Vec::new();
        let table = eol::schedule_for(client, "python").await;
        for rel in releases {
            for asset in rel.assets {
                let Some(version) = parse_asset(&asset.name, triple) else {
                    continue;
                };
                if by_version.contains_key(&version) {
                    // 后面的 release 是更旧的 tag，同名版本以最新 tag 为准。
                    continue;
                }
                let line = minor_line(&version);
                let row = table.as_ref().and_then(|t| t.get(&line));
                let sha = asset.digest.as_deref().and_then(|d| d.strip_prefix("sha256:"));
                by_version.insert(
                    version.clone(),
                    AvailableVersion {
                        tool: ToolId::Python,
                        version: version.clone(),
                        line_key: line.clone(),
                        released: Some(rel.published_at.clone()),
                        is_lts: row.map(|r| r.lts).unwrap_or(false),
                        lts_codename: None,
                        lts_active: row.map(|r| r.eol != Some(true)),
                        eol: row.and_then(|r| r.eol),
                        eol_date: row.and_then(|r| r.eol_date.clone()),
                        size_hint: Some(asset.size),
                        installed: installed.iter().any(|v| v == &version),
                        download_url: Some(apply_wrap(src, &asset.browser_download_url)),
                        file_name: Some(asset.name.clone()),
                        sha256: sha.map(|s| s.to_string()),
                        origin_tag: Some(rel.tag_name.clone()),
                        unavailable_reason: None,
                        is_line_latest: false,
                        line_count: 1,
                    },
                );
            }
            if by_version.is_empty() {
                skipped.push(rel.tag_name);
            }
        }

        if by_version.is_empty() {
            return Err(Error::Detail(format!(
                "最近的发布里没有给这台电脑（{}）准备的包，检查一下网络或换个源",
                triple
            )));
        }

        let mut out: Vec<AvailableVersion> = by_version.into_values().collect();
        out.sort_by(|a, b| util::version_desc(&a.version, &b.version));
        let mut seen: HashMap<String, usize> = HashMap::new();
        for v in &mut out {
            let idx = seen.entry(v.line_key.clone()).or_insert(0);
            *idx += 1;
            v.is_line_latest = *idx == 1;
        }
        let counts = seen.clone();
        for v in &mut out {
            v.line_count = counts.get(&v.line_key).copied().unwrap_or(1);
        }
        Ok(out)
    }

    pub fn artifact(&self, _src: &SourceInfo, v: &AvailableVersion) -> Result<Artifact> {
        Ok(Artifact {
            url: v
                .download_url
                .clone()
                .ok_or_else(|| Error::Detail("没算出该下哪个 Python 包".to_string()))?,
            file_name: v
                .file_name
                .clone()
                .ok_or_else(|| Error::Detail("包名丢了".to_string()))?,
            sha256: v.sha256.clone(),
            size_hint: v.size_hint,
            kind: ArchiveKind::TarGz,
        })
    }

    /// pip 版本从 site-packages 里的 dist-info 目录名读出来。
    pub fn companions(&self, root: &Path) -> Vec<Companion> {
        let mut out = Vec::new();
        let found = find_dist_info(root, "pip");
        out.push(Companion {
            name: "pip".into(),
            version: found.clone(),
            note: found
                .is_none()
                .then(|| "这个包里没有带 pip，装依赖前要先 python -m ensurepip".to_string()),
        });
        if let Some(v) = &out[0].version {
            out.push(Companion { name: "pip3".into(), version: Some(v.clone()), note: None });
        }
        out
    }
}

pub(crate) fn find_dist_info(root: &Path, pkg: &str) -> Option<String> {
    let site_dirs = vec![
        root.join("Lib").join("site-packages"),
        root.join("lib"),
    ];
    let prefix = format!("{pkg}-");
    for site in site_dirs {
        if !site.is_dir() {
            continue;
        }
        if let Some(v) = scan_dist(&site, &prefix) {
            return Some(v);
        }
        // Linux 包把 site-packages 放在 lib/python3.12/ 下面。
        if let Ok(entries) = std::fs::read_dir(&site) {
            for e in entries.flatten() {
                let p = e.path().join("site-packages");
                if p.is_dir()
                    && let Some(v) = scan_dist(&p, &prefix)
                {
                    return Some(v);
                }
            }
        }
    }
    None
}

fn scan_dist(dir: &Path, prefix: &str) -> Option<String> {
    let entries = std::fs::read_dir(dir).ok()?;
    for e in entries.flatten() {
        let name = e.file_name().to_string_lossy().to_string();
        if name.starts_with(prefix) && name.ends_with(".dist-info") {
            let rest = name[prefix.len()..].trim_end_matches(".dist-info");
            let ver = rest.split('-').next().unwrap_or(rest).to_string();
            if !ver.is_empty() {
                return Some(ver);
            }
        }
    }
    None
}

/// `cpython-3.12.16+20261026-x86_64-pc-windows-msvc-install_only.tar.gz` → "3.12.16"
fn parse_asset(name: &str, triple: &str) -> Option<String> {
    let rest = name.strip_prefix("cpython-")?;
    let (version, tail) = rest.split_once('+')?;
    let tail = tail.split_once('-')?.1;
    let tail = tail.strip_suffix("-install_only.tar.gz")?;
    if tail != triple {
        return None;
    }
    version
        .chars()
        .all(|c| c.is_ascii_digit() || c == '.')
        .then(|| version.to_string())
}

fn minor_line(version: &str) -> String {
    let mut parts = version.split('.');
    match (parts.next(), parts.next()) {
        (Some(a), Some(b)) => format!("{a}.{b}"),
        (Some(a), None) => a.to_string(),
        _ => String::new(),
    }
}

pub fn exec_rel() -> PathBuf {
    if cfg!(windows) {
        PathBuf::from("python.exe")
    } else {
        PathBuf::from("bin").join("python3")
    }
}

pub fn payload_markers() -> Vec<PathBuf> {
    vec![exec_rel()]
}

pub fn shim_specs() -> Vec<ShimSpec> {
    vec![
        ShimSpec { name: "python".into(), target: ShimTarget::Bin(exec_rel()) },
        ShimSpec { name: "python3".into(), target: ShimTarget::Bin(exec_rel()) },
        ShimSpec { name: "pip".into(), target: ShimTarget::Module("pip".into()) },
        ShimSpec { name: "pip3".into(), target: ShimTarget::Module("pip".into()) },
    ]
}

#[derive(Debug, Deserialize)]
struct Release {
    tag_name: String,
    published_at: String,
    assets: Vec<Asset>,
}

#[derive(Debug, Deserialize)]
struct Asset {
    name: String,
    #[serde(default)]
    size: u64,
    #[serde(default)]
    digest: Option<String>,
    browser_download_url: String,
}

#[cfg(test)]
mod tests {
    use super::parse_asset;

    #[test]
    fn 只认本机三元组的安装包() {
        let triple = "x86_64-pc-windows-msvc";
        assert_eq!(
            parse_asset("cpython-3.12.16+20261026-x86_64-pc-windows-msvc-install_only.tar.gz", triple).as_deref(),
            Some("3.12.16")
        );
        assert_eq!(
            parse_asset("cpython-3.12.16+20261026-aarch64-pc-windows-msvc-install_only.tar.gz", triple),
            None
        );
        assert_eq!(
            parse_asset("cpython-3.12.16+20261026-x86_64-pc-windows-msvc-install_only_stripped.tar.gz", triple),
            None
        );
    }
}
