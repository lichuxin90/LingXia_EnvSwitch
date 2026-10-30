use std::path::{Path, PathBuf};
use std::sync::Arc;

use super::archive;
use super::download;
use super::error::{Error, Result};
use super::job::Job;
use super::{mirror, paths, records::Record, records::Registry, settings::Settings, shim, tool::Plugin, toolversions, util};
use super::model::{
    AvailableVersion, InstalledVersion, InstallSource, JobEvent, JobPhase, ToolId, VersionLine,
};

/// 装一个版本：定位包 → 下载（分片/续传）→ 校验 → 解压 → 归位 → 登记 → 生成 shim。
/// 出错时把中间产物擦干净，不留半套版本。
pub async fn install(
    client: &reqwest::Client,
    job: Arc<Job>,
    on_event: Option<download::OnEvent>,
    tool: ToolId,
    version: &str,
    source_id: Option<&str>,
    force: bool,
) -> Result<InstalledVersion> {
    let settings = Settings::load()?;
    let layout = paths::layout()?;
    let registry = Registry::load()?;
    let plugin = Plugin::for_tool(tool)?;

    if let Some(existing) = registry.find(tool, version) {
        if existing.usable() && !force {
            return Err(Error::AlreadyInstalled(
                tool.display().to_string(),
                version.to_string(),
            ));
        }
    }
    emit(&on_event, &job, JobPhase::Planning, None, Some("在看哪个包适合这台电脑".to_string()));

    let src = mirror::resolve_source(client, &plugin, &settings, source_id).await?;
    let mut list = mirror::versions(client, &plugin, &src, false).await?;
    let mut avail = list.versions.iter().find(|v| v.version == version).cloned();
    // 落盘清单可能是旧的：要么没这个版本，要么当时没算出下载地址。强制刷新一次再判失败。
    let stale = match &avail {
        Some(a) => a.download_url.is_none(),
        None => true,
    };
    if stale {
        list = mirror::versions(client, &plugin, &src, true).await?;
        avail = list.versions.iter().find(|v| v.version == version).cloned();
    }
    let avail = avail.ok_or_else(|| {
        Error::NotFound(format!(
            "{} 的清单里找不到 {version}，点刷新列表再看看",
            tool.display()
        ))
    })?;
    if avail.download_url.is_none() {
        return Err(Error::Unsupported(format!(
            "官方没有为这台电脑发布 {version}（{}）",
            plugin.platform_triple()
        )));
    }

    let art = plugin.artifact(client, &src, &avail).await?;
    let expected_size = art.size_hint.or(avail.size_hint);
    emit(
        &on_event,
        &job,
        JobPhase::Planning,
        expected_size,
        Some(format!(
            "从「{}」下载 {}{}",
            src.label,
            art.file_name,
            expected_size
                .map(|s| format!("，约 {}", util::human_bytes(s)))
                .unwrap_or_default()
        )),
    );

    if art.sha256.is_none() {
        emit(
            &on_event,
            &job,
            JobPhase::Verifying,
            expected_size,
            Some("这个源没给校验值，只能装完自己核对文件是否完整".to_string()),
        );
    }

    let plan = download::Plan {
        url: art.url.clone(),
        file_name: art.file_name.clone(),
        dest: layout.archive_path(&art.file_name),
        partial: layout.partial_path(&art.file_name),
        expected_sha256: art.sha256.clone(),
        conns: settings.download_conns,
    };

    let archive_path = match download::run(
        client,
        plan,
        &job.id,
        Some(tool),
        Some(version.to_string()),
        on_event.clone(),
        Arc::clone(&job.cancel),
    )
    .await
    {
        Ok(p) => p,
        Err(e) => {
            // 校验失败说明包是坏的，缓存里那份不能留，否则下次继续用它。
            if matches!(e, Error::Verify { .. }) {
                let _ = tokio::fs::remove_file(&layout.archive_path(&art.file_name)).await;
                let _ = tokio::fs::remove_file(&layout.partial_path(&art.file_name)).await;
            }
            return Err(e);
        }
    };

    if job.is_cancelled() {
        return Err(Error::Cancelled);
    }

    let staging = layout.tmp.join(format!(
        "{}-{}-{}",
        tool.as_str(),
        util::safe_file_stem(version),
        util::timestamp()
    ));
    let source = if src.region == "国内" || src.region == "第三方" {
        InstallSource::Mirror
    } else {
        InstallSource::Official
    };
    let placed = {
        let archive = archive_path.clone();
        let staging = staging.clone();
        let version = version.to_string();
        let job = Arc::clone(&job);
        let events = on_event.clone();
        let sha = art.sha256.clone();
        tokio::task::spawn_blocking(move || {
            place_from_archive(
                &archive,
                &staging,
                tool,
                &version,
                sha,
                source,
                None,
                expected_size,
                &job,
                &events,
            )
        })
        .await
        .map_err(|e| Error::Detail(format!("解压线程挂了：{e}")))??
    };

    if !settings.keep_archives {
        let _ = std::fs::remove_file(&archive_path);
        download::prune_cache(&layout.cache, 7).await;
    }
    Ok(placed)
}

