use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::core::error::{Error, Result};
use crate::core::model::{AvailableVersion, Companion, ToolId};
use crate::core::util;

use super::{Artifact, ArchiveKind, ShimSpec, ShimTarget, SourceInfo, apply_wrap, eol};

/// Go 官方发布目录本身就是这套文件的根，清单是 `?mode=json`。
const OFFICIAL: &str = "https://go.dev/dl";
/// Google 在国内给的官方镜像，文件与 go.dev 一致，只是域名不同。
const CN: &str = "https://golang.google.cn/dl";

#[derive(Debug, Clone, Copy)]
pub struct GoPlugin;

impl GoPlugin {
    pub fn sources(&self) -> Vec<SourceInfo> {
        vec![
            SourceInfo {
                id: "official".into(),
                label: "官方 go.dev".into(),
                region: "海外",
                base: OFFICIAL.into(),
                wrap: None,
                note: None,
                probe_url: format!("{OFFICIAL}/?mode=json"),
            },
            SourceInfo {
                id: "google-cn".into(),
                label: "国内 golang.google.cn".into(),
                region: "国内",
                base: CN.into(),
                wrap: None,
                note: Some("Google 在国内的官方镜像，包内容与 go.dev 一致（按官方 sha256 核对）".into()),
                probe_url: format!("{CN}/?mode=json"),
            },
        ]
    }

    /// 清单 JSON 里 os/arch 的写法。
    fn platform() -> (&'static str, &'static str) {
        let arch = if cfg!(target_arch = "aarch64") { "arm64" } else { "amd64" };
        if cfg!(target_os = "windows") {
            ("windows", arch)
        } else if cfg!(target_os = "macos") {
            ("darwin", arch)
        } else {
            ("linux", arch)
        }
    }

    fn archive_ext() -> &'static str {
        if cfg!(target_os = "windows") { "zip" } else { "tar.gz" }
    }

    pub async fn list_versions(
        &self,
        client: &reqwest::Client,
        src: &SourceInfo,
    ) -> Result<Vec<AvailableVersion>> {
        let base = src.base.trim_end_matches('/');
        let url = format!("{base}/?mode=json&include=all");
        let resp = client
            .get(&url)
            .header("User-Agent", "envswitch/0.1")
            .send()
            .await
            .map_err(|e| Error::Network(format!("拿不到 Go 版本清单：{e}")))?;
        if !resp.status().is_success() {
            return Err(Error::NotFound(format!(
                "Go 发布目录返回了 {}，换个源试试",
                resp.status().as_u16()
            )));
        }
        let text = resp
            .text()
            .await
            .map_err(|e| Error::Network(format!("Go 版本清单读不完：{e}")))?;
        let entries: Vec<Entry> = serde_json::from_str(&text)
            .map_err(|e| Error::Config(format!("Go 版本清单格式看不懂：{e}")))?;

        let (os, arch) = Self::platform();
        let ext = Self::archive_ext();
        let installed: Vec<String> = crate::core::records::Registry::load()
            .map(|r| r.list(ToolId::Go).into_iter().map(|x| x.version).collect())
            .unwrap_or_default();
        let table = eol::schedule_for(client, "golang").await;

        let mut out: Vec<AvailableVersion> = Vec::new();
        for e in entries {
            // 预发布（rc/beta）不进列表：装它们的人知道自己要去官方找。
            if !e.stable {
                continue;
            }
            let version = e.version.trim_start_matches("go").to_string();
            if version.is_empty() {
                continue;
            }
            let line = line_of(&version);
            let hit = e.files.iter().find(|f| {
                f.os == os
                    && f.arch == arch
                    && f.filename.ends_with(ext)
                    && f.kind.as_deref() != Some("source")
            });
            let row = table.as_ref().and_then(|t| t.get(&line));
            let (eol_flag, eol_date, note) = eol::note_for(row);
            let (file_name, sha256, size_hint) = match hit {
                Some(f) => (
                    Some(f.filename.clone()),
                    f.sha256.clone(),
                    f.size.filter(|s| *s > 0),
                ),
                None => (None, None, None),
            };
            let download_url = file_name
                .as_ref()
                .map(|n| apply_wrap(src, &format!("{base}/{n}")));
            let unavailable_reason = if download_url.is_none() {
                Some("官方没有为这台电脑发布这个版本".to_string())
            } else {
                note
            };
            out.push(AvailableVersion {
                tool: ToolId::Go,
                line_key: line.clone(),
                version: version.clone(),
                released: None,
                // Go 没有 LTS 这条线，只维护最近两个小版本，EOL 标记才是关键信息。
                is_lts: false,
                lts_active: None,
                lts_codename: None,
                eol: eol_flag,
                eol_date,
                is_line_latest: false,
                line_count: 1,
                size_hint,
                installed: installed.iter().any(|v| v == &version),
                download_url,
                file_name,
                sha256,
                origin_tag: None,
                unavailable_reason,
            });
        }

        if out.is_empty() {
            return Err(Error::Detail(
                "Go 发布目录里没读到任何稳定版本，检查一下网络或换个源".to_string(),
            ));
        }

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
                .ok_or_else(|| Error::Detail("没算出该下哪个 Go 包".to_string()))?,
            file_name: v
                .file_name
                .clone()
                .ok_or_else(|| Error::Detail("包名丢了".to_string()))?,
            sha256: v.sha256.clone(),
            size_hint: v.size_hint,
            kind: if cfg!(target_os = "windows") { ArchiveKind::Zip } else { ArchiveKind::TarGz },
        })
    }

    /// Go 不附带包管理器，详情面板这块留空，但工具链本身的信息值得写一句。
    pub fn companions(&self, root: &Path) -> Vec<Companion> {
        let gofmt = root.join(bin_dir()).join("gofmt").with_extension(EXE);
        vec![
            Companion {
                name: "gofmt".into(),
                version: None,
                note: Some(if gofmt.is_file() {
                    "格式化工具随工具链一起装好".to_string()
                } else {
                    "这个包里没有 gofmt".to_string()
                }),
            },
            Companion {
                name: "模块缓存".into(),
                version: None,
                note: Some("依赖下载在 GOPATH/pkg/mod，与版本目录分开".to_string()),
            },
        ]
    }
}

