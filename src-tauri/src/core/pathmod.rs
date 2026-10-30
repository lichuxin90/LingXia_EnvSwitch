use std::path::Path;

use serde::{Deserialize, Serialize};

use super::error::{Error, Result};
use super::model::{BackupKind, PathBackup, PathScope, PathStatus};
use super::{paths, settings, shim, util};

const ENV_KEY_USER: &str = "Environment";
const ENV_KEY_SYSTEM: &str = r"SYSTEM\CurrentControlSet\Control\Session Manager\Environment";
const BACKUP_PREFIX: &str = "path-";
const MAX_BACKUPS: usize = 20;

/// 改动前留下的完整快照，回滚靠它。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BackupFile {
    pub id: String,
    pub created_at: String,
    pub scope: PathScope,
    pub raw: String,
    /// 原值的类型，回滚时按原样写回，不把 %USERPROFILE% 之类写死。
    pub expand: bool,
}

pub fn is_elevated() -> bool {
    #[cfg(windows)]
    {
        unsafe { windows_sys::Win32::UI::Shell::IsUserAnAdmin() != 0 }
    }
    #[cfg(not(windows))]
    {
        super::unix::is_root()
    }
}

pub fn status(scope: PathScope) -> Result<PathStatus> {
    let layout = paths::layout()?;
    let raw = read_raw(scope)?;
    let entries = util::split_path_list(&raw);
    let found = entries
        .iter()
        .position(|e| paths::norm_eq(Path::new(e), &layout.shims));
    // Windows 给新终端的顺序是"整台电脑"在前、"当前用户"在后，所以用户档里的第 1 位
    // 并不是真正的第 1 位。名次要按合并后的顺序报，否则界面会谎报"已经排第一"。
    let lead = match scope {
        PathScope::User => util::split_path_list(&read_raw(PathScope::System).unwrap_or_default()).len(),
        PathScope::System => 0,
    };
    let position = found.map(|i| i + lead);
    let backups = list_backups()?
        .into_iter()
        .filter(|b| b.scope == scope)
        .collect::<Vec<_>>();
    let note = match scope {
        PathScope::System if !is_elevated() && cfg!(windows) => {
            Some("改整台电脑的 PATH 需要你点一次授权".to_string())
        }
        PathScope::System if found.is_some() && found != Some(0) => {
            Some("转发目录在 PATH 里但没排在最前，别的工具可能抢先".to_string())
        }
        PathScope::User if found.is_some() && position != Some(0) => Some(format!(
            "整台电脑的 PATH 里有 {lead} 个目录排在前面，终端里可能先撞上别的版本；要抢第一请把生效范围改成整台电脑"
        )),
        _ => None,
    };
    Ok(PathStatus {
        scope,
        shim_dir: layout.shims.clone(),
        installed: found.is_some(),
        is_first: position == Some(0),
        position,
        entries,
        backup_count: backups.len(),
        latest_backup: backups.first().map(|b| b.created_at.clone()),
        elevated: is_elevated(),
        note,
    })
}

pub fn read_raw(scope: PathScope) -> Result<String> {
    #[cfg(windows)]
    {
        read_registry(scope).map(|(raw, _)| raw)
    }
    #[cfg(not(windows))]
    {
        Ok(super::unix::read_path(scope)?)
    }
}

/// 把 shim 目录放进 PATH，默认排最前。改之前一定先备份原值。
pub fn apply(scope: PathScope) -> Result<PathStatus> {
    let layout = paths::layout()?;
    let cfg = settings::Settings::load()?;
    let priority = cfg.shim_priority;
    let raw = read_raw(scope)?;
    let new_raw = merged(&raw, &layout.shims, priority);
    write(scope, &raw, &new_raw)?;
    adopt_own_process(scope, &new_raw);
    status(scope)
}

