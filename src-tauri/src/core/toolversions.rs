use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::error::{Error, Result};
use super::model::{ProjectLock, Resolution, ResolutionSource, ToolId};
use super::records::Registry;
use super::{paths, settings, util};

pub const TOOL_VERSIONS_FILE: &str = ".tool-versions";
/// 常见的项目版本提示文件，读得到就顺着用，避免让用户再写一遍。
pub const HINT_FILES: &[(ToolId, &str)] = &[
    (ToolId::Node, ".nvmrc"),
    (ToolId::Node, ".node-version"),
    (ToolId::Python, ".python-version"),
    (ToolId::Go, ".go-version"),
    (ToolId::PyPy, ".pypy-version"),
];

/// `.tool-versions` 与 asdf 一致：`node 20.18.0`，一行一个工具，多版本取第一个。
pub fn parse(text: &str) -> (BTreeMap<String, String>, Vec<String>) {
    let mut entries = BTreeMap::new();
    let mut extra = Vec::new();
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            extra.push(line.to_string());
            continue;
        }
        let mut parts = trimmed.split_whitespace();
        let key = parts.next().unwrap_or("").to_lowercase();
        let value = parts.next().unwrap_or("").to_string();
        if ToolId::from_key(&key).is_some() && !value.is_empty() {
            entries.insert(key, value);
        } else {
            extra.push(line.to_string());
        }
    }
    (entries, extra)
}

pub fn render(entries: &BTreeMap<String, String>, extra: &[String]) -> String {
    let mut out = String::new();
    out.push_str("# 由 灵匣·EnvSwitch 生成，进到这个目录时会自动用这里的版本\n");
    for line in extra {
        out.push_str(line);
        out.push('\n');
    }
    let mut keys: Vec<&String> = entries.keys().collect();
    keys.sort();
    for k in keys {
        out.push_str(&format!("{k} {}\n", entries[k]));
    }
    out
}

pub fn read_project(dir: &Path) -> Result<Option<ProjectLock>> {
    let file = dir.join(TOOL_VERSIONS_FILE);
    if !file.is_file() {
        return Ok(None);
    }
    let text = std::fs::read_to_string(&file)
        .map_err(|e| Error::Io(format!("读 {} 失败：{e}", file.display())))?;
    let (entries, extra_lines) = parse(&text);
    Ok(Some(ProjectLock {
        dir: dir.to_path_buf(),
        file,
        entries,
        extra_lines,
    }))
}

/// 在当前目录锁定版本；`versions` 里没提到的工具保持原样。
pub fn lock(dir: &Path, tool: ToolId, version: &str) -> Result<ProjectLock> {
    if !dir.is_dir() {
        return Err(Error::NotFound(format!(
            "{} 不是一个存在的目录",
            dir.display()
        )));
    }
    let mut lock = read_project(dir)?.unwrap_or_else(|| ProjectLock {
        dir: dir.to_path_buf(),
        file: dir.join(TOOL_VERSIONS_FILE),
        entries: BTreeMap::new(),
        extra_lines: Vec::new(),
    });
    lock.entries
        .insert(tool.as_str().to_string(), version.to_string());
    std::fs::write(&lock.file, render(&lock.entries, &lock.extra_lines))
        .map_err(|e| Error::Io(format!("写 {} 失败：{e}；这个目录可能不可写", lock.file.display())))?;
    Ok(lock)
}

pub fn unlock(dir: &Path, tool: ToolId) -> Result<Option<ProjectLock>> {
    let Some(mut lock) = read_project(dir)? else {
        return Ok(None);
    };
    lock.entries.remove(tool.as_str());
    if lock.entries.is_empty() {
        let _ = std::fs::remove_file(&lock.file);
        return Ok(None);
    }
    std::fs::write(&lock.file, render(&lock.entries, &lock.extra_lines))?;
    Ok(Some(lock))
}

fn read_hint(dir: &Path, tool: ToolId) -> Option<String> {
    HINT_FILES
        .iter()
        .filter(|(t, _)| *t == tool)
        .find_map(|(_, name)| {
            let p = dir.join(name);
            let text = std::fs::read_to_string(p).ok()?;
            let first = text.lines().map(str::trim).find(|l| !l.is_empty())?;
            Some(normalize_hint(first, tool))
        })
}

/// `.nvmrc` 里常写 "20"、"lts/iron"、"v20.18.0"，都要能落到可比较的形态。
fn normalize_hint(raw: &str, tool: ToolId) -> String {
    let mut s = raw.trim().trim_start_matches('v').to_string();
    if let Some(rest) = s.strip_prefix("lts/") {
        // 代号交给调用方按官方列表翻译，这里先保留原样标记。
        s = format!("lts:{rest}");
    }
    if s.is_empty() {
        s = String::new();
    }
    let _ = tool;
    s
}

/// 向上找最近的版本声明目录，和 shim 的行为保持一致。
pub fn find_declaration_dir(start: &Path) -> Option<PathBuf> {
    let mut dir = Some(start.to_path_buf());
    while let Some(d) = dir {
        if d.join(TOOL_VERSIONS_FILE).is_file() {
            return Some(d);
        }
        if HINT_FILES.iter().any(|(_, f)| d.join(f).is_file()) {
            return Some(d);
        }
        dir = d.parent().map(|p| p.to_path_buf());
    }
    None
}

pub fn resolve(tool: ToolId, cwd: &Path) -> Result<Resolution> {
    let settings = settings::Settings::load()?;
    let registry = Registry::load()?;
    Ok(resolve_with(&settings, &registry, tool, cwd))
}

