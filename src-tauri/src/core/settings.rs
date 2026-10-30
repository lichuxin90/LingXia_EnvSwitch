use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::error::Result;
use super::model::{PathScope, ToolId};
use super::{paths, util};

pub const SETTINGS_FILE: &str = "settings.json";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ProxyMode {
    /// 跟随系统设置（默认，兼容 VPN/公司代理）
    System,
    /// 不走任何代理
    Direct,
    /// 手动填一个
    Custom,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Theme {
    System,
    Light,
    Dark,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Language {
    /// 跟随系统语言（默认）。
    Auto,
    /// 简体中文
    #[serde(rename = "zh-CN")]
    ZhCn,
    /// 繁体中文
    #[serde(rename = "zh-TW")]
    ZhTw,
    /// English
    En,
    /// 日本語
    Ja,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TrayBehavior {
    /// 关窗口就缩到托盘，后台常驻
    MinimizeToTray,
    /// 关窗口直接退出
    Quit,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    /// 每个工具用哪个下载源，值来自插件给出的源清单；"auto" 表示自动测速。
    pub sources: BTreeMap<String, String>,
    /// 手工指定的镜像根地址，优先级高于 sources 里的内置项。
    pub custom_base: BTreeMap<String, String>,
    pub proxy_mode: ProxyMode,
    pub proxy_url: Option<String>,
    pub autostart: bool,
    pub tray_behavior: TrayBehavior,
    pub path_scope: PathScope,
    /// shim 目录是否强制排在 PATH 最前面。关掉后可以在设置里一键让位。
    pub shim_priority: bool,
    /// 全局锁定：忽略项目里的 .tool-versions，一律用全局默认版本。
    pub lock_global: bool,
    /// 自动切换总开关；关掉后 shim 只做"转发到全局默认"。
    pub auto_switch: bool,
    /// 装完后是否保留压缩包。默认保留并自动清理 7 天前的旧包。
    pub keep_archives: bool,
    /// 分片下载的并发连接数，1 表示普通单连接下载。
    pub download_conns: usize,
    pub theme: Theme,
    /// 界面语言；只影响前端文案，Rust 侧的报错与脚本输出保持中文。
    pub language: Language,
    /// 界面记住的最后一个项目目录，用于"当前目录将使用 X"。
    pub last_project_dir: Option<String>,
    pub schema_version: u32,
}

impl Default for Settings {
    fn default() -> Self {
        let mut sources = BTreeMap::new();
        // 国内用户装完就能用：先测速，官方源和国内镜像谁快用谁。
        for tool in ToolId::ALL {
            sources.insert(tool.as_str().to_string(), "auto".to_string());
        }
        Self {
            sources,
            custom_base: BTreeMap::new(),
            proxy_mode: ProxyMode::System,
            proxy_url: None,
            autostart: false,
            tray_behavior: TrayBehavior::MinimizeToTray,
            path_scope: PathScope::User,
            shim_priority: true,
            lock_global: false,
            auto_switch: true,
            keep_archives: false,
            download_conns: 4,
            theme: Theme::System,
            language: Language::Auto,
            last_project_dir: None,
            schema_version: SETTINGS_SCHEMA,
        }
    }
}

const SETTINGS_SCHEMA: u32 = 1;

impl Settings {
    pub fn load() -> Result<Self> {
        let layout = paths::layout()?;
        let mut s = util::read_json_or(&layout.config_path(SETTINGS_FILE), Self::default)?;
        if s.sources.is_empty() {
            s.sources = Self::default().sources;
        }
        for tool in ToolId::ALL {
            s.sources
                .entry(tool.as_str().to_string())
                .or_insert_with(|| "auto".to_string());
        }
        Ok(s)
    }

    pub fn save(&self) -> Result<()> {
        let layout = paths::layout()?;
        util::write_json(&layout.config_path(SETTINGS_FILE), self)
    }

    pub fn source_for(&self, tool_key: &str) -> String {
        if let Some(base) = self.custom_base.get(tool_key)
            && !base.trim().is_empty()
        {
            return "custom".to_string();
        }
        self.sources
            .get(tool_key)
            .cloned()
            .unwrap_or_else(|| "auto".to_string())
    }

    pub fn set_source(&mut self, tool_key: &str, id: &str) {
        self.sources.insert(tool_key.to_string(), id.to_string());
    }
}