/// 只把 shim 目录从 PATH 里摘掉，其他条目原样保留。
pub fn withdraw(scope: PathScope) -> Result<PathStatus> {
    let layout = paths::layout()?;
    let raw = read_raw(scope)?;
    let new_raw = removed(&raw, &layout.shims);
    if new_raw == raw {
        return status(scope);
    }
    write(scope, &raw, &new_raw)?;
    adopt_own_process(scope, &new_raw);
    status(scope)
}

/// 在原值开头或结尾加上 shim 目录，其余一个字都不动。
///
/// 这里刻意不按"清洗过的条目表"重建 PATH：本机系统 PATH 里有空条目和结尾的分号，
/// 重建会把它们吞掉，用户撤掉优先级后环境就悄悄变了。空条目原样留着，
/// 摘除时才可能逐字还原成改之前的样子。
fn merged(raw: &str, shim_dir: &Path, priority: bool) -> String {
    let head = shim_dir.to_string_lossy().to_string();
    let mut kept = keep_others(raw, shim_dir);
    if kept.iter().all(String::is_empty) {
        return head;
    }
    if priority {
        kept.insert(0, head);
    } else {
        kept.push(head);
    }
    kept.join(&util::path_list_sep().to_string())
}

/// 从原值里只拿掉 shim 目录那一条，其他条目连同空条目一起保留。
fn removed(raw: &str, shim_dir: &Path) -> String {
    let kept = keep_others(raw, shim_dir);
    if kept.iter().all(String::is_empty) {
        return String::new();
    }
    kept.join(&util::path_list_sep().to_string())
}

fn keep_others(raw: &str, shim_dir: &Path) -> Vec<String> {
    raw.split(util::path_list_sep())
        .map(str::to_string)
        .filter(|e| !is_shim_entry(e, shim_dir))
        .collect()
}

fn is_shim_entry(entry: &str, shim_dir: &Path) -> bool {
    let trimmed = entry.trim().trim_matches('"');
    !trimmed.is_empty() && paths::norm_eq(Path::new(trimmed), shim_dir)
}

/// 某个目录在这份 PATH 里的名次（从 1 开始），不在里面返回 None。
pub fn rank_in(raw: &str, dir: &Path) -> Option<usize> {
    util::split_path_list(raw)
        .iter()
        .position(|e| paths::norm_eq(Path::new(e.trim_matches('"')), dir))
        .map(|i| i + 1)
}

/// 置顶/让位开关变了以后重排一次。
pub fn reorder(scope: PathScope) -> Result<PathStatus> {
    apply(scope)
}

fn write(scope: PathScope, old_raw: &str, new_raw: &str) -> Result<()> {
    if old_raw != new_raw {
        backup(scope, old_raw)?;
    }
    write_now(scope, new_raw)
}

#[cfg(windows)]
fn write_now(scope: PathScope, raw: &str) -> Result<()> {
    // 回写保持原值类型，别把 REG_SZ 悄悄升成 REG_EXPAND_SZ（或反过来）。
    let expand = read_registry(scope).map(|(_, e)| e).unwrap_or(true);
    match scope {
        PathScope::User => {
            use winreg::enums::{HKEY_CURRENT_USER, KEY_READ, KEY_SET_VALUE};
            use winreg::RegKey;
            let root = RegKey::predef(HKEY_CURRENT_USER);
            let key = root
                .open_subkey_with_flags(ENV_KEY_USER, KEY_READ | KEY_SET_VALUE)
                .or_else(|_| root.create_subkey(ENV_KEY_USER).map(|(k, _)| k))
                .map_err(|e| Error::Io(format!("打不开当前用户的环境变量：{e}")))?;
            set_reg(&key, raw, expand)?;
        }
        PathScope::System => {
            if is_elevated() {
                use winreg::enums::{HKEY_LOCAL_MACHINE, KEY_READ, KEY_SET_VALUE};
                use winreg::RegKey;
                let key = RegKey::predef(HKEY_LOCAL_MACHINE)
                    .open_subkey_with_flags(ENV_KEY_SYSTEM, KEY_READ | KEY_SET_VALUE)
                    .map_err(|e| Error::Permission(format!("打不开系统环境变量：{e}")))?;
                set_reg(&key, raw, expand)?;
            } else {
                return raise_and_write(raw);
            }
        }
    }
    broadcast();
    Ok(())
}