pub fn resolve_with(
    settings: &settings::Settings,
    registry: &Registry,
    tool: ToolId,
    cwd: &Path,
) -> Resolution {
    let mut res = Resolution {
        tool,
        version: None,
        source: ResolutionSource::None,
        found_in: None,
        exec_path: None,
        note: None,
    };

    if settings.auto_switch && !settings.lock_global {
        let mut dir = Some(cwd.to_path_buf());
        while let Some(d) = dir {
            if let Ok(Some(lock)) = read_project(&d)
                && let Some(v) = lock.entries.get(tool.as_str())
            {
                res.version = Some(v.clone());
                res.source = ResolutionSource::ProjectFile;
                res.found_in = Some(d.clone());
                break;
            }
            if let Some(v) = read_hint(&d, tool) {
                let matched = match_hint(registry, tool, &v);
                if let Some(vv) = matched {
                    res.version = Some(vv);
                    res.source = ResolutionSource::ProjectFile;
                    res.found_in = Some(d.clone());
                    res.note = Some(format!("来自 {}，按已装版本对齐", d.join(".nvmrc").display()));
                    break;
                }
            }
            dir = d.parent().map(|p| p.to_path_buf());
        }
    } else if settings.lock_global {
        res.note = Some("全局锁定已开启，项目里的版本声明暂时不生效".to_string());
    }

    if res.version.is_none() {
        if let Some(v) = registry.default_of(tool) {
            res.version = Some(v);
            res.source = ResolutionSource::GlobalDefault;
        } else {
            let installed = registry.list(tool);
            if installed.len() == 1 {
                res.version = installed[0].version.clone().into();
                res.source = ResolutionSource::OnlyInstalled;
            }
        }
    }

    if let Some(v) = &res.version {
        match registry.find(tool, v) {
            Some(rec) if rec.usable() => res.exec_path = Some(rec.exec_path.clone()),
            Some(_) => {
                res.note = Some(format!("{v} 的文件不在原来的位置了，需要重新安装"));
                res.exec_path = None;
            }
            None => {
                res.note = Some(format!("{v} 还没装，先装再用"));
                res.version = None;
                res.source = ResolutionSource::None;
            }
        }
    }
    res
}

/// 提示文件里写 "20" 或 "lts/iron" 时，从已安装版本里挑最接近的那个。
fn match_hint(registry: &Registry, tool: ToolId, raw: &str) -> Option<String> {
    let installed = registry.list(tool);
    if installed.is_empty() {
        return None;
    }
    if raw.starts_with("lts:") {
        return installed
            .iter()
            .find(|r| r.version.contains(&raw[4..]))
            .or(installed.first())
            .map(|r| r.version.clone());
    }
    let want = raw.trim_start_matches('v');
    if let Some(exact) = installed.iter().find(|r| r.version == want) {
        return Some(exact.version.clone());
    }
    // 只写了主版本号：取同主线里最新的已装版本
    installed
        .iter()
        .find(|r| util::same_line(&r.version, want))
        .map(|r| r.version.clone())
}

/// 供"已绑定项目目录"展示：只认用户在界面上登记过的目录，不猜。
pub fn registered_projects() -> Result<Vec<PathBuf>> {
    let file = paths::layout()?.config_path("projects.json");
    Ok(util::read_json_or(&file, Vec::new)?)
}

#[derive(Serialize, Deserialize, Default)]
#[serde(transparent)]
struct Projects(Vec<PathBuf>);

pub fn register_project(dir: &Path) -> Result<()> {
    let file = paths::layout()?.config_path("projects.json");
    let mut list = Projects(util::read_json_or::<Vec<PathBuf>>(&file, Vec::new)?);
    let abs = util::canonical(dir);
    if !list.0.iter().any(|p| util::canonical(p) == abs) {
        list.0.push(abs);
    }
    util::write_json(&file, &list.0)
}

pub fn unregister_project(dir: &Path) -> Result<()> {
    let file = paths::layout()?.config_path("projects.json");
    let mut list = Projects(util::read_json_or::<Vec<PathBuf>>(&file, Vec::new)?);
    let abs = util::canonical(dir);
    list.0.retain(|p| util::canonical(p) != abs);
    util::write_json(&file, &list.0)
}

pub fn bound_projects(tool: ToolId, version: &str) -> Vec<PathBuf> {
    registered_projects()
        .unwrap_or_default()
        .into_iter()
        .filter_map(|d| {
            let lock = read_project(&d).ok()??;
            lock.entries
                .get(tool.as_str())
                .filter(|v| v.as_str() == version)
                .map(|_| d)
        })
        .collect()
}

/// 扫描数据目录里所有已装版本，把记录丢失或文件被手删的条目修正掉。
pub fn reconcile() -> Result<(usize, usize)> {
    let layout = paths::layout()?;
    let mut registry = Registry::load()?;
    let mut fixed = 0usize;
    let mut dropped = 0usize;
    for rec in registry.records.clone() {
        let expected = layout.version_dir(rec.tool.as_str(), &rec.version);
        if rec.dir != expected && expected.is_dir() {
            let mut r = rec.clone();
            r.dir = expected.clone();
            registry.put(r);
            fixed += 1;
        }
        if !rec.referenced && !rec.dir.is_dir() {
            registry.remove(rec.tool, &rec.version);
            dropped += 1;
        }
    }
    if fixed > 0 || dropped > 0 {
        registry.save()?;
    }
    Ok((fixed, dropped))
}