/// 解开放进 installs 并登记：下载后与离线导入共用这一段。
/// 同步实现，异步调用方用 spawn_blocking 包一层，别让解压堵住运行时。
pub fn place_from_archive(
    archive_path: &Path,
    staging: &Path,
    tool: ToolId,
    version: &str,
    sha256: Option<String>,
    source: InstallSource,
    origin: Option<PathBuf>,
    total: Option<u64>,
    job: &Job,
    on_event: &Option<download::OnEvent>,
) -> Result<InstalledVersion> {
    let layout = paths::layout()?;
    let plugin = Plugin::for_tool(tool)?;
    let mut registry = Registry::load()?;

    emit(on_event, job, JobPhase::Extracting, total, None);
    archive::extract(archive_path, staging)
        .map_err(|e| e.with_detail(format!("解压 {}", archive_path.display())))?;

    let payload_root = archive::locate_payload_root(staging, &plugin.payload_markers())
        .ok_or_else(|| {
            let _ = util::remove_tree(staging);
            Error::Detail(format!(
                "包解压出来了，但里面找不到 {} 的可执行文件，这个包可能不对版",
                tool.display()
            ))
        })?;

    let target = layout.version_dir(tool.as_str(), version);
    // 旧目录先改名让到 tmp 里，别就地删：删到一半撞见被占用的文件，旧版本就半残了
    // （新包还没放进去，用户手上只剩一个空壳）。改名失败说明它正被占用，
    // 这时候直接退出，新旧两边都一个字没动。
    let aside = if target.exists() {
        let stale = layout.tmp.join(format!(
            "retired-{}-{}-{}",
            tool.as_str(),
            util::safe_file_stem(version),
            util::timestamp()
        ));
        std::fs::rename(&target, &stale).map_err(|e| {
            let _ = util::remove_tree(staging);
            Error::Detail(format!(
                "旧版本目录挪不动（{e}），多半是里面的程序还在跑；先关掉再重装这一版"
            ))
        })?;
        Some(stale)
    } else {
        None
    };
    let restore_old = |install: &Path| {
        if let Some(s) = &aside {
            let _ = std::fs::rename(s, install);
        }
    };
    if let Err(e) = archive::move_payload(&payload_root, &target) {
        restore_old(&target);
        let _ = util::remove_tree(staging);
        return Err(e.with_detail("把解压好的文件挪进数据目录"));
    }
    let _ = util::remove_tree(staging);

    let exec_path = target.join(plugin.exec_rel());
    if !exec_path.is_file() {
        let _ = util::remove_tree(&target);
        restore_old(&target);
        return Err(Error::Detail(format!(
            "装完了却找不到 {}，这次安装算失败",
            exec_path.display()
        )));
    }
    // 新的一份已经就位，让位的旧目录只是垃圾，删不动也不影响使用。
    if let Some(s) = aside {
        let _ = util::remove_tree(&s);
    }
    make_executable(&exec_path);

    emit(on_event, job, JobPhase::Linking, total, None);
    let companions = plugin.companions(&target);
    let record = Record {
        tool,
        version: version.to_string(),
        dir: target.clone(),
        exec_path: exec_path.clone(),
        sha256,
        source,
        installed_at: chrono::Local::now().to_rfc3339(),
        origin,
        companions,
        size_bytes: util::dir_size(&target),
        referenced: false,
    };

    let first_of_tool = registry.list(tool).is_empty();
    registry.put(record);
    if registry.default_of(tool).is_none() || first_of_tool {
        registry.set_default(tool, version)?;
    }
    registry.save()?;
    // 新装的版本要让 shim 认得，用户才能在终端里直接敲 node。
    shim::ensure_tool_shims(tool)?;

    let note = if registry.default_of(tool).as_deref() == Some(version) {
        Some("已自动设为全局默认版本".to_string())
    } else {
        None
    };
    emit(on_event, job, JobPhase::Done, total, note);

    let registry = Registry::load()?;
    to_installed(&registry, tool, version).ok_or_else(|| {
        Error::Detail("安装记录写好了但读不回来，检查数据目录权限".to_string())
    })
}