#[cfg(windows)]
fn set_reg(
    key: &winreg::RegKey,
    raw: &str,
    expand: bool,
) -> std::result::Result<(), Error> {
    use winreg::enums::{REG_EXPAND_SZ, REG_SZ};
    use winreg::types::ToRegValue;
    let mut value = raw.to_reg_value();
    value.vtype = if expand { REG_EXPAND_SZ } else { REG_SZ };
    key.set_raw_value("Path", &value)
        .map_err(|e| Error::Io(format!("写 PATH 失败：{e}")))
}

/// Windows 的 PATH 习惯存成 REG_EXPAND_SZ，回写时保持原类型，
/// 否则 %SystemRoot% 这类引用会被固化，别人的机器搬过来就废了。
#[cfg(windows)]
fn read_registry(scope: PathScope) -> Result<(String, bool)> {
    use winreg::enums::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, KEY_READ};
    use winreg::RegKey;
    let (root, sub) = match scope {
        PathScope::User => (RegKey::predef(HKEY_CURRENT_USER), ENV_KEY_USER),
        PathScope::System => (RegKey::predef(HKEY_LOCAL_MACHINE), ENV_KEY_SYSTEM),
    };
    let key = root
        .open_subkey_with_flags(sub, KEY_READ)
        .map_err(|e| Error::Io(format!("读不到环境变量位置：{e}")))?;
    let value = key
        .get_raw_value("Path")
        .map_err(|e| Error::NotFound(format!("这里还没有 PATH 记录：{e}")))?;
    let raw = String::from_utf16_lossy(
        &value
            .bytes
            .chunks(2)
            .filter_map(|c| if c.len() == 2 { Some(u16::from_le_bytes([c[0], c[1]])) } else { None })
            .collect::<Vec<u16>>(),
    )
    .trim_end_matches('\0')
    .to_string();
    Ok((raw, value.vtype == winreg::enums::RegType::REG_EXPAND_SZ))
}

/// 系统级 PATH 要管理员：让自己那个控制台小程序提权去写，写完轮询确认。
#[cfg(windows)]
fn raise_and_write(raw: &str) -> Result<()> {
    let layout = paths::layout()?;
    let bin = shim::shim_bin()?;
    let payload = layout.tmp.join(format!("path-apply-{}.json", util::timestamp()));
    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct Payload<'a> {
        raw: &'a str,
        expand: bool,
    }
    util::write_json(
        &payload,
        &Payload { raw, expand: true },
    )?;

    let args = format!("path-apply \"{}\"", payload.display());
    if !request_elevation(&bin, &args) {
        return Err(Error::Permission(
            "授权窗口被关掉了，系统 PATH 没有改动".to_string(),
        ));
    }
    // 等提权进程落地；用户在 UAC 里点否就永远等不到。
    for _ in 0..60 {
        std::thread::sleep(std::time::Duration::from_millis(500));
        if let Ok((current, _)) = read_registry(PathScope::System) {
            if current == raw {
                let _ = std::fs::remove_file(&payload);
                broadcast();
                return Ok(());
            }
        }
    }
    let _ = std::fs::remove_file(&payload);
    Err(Error::Permission(
        "等了半分钟系统 PATH 还没变，多半是授权没点".to_string(),
    ))
}

#[cfg(windows)]
pub fn request_elevation(exe: &Path, args: &str) -> bool {
    use windows_sys::Win32::UI::Shell::ShellExecuteW;
    use windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOW;
    let exe_w = to_wide(&exe.to_string_lossy());
    let args_w = to_wide(args);
    let verb_w = to_wide("runas");
    let ret = unsafe {
        ShellExecuteW(
            std::ptr::null_mut(),
            verb_w.as_ptr(),
            exe_w.as_ptr(),
            args_w.as_ptr(),
            std::ptr::null(),
            SW_SHOW,
        )
    };
    ret as isize > 32
}

