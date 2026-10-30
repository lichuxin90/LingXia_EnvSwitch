use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use super::error::{Error, Result};
use super::model::{Companion, InstallSource, ToolId};

use super::{paths, util};

pub const REGISTRY_FILE: &str = "installs.json";

/// 一条已安装记录。数据目录被搬走或版本目录被手删时，`verify()` 会把它标成不可用。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Record {
    pub tool: ToolId,
    pub version: String,
    pub dir: PathBuf,
    pub exec_path: PathBuf,
    pub sha256: Option<String>,
    pub source: InstallSource,
    pub installed_at: String,
    pub origin: Option<PathBuf>,
    pub companions: Vec<Companion>,
    pub size_bytes: u64,
    /// 接管来的版本只引用原目录，卸载时不能真删用户文件。
    pub referenced: bool,
}

impl Default for Record {
    fn default() -> Self {
        Self {
            tool: ToolId::Node,
            version: String::new(),
            dir: PathBuf::new(),
            exec_path: PathBuf::new(),
            sha256: None,
            source: InstallSource::Official,
            installed_at: String::new(),
            origin: None,
            companions: Vec::new(),
            size_bytes: 0,
            referenced: false,
        }
    }
}

impl Record {
    pub fn usable(&self) -> bool {
        self.exec_path.is_file()
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Registry {
    pub records: Vec<Record>,
    /// 每个工具的全局默认版本，也就是 shim 在项目之外使用的版本。
    pub defaults: BTreeMap<String, String>,
}

impl Registry {
    pub fn load() -> Result<Self> {
        let layout = paths::layout()?;
        util::read_json_or(&layout.config_path(REGISTRY_FILE), Self::default)
    }

    pub fn save(&self) -> Result<()> {
        let layout = paths::layout()?;
        util::write_json(&layout.config_path(REGISTRY_FILE), self)
    }

    pub fn find(&self, tool: ToolId, version: &str) -> Option<&Record> {
        self.records
            .iter()
            .find(|r| r.tool == tool && r.version == version)
    }

    pub fn list(&self, tool: ToolId) -> Vec<Record> {
        let mut v: Vec<Record> = self.records.iter().filter(|r| r.tool == tool).cloned().collect();
        v.sort_by(|a, b| util::version_desc(&a.version, &b.version));
        v
    }

    pub fn default_of(&self, tool: ToolId) -> Option<String> {
        self.defaults.get(tool.as_str()).filter(|s| !s.is_empty()).cloned()
    }

    /// 设置全局默认版本；版本没装过就直接报错，避免 shim 指到空路径。
    pub fn set_default(&mut self, tool: ToolId, version: &str) -> Result<()> {
        if self.find(tool, version).is_none() {
            return Err(Error::NotInstalled(tool.display().to_string(), version.to_string()));
        }
        self.defaults
            .insert(tool.as_str().to_string(), version.to_string());
        Ok(())
    }

    pub fn clear_default(&mut self, tool: ToolId) {
        self.defaults.remove(tool.as_str());
    }

    pub fn put(&mut self, rec: Record) {
        self.records.retain(|r| !(r.tool == rec.tool && r.version == rec.version));
        self.records.push(rec);
        self.records.sort_by(|a, b| {
            a.tool.as_str().cmp(b.tool.as_str()).then_with(|| {
                util::version_desc(&a.version, &b.version)
            })
        });
    }

    pub fn remove(&mut self, tool: ToolId, version: &str) -> Option<Record> {
        let idx = self
            .records
            .iter()
            .position(|r| r.tool == tool && r.version == version)?;
        let rec = self.records.remove(idx);
        if self.default_of(tool).as_deref() == Some(version) {
            self.clear_default(tool);
        }
        Some(rec)
    }
}
