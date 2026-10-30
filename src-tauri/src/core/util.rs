use std::path::{Path, PathBuf};

use serde::de::DeserializeOwned;
use serde::Serialize;

use super::error::{Error, Result};

pub fn read_json_opt<T: DeserializeOwned>(path: &Path) -> Result<Option<T>> {
    if !path.exists() {
        return Ok(None);
    }
    let text = std::fs::read_to_string(path)
        .map_err(|e| Error::Io(format!("读 {} 失败：{e}", path.display())))?;
    if text.trim().is_empty() {
        return Ok(None);
    }
    serde_json::from_str::<T>(&text)
        .map(Some)
        .map_err(|e| Error::Config(format!("{} 内容不对：{e}", path.display())))
}

pub fn read_json_or<T: DeserializeOwned>(path: &Path, fallback: impl FnOnce() -> T) -> Result<T> {
    Ok(read_json_opt(path)?.unwrap_or_else(fallback))
}

/// 先写同目录临时文件再改名，避免断电或并发写留下半个 JSON。
pub fn write_json<T: Serialize>(path: &Path, value: &T) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| Error::Io(format!("建目录 {} 失败：{e}", parent.display())))?;
    }
    let tmp = path.with_extension(format!("json.tmp-{}", std::process::id()));
    let text = serde_json::to_string_pretty(value)
        .map_err(|e| Error::Config(format!("序列化失败：{e}")))?;
    std::fs::write(&tmp, text).map_err(|e| Error::Io(format!("写 {} 失败：{e}", tmp.display())))?;
    std::fs::rename(&tmp, path)
        .map_err(|e| Error::Io(format!("替换 {} 失败：{e}", path.display())))?;
    Ok(())
}

/// 版本号比较：按 . - _ 拆段，数字段按数值、其余按文本，够 Node/Python 的形态用。
pub fn version_cmp(a: &str, b: &str) -> std::cmp::Ordering {
    use std::cmp::Ordering;
    let ta = split_version(a);
    let tb = split_version(b);
    for (x, y) in ta.iter().zip(tb.iter()) {
        match (x.parse::<u64>(), y.parse::<u64>()) {
            (Ok(nx), Ok(ny)) => match nx.cmp(&ny) {
                Ordering::Equal => continue,
                o => return o,
            },
            (Ok(_), Err(_)) => return Ordering::Greater,
            (Err(_), Ok(_)) => return Ordering::Less,
            (Err(_), Err(_)) => match x.cmp(y) {
                Ordering::Equal => continue,
                o => return o,
            },
        }
    }
    ta.len().cmp(&tb.len())
}

pub fn version_desc(a: &str, b: &str) -> std::cmp::Ordering {
    version_cmp(b, a)
}

fn split_version(v: &str) -> Vec<String> {
    v.trim()
        .trim_start_matches('v')
        .split(|c| matches!(c, '.' | '-' | '_' | '+'))
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .collect()
}

pub fn version_major(v: &str) -> Option<u64> {
    split_version(v).first().and_then(|s| s.parse().ok())
}

/// "20.18.0" 与 "20" 算同一主线，用于把官方列表里的小版本折叠成卡片。
pub fn same_line(a: &str, b: &str) -> bool {
    let (sa, sb) = (split_version(a), split_version(b));
    match (sa.first(), sb.first()) {
        (Some(x), Some(y)) => x == y,
        _ => false,
    }
}

pub fn dir_size(path: &Path) -> u64 {
    let Ok(entries) = std::fs::read_dir(path) else {
        return 0;
    };
    entries.flatten().fold(0u64, |acc, e| {
        let p = e.path();
        if p.is_dir() {
            acc + dir_size(&p)
        } else {
            acc + e.metadata().map(|m| m.len()).unwrap_or(0)
        }
    })
}

