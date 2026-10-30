//! PyPy 是 Python 的附加分支，不在侧边栏单列，装完只在 Python 卡片下面多出一条线。
//!
//! 官方不发布 JSON 清单，只有 downloads.python.org/pypy/ 这一个目录页，
//! 文件名本身就带了全部信息：`pypy3.11-v8.0.1-win64.zip` = 兼容 CPython 3.11 的 8.0.1 版。

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::core::error::{Error, Result};
use crate::core::model::{AvailableVersion, Companion, ToolId};
use crate::core::util;

use super::{Artifact, ArchiveKind, ShimSpec, ShimTarget, SourceInfo, apply_wrap, eol, python};

const OFFICIAL: &str = "https://downloads.python.org/pypy";

#[derive(Debug, Clone, Copy)]
pub struct PyPyPlugin;

impl PyPyPlugin {
    pub fn sources(&self) -> Vec<SourceInfo> {
        vec![SourceInfo {
            id: "official".into(),
            label: "官方 downloads.python.org".into(),
            region: "海外",
            base: OFFICIAL.into(),
            wrap: None,
            note: Some(
                "PyPy 官方只发这一个地址，国内没有可信镜像；实在下不动就在设置里填自己的中转地址"
                    .to_string(),
            ),
            probe_url: format!("{OFFICIAL}/"),
        }]
    }

    /// 目录页里本机这套后缀。Windows 只有 win64 的 zip，其余平台是 tar 包。
    pub(crate) fn platform_token() -> &'static str {
        if cfg!(target_os = "windows") {
            "win64"
        } else if cfg!(target_os = "macos") {
            if cfg!(target_arch = "aarch64") {
                "macos_arm64"
            } else {
                "macos_x86_64"
            }
        } else if cfg!(target_arch = "aarch64") {
            "aarch64"
        } else {
            "linux64"
        }
    }

    pub async fn list_versions(
        &self,
        client: &reqwest::Client,
        src: &SourceInfo,
    ) -> Result<Vec<AvailableVersion>> {
        let base = src.base.trim_end_matches('/');
        let resp = client
            .get(format!("{base}/"))
            .header("User-Agent", "envswitch/0.1")
            .send()
            .await
            .map_err(|e| Error::Network(format!("拿不到 PyPy 版本清单：{e}")))?;
        if !resp.status().is_success() {
            return Err(Error::NotFound(format!(
                "PyPy 下载页返回了 {}，换个源或者稍后再试",
                resp.status().as_u16()
            )));
        }
        let text = resp
            .text()
            .await
            .map_err(|e| Error::Network(format!("PyPy 下载页读不完：{e}")))?;

        let token = Self::platform_token();
        let mut newest: HashMap<String, (String, String)> = HashMap::new();
        for name in hrefs(&text) {
            let Some((compat, release, file_name)) = parse_name(&name, token) else {
                continue;
            };
            // 同一条兼容线上留最新的发布版；旧的那些在 PyPy 这儿没有并列价值。
            let keep = match newest.get(&compat) {
                None => true,
                Some((best, _)) => util::version_desc(&release, best) == std::cmp::Ordering::Greater,
            };
            if keep {
                newest.insert(compat, (release, file_name));
            }
        }
        if newest.is_empty() {
            return Err(Error::Detail(
                "PyPy 下载页里没找到适合这台电脑的包".to_string(),
            ));
        }

        let installed: Vec<String> = crate::core::records::Registry::load()
            .map(|r| r.list(ToolId::PyPy).into_iter().map(|x| x.version).collect())
            .unwrap_or_default();
        // PyPy 自己没有时间表，按它兼容的 CPython 版本线借一份，至少能告诉用户这条线还值不值得用。
        let table = eol::schedule_for(client, "python").await;

        let mut out: Vec<AvailableVersion> = newest
            .into_iter()
            .map(|(compat, (release, file_name))| {
                let version = format!("{compat}-{release}");
                let row = table.as_ref().and_then(|t| t.get(&compat));
                let url = apply_wrap(src, &format!("{base}/{file_name}"));
                AvailableVersion {
                    tool: ToolId::PyPy,
                    line_key: compat.clone(),
                    installed: installed.iter().any(|v| v == &version),
                    download_url: Some(url),
                    file_name: Some(file_name),
                    version: version.clone(),
                    released: None,
                    is_lts: row.map(|r| r.lts).unwrap_or(false),
                    lts_active: None,
                    lts_codename: None,
                    eol: row.and_then(|r| r.eol),
                    eol_date: row.and_then(|r| r.eol_date.clone()),
                    size_hint: None,
                    sha256: None,
                    origin_tag: None,
                    unavailable_reason: row
                        .filter(|r| r.eol == Some(true))
                        .map(|_| format!("它兼容的 CPython {compat} 已经停止更新，新项目建议用 Python")),
                    is_line_latest: true,
                    line_count: 1,
                }
            })
            .collect();

        out.sort_by(|a, b| util::version_desc(&a.version, &b.version));
        Ok(out)
    }

    pub fn artifact(&self, _src: &SourceInfo, v: &AvailableVersion) -> Result<Artifact> {
        let file_name = v
            .file_name
            .clone()
            .ok_or_else(|| Error::Detail("包名丢了".to_string()))?;
        let kind = if file_name.ends_with(".zip") {
            ArchiveKind::Zip
        } else if file_name.ends_with(".tar.bz2") {
            ArchiveKind::TarBz2
        } else {
            ArchiveKind::TarGz
        };
        Ok(Artifact {
            url: v
                .download_url
                .clone()
                .ok_or_else(|| Error::Detail("没算出该下哪个 PyPy 包".to_string()))?,
            file_name,
            // 官方目录页不发布校验码，只能靠下载地址本身。
            sha256: None,
            size_hint: v.size_hint,
            kind,
        })
    }

    /// PyPy 自带 pip，版本从 site-packages 里的 dist-info 目录名读出来。
    pub fn companions(&self, root: &Path) -> Vec<Companion> {
        let found = python::find_dist_info(root, "pip");
        vec![
            Companion {
                name: "pip".into(),
                version: found.clone(),
                note: found.is_none().then(|| {
                    "这个 PyPy 里没找到 pip，用 pypy -m ensurepip 补一下".to_string()
                }),
            },
            Companion {
                name: "运行方式".into(),
                version: None,
                note: Some("在终端里用 pypy 或 pypy3 启动，python 仍归 Python 那边管".to_string()),
            },
        ]
    }
}

