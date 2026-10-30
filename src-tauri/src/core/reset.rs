//! 一键重置：把 EnvSwitch 加到系统里的东西收回去，让电脑回到没装过它的样子。
//!
//! 顺序是先停转发（shim）、再改 PATH、最后摘终端集成；版本目录默认保留，
//! 免得用户手滑把已经装好的运行时一起删掉。
use super::error::Result;
use super::model::{PathScope, ResetOutcome};
use super::records::Registry;
use super::settings::Settings;
use super::{hook, paths, pathmod, shim, util};

/// keep_installs=false 时连 downloads 里的版本目录一起清掉。
pub fn reset(keep_installs: bool) -> Result<ResetOutcome> {
    // 数据目录都没选好就谈不上重置，先让首次设置把根定下来。
    paths::layout()?;
    let scope = Settings::load().map(|s| s.path_scope).unwrap_or(PathScope::User);
    let mut failed: Vec<String> = Vec::new();

    // 先删转发文件：PATH 里还留着目录也没关系，里面空了就不再接管命令。
    let shims_removed = shim::wipe().unwrap_or_else(|e| {
        failed.push(format!("转发文件没清干净：{e}"));
        0
    });

    // 备份里 newest-first 地找：只有改动**之前**留的那份才干净，具体挑法见 withdraw_via_backup。
    // 一份干净的都没有（或干净的那份就是现状），就只摘自己加的那一段，别拿脏备份覆盖用户现在的环境。
    let mut path_restored = None;
    match pathmod::withdraw_via_backup(scope) {
        Ok(Some(id)) => path_restored = Some(id),
        Ok(None) => {}
        Err(e) => failed.push(format!("按备份恢复 PATH 失败：{e}")),
    }
    if path_restored.is_none() {
        match pathmod::withdraw(scope) {
            Ok(_) => path_restored = Some(format!("摘掉了 {} 里由 EnvSwitch 加的那一段", scope.label())),
            Err(e) => failed.push(format!("PATH 没能改回去：{e}")),
        }
    }
    if path_restored.is_none() {
        match pathmod::withdraw(scope) {
            Ok(_) => path_restored = Some(format!("摘掉了 {} 里由 EnvSwitch 加的那一段", scope.label())),
            Err(e) => failed.push(format!("PATH 没能改回去：{e}")),
        }
    }

    let hooks_removed = hook::remove_all().unwrap_or_else(|e| {
        failed.push(format!("终端集成没摘干净：{e}"));
        Vec::new()
    });
    hook::delete_scripts();

    let registry = Registry::load().unwrap_or_default();
    let (installs_kept, installs_removed) = if keep_installs {
        (registry.records.len(), 0)
    } else {
        let kept = 0;
        let mut removed = 0usize;
        for rec in &registry.records {
            if rec.referenced {
                // 引用型版本的文件在用户自己装的位置，重置不碰。
                continue;
            }
            if rec.dir.is_dir() {
                let (n, errs) = util::remove_tree(&rec.dir);
                removed += if n > 0 { 1 } else { 0 };
                for e in errs {
                    failed.push(format!("删不掉 {e}"));
                }
            }
        }
        (kept, removed)
    };

    Ok(ResetOutcome {
        shims_removed,
        path_restored,
        path_scope: scope,
        installs_kept,
        installs_removed,
        hooks_removed,
        failed,
    })
}

/// 重置前给界面看的清单：会动到哪些东西，让用户心里有数。
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Preview {
    pub shim_dir: std::path::PathBuf,
    pub shims: usize,
    pub scope: PathScope,
    pub backup_count: usize,
    pub hooks: Vec<String>,
    pub installed_count: usize,
    pub referenced_count: usize,
    pub installs_bytes: u64,
    pub note: String,
}

pub fn preview() -> Result<Preview> {
    let layout = paths::layout()?;
    let scope = Settings::load().map(|s| s.path_scope).unwrap_or(PathScope::User);
    let registry = Registry::load()?;
    let shims = shim::list()?.iter().filter(|s| s.exists).count();
    let hooks = hook::status()?
        .iter()
        .filter(|h| h.installed)
        .map(|h| h.label.to_string())
        .collect();
    let installs_bytes = registry
        .records
        .iter()
        .filter(|r| !r.referenced)
        .map(|r| util::dir_size(&r.dir))
        .sum();
    Ok(Preview {
        shim_dir: layout.shims,
        shims,
        scope,
        backup_count: pathmod::list_backups()
            .map(|v| v.into_iter().filter(|b| b.scope == scope).count())
            .unwrap_or(0),
        hooks,
        installed_count: registry.records.len(),
        referenced_count: registry.records.iter().filter(|r| r.referenced).count(),
        installs_bytes,
        note: "重置只收走 EnvSwitch 加到系统里的部分；接管来的版本只删记录，不碰原目录".to_string(),
    })
}