#[cfg(windows)]
pub fn broadcast() {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        HWND_BROADCAST, WM_SETTINGCHANGE, SendMessageTimeoutW, SMTO_ABORTIFHUNG,
    };
    let term = to_wide("Environment");
    unsafe {
        let mut result = 0usize;
        SendMessageTimeoutW(
            HWND_BROADCAST,
            WM_SETTINGCHANGE,
            0,
            term.as_ptr() as isize,
            SMTO_ABORTIFHUNG,
            3000,
            &mut result,
        );
    }
}

#[cfg(windows)]
fn to_wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// 供 CLI 提权分支调用：把 JSON 里的值写进系统 PATH。
pub fn apply_from_file(path: &Path) -> Result<()> {
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Payload {
        raw: String,
        #[serde(default = "default_true")]
        expand: bool,
    }
    fn default_true() -> bool {
        true
    }
    let text = std::fs::read_to_string(path)
        .map_err(|e| Error::Io(format!("读不到要写入的 PATH 文件：{e}")))?;
    let payload: Payload = serde_json::from_str(&text)?;
    #[cfg(windows)]
    {
        use winreg::enums::{HKEY_LOCAL_MACHINE, KEY_READ, KEY_WRITE};
        use winreg::RegKey;
        let key = RegKey::predef(HKEY_LOCAL_MACHINE)
            .open_subkey_with_flags(ENV_KEY_SYSTEM, KEY_READ | KEY_WRITE)
            .map_err(|e| {
                Error::Permission(format!("这次运行没有管理员权限，写不了系统 PATH：{e}"))
            })?;
        set_reg(&key, &payload.raw, payload.expand)?;
        broadcast();
    }
    #[cfg(not(windows))]
    {
        let _ = payload;
        return Err(Error::Unsupported("这台机器不用提权写 PATH".to_string()));
    }
    Ok(())
}

#[cfg(not(windows))]
fn write_now(scope: PathScope, raw: &str) -> Result<()> {
    super::unix::write_path(scope, raw)
}

/// 让本进程（以及它马上要拉起的终端）立刻用上新 PATH。
fn adopt_own_process(scope: PathScope, new_raw: &str) {
    // SAFETY: 命令都跑在 blocking 线程池上，改自己的环境变量没有并发读者。
    unsafe { std::env::set_var("PATH", effective_raw(scope, new_raw)) };
}

/// 进程真正拿到的 PATH 是两段拼起来的。只把改动的那一段塞回去会把另一段整个丢掉，
/// 那时连 powershell.exe、git 都找不到，环境体检会误报"这台电脑上没找到"。
fn effective_raw(scope: PathScope, new_raw: &str) -> String {
    #[cfg(windows)]
    {
        let other = match scope {
            PathScope::System => read_raw(PathScope::User).unwrap_or_default(),
            PathScope::User => read_raw(PathScope::System).unwrap_or_default(),
        };
        let sep = util::path_list_sep().to_string();
        match (new_raw.is_empty(), other.is_empty()) {
            (true, _) => other,
            (_, true) => new_raw.to_string(),
            _ => match scope {
                PathScope::System => format!("{new_raw}{sep}{other}"),
                PathScope::User => format!("{other}{sep}{new_raw}"),
            },
        }
    }
    #[cfg(not(windows))]
    {
        let _ = scope;
        new_raw.to_string()
    }
}

// ---------- 备份 ----------

pub fn backup(scope: PathScope, raw: &str) -> Result<String> {
    let layout = paths::layout()?;
    #[cfg(windows)]
    let expand = read_registry(scope).map(|(_, e)| e).unwrap_or(true);
    #[cfg(not(windows))]
    let expand = true;

    let id = format!("{}-{}", scope_tag(scope), util::timestamp());
    let file = layout.backups.join(format!("{BACKUP_PREFIX}{id}.json"));
    util::write_json(
        &file,
        &BackupFile {
            id: id.clone(),
            created_at: chrono::Local::now().to_rfc3339(),
            scope,
            raw: raw.to_string(),
            expand,
        },
    )?;
    prune_backups()?;
    Ok(id)
}