#[cfg(windows)]
const EXE: &str = "exe";
#[cfg(not(windows))]
const EXE: &str = "";

fn bin_dir() -> PathBuf {
    PathBuf::from("bin")
}

pub fn exec_rel() -> PathBuf {
    bin_dir().join(if cfg!(windows) { "go.exe" } else { "go" })
}

pub fn payload_markers() -> Vec<PathBuf> {
    vec![exec_rel()]
}

pub fn shim_specs() -> Vec<ShimSpec> {
    let gofmt = bin_dir().join(if cfg!(windows) { "gofmt.exe" } else { "gofmt" });
    vec![
        ShimSpec { name: "go".into(), target: ShimTarget::Bin(exec_rel()) },
        ShimSpec { name: "gofmt".into(), target: ShimTarget::Bin(gofmt) },
    ]
}

/// 版本线按小版本：1.27.3 → 1.27。
pub fn line_of(version: &str) -> String {
    let mut parts = version.split('.');
    match (parts.next(), parts.next()) {
        (Some(a), Some(b)) => format!("{a}.{b}"),
        (Some(a), None) => a.to_string(),
        _ => String::new(),
    }
}

#[derive(Debug, Deserialize)]
struct Entry {
    version: String,
    #[serde(default)]
    stable: bool,
    #[serde(default)]
    files: Vec<FileEntry>,
}

#[derive(Debug, Deserialize)]
struct FileEntry {
    filename: String,
    #[serde(default)]
    os: String,
    #[serde(default)]
    arch: String,
    #[serde(default)]
    sha256: Option<String>,
    #[serde(default)]
    size: Option<u64>,
    #[serde(default)]
    kind: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::{Entry, FileEntry, line_of};

    #[test]
    fn 版本线按小版本取() {
        assert_eq!(line_of("1.27.3"), "1.27");
        assert_eq!(line_of("1.9"), "1.9");
        assert_eq!(line_of("2"), "2");
    }

    #[test]
    fn 只认官方清单那种写法() {
        let json = r#"{
            "version":"go1.27.3","stable":true,
            "files":[{"filename":"go1.27.3.windows-amd64.zip","os":"windows","arch":"amd64",
                      "sha256":"aa","size":123,"kind":"archive"},
                     {"filename":"go1.27.3.src.tar.gz","os":"","arch":"","kind":"source"}]
        }"#;
        let e: Entry = serde_json::from_str(json).expect("Go 清单条目");
        assert_eq!(e.version, "go1.27.3");
        assert!(e.stable);
        assert_eq!(e.files.len(), 2);
        assert_eq!(e.files[0].sha256.as_deref(), Some("aa"));
        assert_eq!(e.files[1].kind.as_deref(), Some("source"));
    }

    #[test]
    fn 旧写法里没有的字段不报错() {
        let e: FileEntry =
            serde_json::from_str(r#"{"filename":"go1.9.linux-amd64.tar.gz"}"#).expect("只给包名也能读");
        assert!(e.os.is_empty());
        assert!(e.sha256.is_none());
    }
}