/// 删除尽力而为：卸载时个别文件被占用不该让整个卸载失败。
pub fn remove_tree(path: &Path) -> (usize, Vec<String>) {
    let mut failed = Vec::new();
    let mut removed = 0usize;
    fn walk(p: &Path, failed: &mut Vec<String>, removed: &mut usize) {
        let Ok(entries) = std::fs::read_dir(p) else {
            failed.push(p.display().to_string());
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() && !path.is_symlink() {
                walk(&path, failed, removed);
            }
            match std::fs::remove_file(&path) {
                Ok(_) => *removed += 1,
                Err(_) => failed.push(path.display().to_string()),
            }
        }
        match std::fs::remove_dir(p) {
            Ok(_) => *removed += 1,
            Err(e) => failed.push(format!("{}（{}）", p.display(), e)),
        }
    }
    if path.is_dir() {
        walk(path, &mut failed, &mut removed);
    } else if path.exists() {
        match std::fs::remove_file(path) {
            Ok(_) => removed += 1,
            Err(e) => failed.push(format!("{}（{e}）", path.display())),
        }
    }
    (removed, failed)
}

pub fn remove_tree_strict(path: &Path) -> Result<()> {
    let (removed, failed) = remove_tree(path);
    if !failed.is_empty() {
        return Err(Error::Io(format!(
            "有 {} 项没能删掉（前几项：{}），可能正被进程占用；已删掉 {removed} 项",
            failed.len(),
            failed[..failed.len().min(3)].join("、")
        )));
    }
    Ok(())
}

pub fn human_bytes(n: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut v = n as f64;
    let mut i = 0;
    while v >= 1024.0 && i < UNITS.len() - 1 {
        v /= 1024.0;
        i += 1;
    }
    if i == 0 {
        format!("{n} {}", UNITS[0])
    } else {
        format!("{:.1} {}", v, UNITS[i])
    }
}

/// 只保留字母数字与少数安全字符，用于把版本号变成文件名。
pub fn safe_file_stem(s: &str) -> String {
    s.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_') {
                c
            } else {
                '_'
            }
        })
        .collect()
}

pub fn file_name_of(url: &str) -> String {
    url.split(['?', '#']).next().unwrap_or(url).rsplit('/').next().unwrap_or("download").to_string()
}

pub fn path_list_sep() -> char {
    if cfg!(windows) { ';' } else { ':' }
}

pub fn split_path_list(raw: &str) -> Vec<String> {
    raw.split(path_list_sep())
        .map(|s| s.trim().trim_matches('"').to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

pub fn canonical(path: &Path) -> PathBuf {
    strip_verbatim(&path.canonicalize().unwrap_or_else(|_| path.to_path_buf()))
}

/// Windows 上 `canonicalize()` 会吐出 `\\?\C:\dir` 这种"逐字路径"。它自己能用，
/// 但当成参数交给 node、python 会被照普通路径解析成 `C:\?\C:\dir`，npm 就找不到入口脚本。
pub fn strip_verbatim(p: &Path) -> PathBuf {
    let text = p.as_os_str().to_string_lossy();
    if let Some(rest) = text.strip_prefix(r"\\?\UNC\") {
        // UNC 共享去掉前缀后要留回 `\\server\share`，否则变成普通相对路径。
        return PathBuf::from(format!(r"\\{rest}"));
    }
    if let Some(rest) = text.strip_prefix(r"\\?\") {
        return PathBuf::from(&*rest);
    }
    p.to_path_buf()
}

pub fn timestamp() -> String {
    chrono::Local::now().format("%Y%m%d-%H%M%S").to_string()
}

#[cfg(test)]
mod tests {
    use super::strip_verbatim;
    use std::path::{Path, PathBuf};

    #[test]
    fn 逐字路径前缀去掉后才是能外发的写法() {
        assert_eq!(strip_verbatim(Path::new(r"\\?\C:\env\data")), PathBuf::from(r"C:\env\data"));
        assert_eq!(strip_verbatim(Path::new(r"\\?\UNC\srv\share")), PathBuf::from(r"\\srv\share"));
        assert_eq!(strip_verbatim(Path::new("/home/u/data")), PathBuf::from("/home/u/data"));
    }
}