#[cfg(unix)]
fn make_executable(path: &Path) {
    use std::os::unix::fs::PermissionsExt;
    if let Ok(meta) = std::fs::metadata(path) {
        let mut mode = meta.permissions().mode();
        mode |= 0o111;
        let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode));
    }
}

#[cfg(windows)]
fn make_executable(_path: &Path) {}

fn emit(
    on_event: &Option<download::OnEvent>,
    job: &Job,
    phase: JobPhase,
    total: Option<u64>,
    message: Option<String>,
) {
    if let Some(cb) = on_event {
        cb(&JobEvent {
            job_id: job.id.clone(),
            tool: Some(job.tool),
            version: Some(job.version.clone()),
            phase,
            received: 0,
            total,
            bytes_per_sec: 0,
            eta_secs: None,
            percent: None,
            message,
        });
    }
}

/// 卸载：接管来的版本只删记录，不动用户原来的文件。
pub fn uninstall(tool: ToolId, version: &str, purge: bool) -> Result<String> {
    let layout = paths::layout()?;
    let mut registry = Registry::load()?;
    let record = registry
        .find(tool, version)
        .cloned()
        .ok_or_else(|| Error::NotInstalled(tool.display().to_string(), version.to_string()))?;

    let mut freed = 0u64;
    if record.referenced || !purge {
        // 引用型版本的文件在用户自己装的位置，删了会破坏他的环境。
    } else {
        let path = if record.dir.is_dir() { record.dir.clone() } else { layout.version_dir(tool.as_str(), version) };
        let size = util::dir_size(&path);
        util::remove_tree_strict(&path).map_err(|e| e.with_detail(format!("删 {}", path.display())))?;
        freed = size;
    }

    registry.remove(tool, version);
    registry.save()?;

    // 顺手把该版本的缓存包也清掉，省磁盘。
    if purge {
        let mut deleted = Vec::new();
        if let Ok(entries) = std::fs::read_dir(&layout.cache) {
            for e in entries.flatten() {
                let p = e.path();
                let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("").to_string();
                if name.contains(version)
                    && (name.ends_with(".zip") || name.ends_with(".tar.gz"))
                {
                    let _ = std::fs::remove_file(&p);
                    deleted.push(name);
                }
            }
        }
        let _ = deleted;
    }

    if registry.list(tool).is_empty() {
        shim::remove_tool_shims(tool)?;
    } else {
        shim::ensure_tool_shims(tool)?;
    }
    Ok(util::human_bytes(freed))
}

