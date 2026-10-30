//! 配置导入导出：把源、代理、全局默认版本、项目锁定打包成一个 JSON，发给同事直接套用。
//!
//! 只搬配置不搬运行时本体：版本二进制太大，导入后由界面提示一键补齐。
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::error::{Error, Result};
use super::model::TransferSummary;
use super::records::Registry;
use super::settings::Settings;
use super::{paths, toolversions, util};

pub const BUNDLE_APP: &str = "envswitch-config";
pub const BUNDLE_SCHEMA: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Bundle {
    /// 固定成 envswitch-config，导入时靠它认文件，避免用户误选别的 JSON。
    pub app: String,
    pub schema: u32,
    pub exported_at: String,
    /// 导出时的系统与版本，仅作展示。
    pub from: String,
    pub settings: Settings,
    /// 每个工具的全局默认版本。
    pub defaults: BTreeMap<String, String>,
    /// 已装版本清单，导入方据此判断要不要补装。
    pub installs: Vec<BundleInstall>,
    /// 项目锁定：目录 + 各工具的版本声明。
    pub projects: Vec<BundleProject>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct BundleInstall {
    pub tool: String,
    pub version: String,
    /// 官方源 / 国内镜像 / 离线包 / 接管，便于同事知道来源。
    pub source: String,
    pub sha256: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct BundleProject {
    pub dir: String,
    pub entries: BTreeMap<String, String>,
}

impl Default for Bundle {
    fn default() -> Self {
        Self {
            app: BUNDLE_APP.to_string(),
            schema: BUNDLE_SCHEMA,
            exported_at: String::new(),
            from: String::new(),
            settings: Settings::default(),
            defaults: BTreeMap::new(),
            installs: Vec::new(),
            projects: Vec::new(),
        }
    }
}

/// 导出：当前配置 + 默认版本 + 已装清单 + 项目锁定。
pub fn export(dest: &Path) -> Result<TransferSummary> {
    let bundle = collect()?;
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let text = serde_json::to_string_pretty(&bundle)
        .map_err(|e| Error::Config(format!("配置打包失败：{e}")))?;
    std::fs::write(dest, text).map_err(|e| {
        Error::Permission(format!("写不进 {}：{e}", dest.display()))
    })?;
    Ok(summary(dest, &bundle))
}

fn collect() -> Result<Bundle> {
    let settings = Settings::load()?;
    let registry = Registry::load()?;
    let mut installs = Vec::new();
    for rec in &registry.records {
        installs.push(BundleInstall {
            tool: rec.tool.as_str().to_string(),
            version: rec.version.clone(),
            source: format!("{:?}", rec.source).to_lowercase(),
            sha256: rec.sha256.clone(),
        });
    }
    let mut projects = Vec::new();
    for dir in toolversions::registered_projects().unwrap_or_default() {
        if let Some(lock) = toolversions::read_project(&dir)? {
            projects.push(BundleProject {
                dir: dir.display().to_string(),
                entries: lock.entries,
            });
        }
    }
    Ok(Bundle {
        app: BUNDLE_APP.to_string(),
        schema: BUNDLE_SCHEMA,
        exported_at: chrono::Local::now().to_rfc3339(),
        from: format!(
            "EnvSwitch {} / {}",
            super::VERSION,
            std::env::consts::OS
        ),
        settings,
        defaults: registry.defaults.clone(),
        installs,
        projects,
    })
}

fn summary(file: &Path, bundle: &Bundle) -> TransferSummary {
    let mut tools = BTreeMap::new();
    for (k, v) in &bundle.defaults {
        tools.insert(k.clone(), v.clone());
    }
    TransferSummary {
        file: file.to_path_buf(),
        tools,
        project_count: bundle.projects.len(),
        installed_count: bundle.installs.len(),
    }
}

/// 导入结果的落点说明：哪些配置已经套用、哪些版本还缺。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportOutcome {
    pub summary: TransferSummary,
    /// 本机还没装的版本，界面提示一键补齐。
    pub missing: Vec<String>,
    /// 本机不存在的项目目录，跳过它们的锁定。
    pub skipped_projects: Vec<String>,
    pub note: String,
}

/// 导入：套用配置，登记项目，缺的版本列出来。
pub fn import(src: &Path, apply_settings: bool) -> Result<ImportOutcome> {
    let bundle = read_bundle(src)?;
    let mut registry = Registry::load()?;
    let mut missing = Vec::new();
    for item in &bundle.installs {
        let Some(tool) = super::model::ToolId::from_key(&item.tool) else {
            continue;
        };
        if registry.find(tool, &item.version).is_none() {
            missing.push(format!("{} {}", tool.display(), item.version));
        }
    }

    let mut skipped_projects = Vec::new();
    for p in &bundle.projects {
        let dir = PathBuf::from(&p.dir);
        if !dir.is_dir() {
            skipped_projects.push(p.dir.clone());
            continue;
        }
        for (key, version) in &p.entries {
            let Some(tool) = super::model::ToolId::from_key(key) else {
                continue;
            };
            // 只在这台电脑上真有这个版本时才写锁定，免得 shim 找不到人。
            if registry.find(tool, version).is_none() {
                continue;
            }
            toolversions::lock(&dir, tool, version)?;
        }
        toolversions::register_project(&dir)?;
    }

    // 全局默认只在本机装了那个版本时才照搬。
    for (key, version) in &bundle.defaults {
        let Some(tool) = super::model::ToolId::from_key(key) else {
            continue;
        };
        if registry.find(tool, version).is_some() {
            registry.set_default(tool, version)?;
        }
    }
    registry.save()?;

    if apply_settings {
        let mut s = bundle.settings.clone();
        // 数据目录位置和代理密码不该跟着文件跑，这里只搬与团队约定相关的项。
        s.last_project_dir = Settings::load().ok().and_then(|x| x.last_project_dir);
        s.save()?;
    }

    let mut summary = summary(src, &bundle);
    summary.tools = registry.defaults.clone();
    let note = if missing.is_empty() {
        "配置已套用，版本本机都齐了".to_string()
    } else {
        format!("配置已套用，还有 {} 个版本本机没装，可以一键补齐", missing.len())
    };
    Ok(ImportOutcome {
        summary,
        missing,
        skipped_projects,
        note,
    })
}

fn read_bundle(src: &Path) -> Result<Bundle> {
    let text = std::fs::read_to_string(src)
        .map_err(|e| Error::Io(format!("打不开 {}：{e}", src.display())))?;
    let bundle: Bundle = serde_json::from_str(&text)
        .map_err(|e| Error::Config(format!("这份文件不是 EnvSwitch 导出的配置：{e}")))?;
    if bundle.app != BUNDLE_APP {
        return Err(Error::Config(
            "文件里没有 EnvSwitch 的配置标记，怕是选错了文件".to_string(),
        ));
    }
    if bundle.schema > BUNDLE_SCHEMA {
        return Err(Error::Config(format!(
            "这份配置由更新版本的 EnvSwitch 导出（第 {} 版），请先升级软件",
            bundle.schema
        )));
    }
    Ok(bundle)
}

/// 导出文件名默认带日期，放在数据目录旁边的 downloads 里由界面决定；
/// 这里只给一个建议名。
pub fn suggested_name() -> String {
    format!("envswitch-config-{}.json", util::timestamp())
}

pub fn default_export_dir() -> PathBuf {
    paths::layout()
        .map(|l| l.root)
        .unwrap_or_else(|_| std::env::current_dir().unwrap_or_default())
}