/// 从目录页里挑出同级包名。上级链接、图标、校验码文件都带路径或后缀，直接排除。
fn hrefs(html: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut at = 0;
    while let Some(i) = html[at..].find("href=\"") {
        let start = at + i + 6;
        let Some(j) = html[start..].find('"') else { break };
        let raw = &html[start..start + j];
        if raw.starts_with("pypy") && !raw.contains('/') && !raw.contains('?') {
            out.push(raw.to_string());
        }
        at = start + j;
    }
    out
}

/// `pypy3.11-v8.0.1-win64.zip` → ("3.11", "v8.0.1", 原文件名)。
/// 预发布（7.3.12rc1）和源码包会被挡掉：rc 只在明确要试新的时候才需要，源码包本机跑不了。
pub(crate) fn parse_name(name: &str, token: &str) -> Option<(String, String, String)> {
    let ext = if name.ends_with(".zip") {
        ".zip"
    } else if name.ends_with(".tar.bz2") {
        ".tar.bz2"
    } else if name.ends_with(".tar.gz") {
        ".tar.gz"
    } else {
        return None;
    };
    let stem = name.strip_suffix(ext)?;
    let stem = stem.strip_prefix("pypy")?;
    let (compat, tail) = stem.split_once("-v")?;
    if !compat.chars().all(|c| c.is_ascii_digit() || c == '.') || !compat.contains('.') {
        return None;
    }
    let release = tail.strip_suffix(&format!("-{token}"))?;
    if release.is_empty() || !release.chars().all(|c| c.is_ascii_digit() || c == '.') {
        return None;
    }
    if compat == "3.2" || compat == "2.7" {
        // 上古的 Python 2 分支，界面里列出来只会让人误装。
        return None;
    }
    Some((compat.to_string(), format!("v{release}"), name.to_string()))
}

pub fn exec_rel() -> PathBuf {
    if cfg!(windows) {
        PathBuf::from("pypy.exe")
    } else {
        PathBuf::from("bin").join("pypy3")
    }
}

pub fn payload_markers() -> Vec<PathBuf> {
    vec![exec_rel(), PathBuf::from(if cfg!(windows) { "python.exe" } else { "bin/pypy" })]
}

/// 只认领 pypy 这两个命令。python/python3 归 Python 插件，抢过来会把用户的默认解释器换掉。
pub fn shim_specs() -> Vec<ShimSpec> {
    let target = exec_rel();
    vec![
        ShimSpec { name: "pypy".into(), target: ShimTarget::Bin(target.clone()) },
        ShimSpec { name: "pypy3".into(), target: ShimTarget::Bin(target) },
    ]
}

/// 版本线按兼容的 CPython 小版本：3.11-v8.0.1 → 3.11。
pub fn line_of(version: &str) -> String {
    version.split('-').next().unwrap_or(version).to_string()
}

#[cfg(test)]
mod tests {
    use super::{hrefs, line_of, parse_name};

    #[test]
    fn 包名读得出版本线() {
        assert_eq!(
            parse_name("pypy3.11-v8.0.1-win64.zip", "win64"),
            Some(("3.11".into(), "v8.0.1".into(), "pypy3.11-v8.0.1-win64.zip".into()))
        );
        // 老的 Linux 包是 tar.bz2，后缀不同但同样能读。
        assert_eq!(
            parse_name("pypy3.10-v7.3.19-linux64.tar.bz2", "linux64").map(|(c, _, _)| c),
            Some("3.10".to_string())
        );
    }

    #[test]
    fn 预发布与别的平台不收() {
        assert_eq!(parse_name("pypy3.10-v7.3.12rc1-win64.zip", "win64"), None);
        assert_eq!(parse_name("pypy3.11-v8.0.1-macos_arm64.tar.gz", "win64"), None);
        assert_eq!(parse_name("pypy3.11-v8.0.1-src.zip", "win64"), None);
        assert_eq!(parse_name("pypy2.7-v7.3.16-win64.zip", "win64"), None);
        assert_eq!(parse_name("pypy3.11-v8.0.1-win64.zip.sha256", "win64"), None);
    }

    #[test]
    fn 版本线取横杠前面那段() {
        assert_eq!(line_of("3.11-v8.0.1"), "3.11");
        assert_eq!(line_of("3.12-v8.0.0"), "3.12");
    }

    #[test]
    fn 目录页只收同级包名() {
        let html = r#"<a href="../">..</a><a href="/icons/file.png">i</a>
            <a href="pypy3.11-v8.0.1-win64.zip">z</a><a href="pypy3.11-v8.0.1-win64.zip.sha256">s</a>"#;
        assert_eq!(
            hrefs(html),
            vec!["pypy3.11-v8.0.1-win64.zip", "pypy3.11-v8.0.1-win64.zip.sha256"]
        );
    }
}