/// 切换全局默认版本：只改配置，shim 本身不动，所以是零成本。
pub fn switch_default(tool: ToolId, version: &str) -> Result<Option<String>> {
    let mut registry = Registry::load()?;
    registry.set_default(tool, version)?;
    registry.save()?;
    shim::ensure_tool_shims(tool)?;
    Ok(registry.default_of(tool))
}

pub fn set_global_lock(locked: bool) -> Result<()> {
    let mut s = Settings::load()?;
    s.lock_global = locked;
    s.save()
}

pub fn installed(tool: ToolId) -> Result<Vec<InstalledVersion>> {
    let registry = Registry::load()?;
    let default = registry.default_of(tool);
    Ok(registry
        .list(tool)
        .into_iter()
        .map(|r| build_installed(&r, default.as_deref()))
        .collect())
}

fn to_installed(registry: &Registry, tool: ToolId, version: &str) -> Option<InstalledVersion> {
    let rec = registry.find(tool, version)?;
    Some(build_installed(rec, registry.default_of(tool).as_deref()))
}

fn build_installed(
    rec: &Record,
    default: Option<&str>,
) -> InstalledVersion {
    let is_default = default == Some(rec.version.as_str());
    InstalledVersion {
        tool: rec.tool,
        version: rec.version.clone(),
        dir: rec.dir.clone(),
        exec_path: rec.exec_path.clone(),
        size_bytes: if rec.size_bytes > 0 {
            rec.size_bytes
        } else {
            util::dir_size(&rec.dir)
        },
        installed_at: rec.installed_at.clone(),
        source: rec.source,
        sha256: rec.sha256.clone(),
        origin: rec.origin.clone(),
        companions: rec.companions.clone(),
        is_active: is_default,
        is_global_default: is_default,
        bound_projects: toolversions::bound_projects(rec.tool, &rec.version),
        usable: rec.exec_path.is_file(),
        unusable_reason: if rec.exec_path.is_file() {
            None
        } else {
            Some(format!("{} 不在了", rec.exec_path.display()))
        },
    }
}

pub fn group_lines(
    tool: ToolId,
    list: &[AvailableVersion],
    registry: &Registry,
) -> Vec<VersionLine> {
    let default = registry.default_of(tool);
    let default_line = default.as_deref().map(|d| crate::core::tool::line_key(tool, d));
    let mut by_line: Vec<(String, Vec<AvailableVersion>)> = Vec::new();
    for v in list {
        match by_line.iter_mut().find(|(k, _)| k == &v.line_key) {
            Some((_, items)) => items.push(v.clone()),
            None => by_line.push((v.line_key.clone(), vec![v.clone()])),
        }
    }
    let mut lines: Vec<VersionLine> = by_line
        .into_iter()
        .map(|(key, mut items)| {
            items.sort_by(|a, b| util::version_desc(&a.version, &b.version));
            let newest = items[0].clone();
            let installed_any = items.iter().any(|i| i.installed)
                || registry.list(tool).iter().any(|r| r.version == newest.version);
            let size = registry
                .list(tool)
                .into_iter()
                .filter(|r| crate::core::tool::in_line(tool, &r.version, &key))
                .map(|r| util::dir_size(&r.dir))
                .sum();
            VersionLine {
                tool,
                major: key.clone(),
                newest: newest.version.clone(),
                is_lts: newest.is_lts,
                lts_active: newest.lts_active,
                lts_codename: newest.lts_codename.clone(),
                eol: newest.eol,
                eol_date: newest.eol_date.clone(),
                released: newest.released.clone(),
                installed: installed_any,
                is_global_default: default_line.as_deref() == Some(key.as_str()),
                size_bytes: size,
                member_count: items.len(),
                members: items,
            }
        })
        .collect();
    lines.sort_by(|a, b| util::version_desc(&a.newest, &b.newest));
    lines
}