fn scope_tag(scope: PathScope) -> &'static str {
    match scope {
        PathScope::User => "user",
        PathScope::System => "system",
    }
}

pub fn list_backups() -> Result<Vec<PathBackup>> {
    let layout = paths::layout()?;
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir(&layout.backups) else {
        return Ok(out);
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("").to_string();
        if !name.starts_with(BACKUP_PREFIX) || !name.ends_with(".json") {
            continue;
        }
        let Ok(Some(b)) = util::read_json_opt::<BackupFile>(&path) else {
            continue;
        };
        let entries_vec = util::split_path_list(&b.raw);
        out.push(PathBackup {
            id: b.id,
            created_at: b.created_at,
            scope: b.scope,
            kind: BackupKind::Path,
            entry_count: entries_vec.len(),
            preview: entries_vec.into_iter().take(6).collect(),
        });
    }
    out.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    Ok(out)
}

/// 一键回滚到某个备份。scope 由备份自己记着，避免把用户级内容写进系统级。
pub fn restore(id: &str) -> Result<PathStatus> {
    let layout = paths::layout()?;
    let file = layout.backups.join(format!("{BACKUP_PREFIX}{id}.json"));
    let b: BackupFile = util::read_json_opt(&file)?
        .ok_or_else(|| Error::NotFound(format!("备份 {id} 的文件不在了")))?;
    let current = read_raw(b.scope)?;
    // 回滚本身也要留痕，否则连续回滚两次就找不回来。
    backup(b.scope, &current)?;
    #[cfg(windows)]
    {
        let raw = b.raw.clone();
        if b.scope == PathScope::System && !is_elevated() {
            raise_and_write(&raw)?;
        } else {
            write_now(b.scope, &raw)?;
        }
    }
    #[cfg(not(windows))]
    write_now(b.scope, &b.raw)?;
    adopt_own_process(b.scope, &b.raw);
    status(b.scope)
}

/// 用"还没被我们动过"的那份备份回滚：从新往旧找第一份不含转发目录的，它才是改前的现场。
///
/// 直接取最新一份会翻车：最新的那份常常是上一次改动时拍的，里面还带着转发目录，
/// 恢复它等于把刚要收走的东西又摆回 PATH。值和现状已经一样时返回 None，
/// 让调用方走"只摘自己那一段"，不必为了写回一模样的字符串再弹一次广播。
pub fn withdraw_via_backup(scope: PathScope) -> Result<Option<String>> {
    let layout = paths::layout()?;
    let current = read_raw(scope)?;
    let mut snapshots: Vec<(String, String)> = Vec::new();
    for b in list_backups()? {
        if b.scope != scope {
            continue;
        }
        let file = layout.backups.join(format!("{BACKUP_PREFIX}{}.json", b.id));
        if let Some(s) = util::read_json_opt::<BackupFile>(&file)? {
            snapshots.push((b.id, s.raw));
        }
    }
    match pick_pristine(&layout.shims, &current, &snapshots) {
        None => Ok(None),
        Some(id) => {
            restore(&id)?;
            Ok(Some(id))
        }
    }
}

/// 从新往旧挑第一份不含转发目录的备份。列表必须是新的在前。
fn pick_pristine(shim_dir: &Path, current: &str, snapshots: &[(String, String)]) -> Option<String> {
    for (id, raw) in snapshots {
        let holds = util::split_path_list(raw)
            .iter()
            .any(|e| paths::norm_eq(Path::new(e), shim_dir));
        if holds {
            continue;
        }
        // 干净的那份就是现状：再往前翻会更旧，环境本来没被我们改过，不用恢复。
        return if raw == current { None } else { Some(id.clone()) };
    }
    None
}

fn prune_backups() -> Result<()> {
    let layout = paths::layout()?;
    let mut files = Vec::new();
    if let Ok(entries) = std::fs::read_dir(&layout.backups) {
        for e in entries.flatten() {
            let p = e.path();
            let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("").to_string();
            if name.starts_with(BACKUP_PREFIX) && name.ends_with(".json") {
                files.push(p);
            }
        }
    }
    files.sort();
    while files.len() > MAX_BACKUPS {
        let old = files.remove(0);
        let _ = std::fs::remove_file(old);
    }
    Ok(())
}

pub fn both_statuses() -> Result<(PathStatus, Option<PathStatus>)> {
    let user = status(PathScope::User)?;
    let system = status(PathScope::System).ok();
    Ok((user, system))
}

#[cfg(all(test, windows))]
mod tests {
    use super::{merged, pick_pristine, removed};
    use std::path::Path;

    const SHIM: &str = r"F:\data\shims";

    #[test]
    fn 放上去再摘掉能逐字回到原值() {
        // 本机系统 PATH 里就有空条目和结尾的分号，必须原样带过去再带回来。
        let raw = r"C:\Windows;F:\tools;;E:\Program Files\nodejs\;";
        for priority in [true, false] {
            assert_eq!(removed(&merged(raw, Path::new(SHIM), priority), Path::new(SHIM)), raw);
        }
    }

    #[test]
    fn 置顶时排在第一个() {
        let out = merged(r"A;B", Path::new(SHIM), true);
        assert_eq!(out, format!(r"{SHIM};A;B"));
    }

    #[test]
    fn 让位时追加在最后() {
        let out = merged(r"A;B", Path::new(SHIM), false);
        assert_eq!(out, format!(r"A;B;{SHIM}"));
    }

    #[test]
    fn 已经在位只挪位置不重复() {
        let out = merged(format!(r"A;{SHIM};B").as_str(), Path::new(SHIM), true);
        assert_eq!(out, format!(r"{SHIM};A;B"));
    }

    #[test]
    fn 原值还没内容时直接给出转发目录() {
        assert_eq!(merged("", Path::new(SHIM), true), SHIM);
    }

    #[test]
    fn 大小写和末尾斜杠算同一条() {
        let out = merged(r"A;f:\DATA\Shims\;B", Path::new(SHIM), true);
        assert_eq!(out, format!(r"{SHIM};A;B"));
    }

    #[test]
    fn 没插过时摘掉保持不动() {
        let raw = r"C:\Windows;F:\tools";
        assert_eq!(removed(raw, Path::new(SHIM)), raw);
    }

    #[test]
    fn 回滚挑改前那份而不是最新的() {
        // 现场实测的形状：最新一份是改动时拍的，里面还带着转发目录，照它恢复等于没收回去。
        let snaps = vec![
            (r"user-2".to_string(), format!(r"{SHIM};A;B")),
            ("user-1".to_string(), r"A;B".to_string()),
        ];
        assert_eq!(
            pick_pristine(Path::new(SHIM), &format!(r"{SHIM};A;B"), &snaps),
            Some("user-1".to_string())
        );
    }

    #[test]
    fn 干净备份和现状一样时不恢复() {
        let snaps = vec![
            (r"user-2".to_string(), format!(r"{SHIM};A;B")),
            ("user-1".to_string(), r"A;B".to_string()),
        ];
        assert_eq!(pick_pristine(Path::new(SHIM), r"A;B", &snaps), None);
    }

    #[test]
    fn 一份干净的都没有就不靠备份恢复() {
        let snaps = vec![(r"user-2".to_string(), format!(r"{SHIM};A;B"))];
        assert_eq!(pick_pristine(Path::new(SHIM), &format!(r"{SHIM};A;B"), &snaps), None);
        // 大小写不同也是同一条，不能当成干净的备份。
        let dirty = vec![(r"user-1".to_string(), r"f:\data\shims;A;B".to_string())];
        assert_eq!(pick_pristine(Path::new(SHIM), r"A;B", &dirty), None);
    }
}
