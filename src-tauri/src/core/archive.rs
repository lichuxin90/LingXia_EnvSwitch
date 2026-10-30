use std::fs::File;
use std::io::{self, BufReader, Read};
use std::path::{Component, Path, PathBuf};

use bzip2::bufread::BzDecoder;
use flate2::read::GzDecoder;

use super::error::{Error, Result};

/// 解压上限：离线包是用户随便拖进来的，防 zip 炸弹与路径穿越。
const MAX_ENTRIES: usize = 200_000;
const MAX_TOTAL_BYTES: u64 = 8 * 1024 * 1024 * 1024;

pub fn extract(archive: &Path, dest: &Path) -> Result<Extracted> {
    let name = archive.file_name().and_then(|n| n.to_str()).unwrap_or("").to_lowercase();
    if name.ends_with(".zip") {
        extract_zip(archive, dest)
    } else if name.ends_with(".tar.gz") || name.ends_with(".tgz") {
        extract_tar_gz(archive, dest)
    } else if name.ends_with(".tar.bz2") || name.ends_with(".tbz2") {
        extract_tar_bz2(archive, dest)
    } else if name.ends_with(".tar.xz") || name.ends_with(".tar.zst") || name.ends_with(".7z") {
        Err(Error::Unsupported(format!(
            "{} 这种压缩包暂时打不开，请换成 .zip 或 .tar.gz",
            archive.display()
        )))
    } else {
        Err(Error::Unsupported(format!(
            "认不出 {} 是什么压缩包",
            archive.display()
        )))
    }
}

#[derive(Debug, Clone)]
pub struct Extracted {
    pub files: usize,
    pub bytes: u64,
    pub top_level_dir: Option<PathBuf>,
}

fn safe_join(base: &Path, raw: &str) -> Result<PathBuf> {
    let rel = Path::new(raw);
    if rel.is_absolute() || rel.has_root() {
        return Err(Error::Detail(format!("压缩包里有个绝对路径 {raw}，为安全起见停住了")));
    }
    let mut out = base.to_path_buf();
    for c in rel.components() {
        match c {
            Component::Normal(part) => out.push(part),
            Component::CurDir => {}
            Component::ParentDir => {
                return Err(Error::Detail(format!(
                    "压缩包里有个 {raw} 想跑到目录外面去，为安全起见停住了"
                )));
            }
            other => {
                return Err(Error::Detail(format!("压缩包里有个不认识的写法：{other:?}")));
            }
        }
    }
    Ok(out)
}

fn note_top(tops: &mut Vec<PathBuf>, raw: &str) {
    if let Some(first) = Path::new(raw).components().next() {
        let comp = PathBuf::from(first.as_os_str());
        if !tops.contains(&comp) && tops.len() < 2 {
            tops.push(comp);
        }
    }
}

fn extract_zip(archive: &Path, dest: &Path) -> Result<Extracted> {
    let file = File::open(archive)
        .map_err(|e| Error::Io(format!("打不开 {}：{e}", archive.display())))?;
    let mut zip = zip::ZipArchive::new(file)
        .map_err(|e| Error::Detail(format!("这个 zip 包读不了：{e}")))?;

    std::fs::create_dir_all(dest)?;
    let mut files = 0usize;
    let mut bytes = 0u64;
    let mut tops = Vec::new();

    for i in 0..zip.len() {
        if i >= MAX_ENTRIES {
            return Err(Error::Detail("压缩包里的文件多得不正常，停住了".to_string()));
        }
        let mut entry = zip
            .by_index(i)
            .map_err(|e| Error::Detail(format!("解压卡住：{e}")))?;
        let name = entry.name().to_string();
        let is_dir = entry.is_dir();
        let path = safe_join(dest, &name)?;
        note_top(&mut tops, &name);

        if is_dir {
            std::fs::create_dir_all(path)?;
            continue;
        }
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let size = entry.size();
        if bytes + size > MAX_TOTAL_BYTES {
            return Err(Error::Detail("解压出来太大了，为安全起见停住了".to_string()));
        }
        let mut out = File::create(&path)?;
        let written = io::copy(&mut entry, &mut out)
            .map_err(|e| Error::Io(format!("写 {} 失败：{e}", path.display())))?;
        bytes += written;
        files += 1;

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if let Some(mode) = entry.unix_mode() {
                let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(mode));
            }
        }
    }

    Ok(Extracted {
        files,
        bytes,
        top_level_dir: pick_top(tops),
    })
}

fn extract_tar_gz(archive: &Path, dest: &Path) -> Result<Extracted> {
    let file = open(archive)?;
    // 流式解压，tar 不需要随机读，所以不用先把整包落地。
    let mut tar = tar::Archive::new(GzDecoder::new(file));
    tar.set_preserve_permissions(true);
    tar.set_unpack_xattrs(false);
    extract_tar(&mut tar, dest)
}

/// PyPy 在 macOS/Linux 上发的是 .tar.bz2，只有 Windows 用 zip。
fn extract_tar_bz2(archive: &Path, dest: &Path) -> Result<Extracted> {
    let file = open(archive)?;
    let mut tar = tar::Archive::new(BzDecoder::new(BufReader::new(file)));
    tar.set_preserve_permissions(true);
    tar.set_unpack_xattrs(false);
    extract_tar(&mut tar, dest)
}

fn open(archive: &Path) -> Result<File> {
    File::open(archive).map_err(|e| Error::Io(format!("打不开 {}：{e}", archive.display())))
}

fn extract_tar<R: Read>(tar: &mut tar::Archive<R>, dest: &Path) -> Result<Extracted> {
    std::fs::create_dir_all(dest)?;
    let mut files = 0usize;
    let mut bytes = 0u64;
    let mut tops = Vec::new();

    for entry in tar.entries()? {
        if files >= MAX_ENTRIES {
            return Err(Error::Detail("压缩包里的文件多得不正常，停住了".to_string()));
        }
        let mut entry = entry?;
        let raw = String::from_utf8_lossy(&entry.path_bytes()).to_string();
        let path = safe_join(dest, &raw)?;
        note_top(&mut tops, &raw);

        let kind = entry.header().entry_type();
        if kind.is_dir() {
            std::fs::create_dir_all(path)?;
            continue;
        }
        if kind.is_file() {
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            let size = entry.header().size().unwrap_or(0);
            if bytes + size > MAX_TOTAL_BYTES {
                return Err(Error::Detail("解压出来太大了，为安全起见停住了".to_string()));
            }
            // unpack 会带上原包里的权限位，Linux 下 node/python 的执行位靠它保住。
            entry.unpack(&path)?;
            files += 1;
            bytes += size;
            continue;
        }
        if kind.is_symlink() || kind.is_hard_link() {
            // 预编译包里的 libpythonX.Y.so 之类软链接按原样重建。
            entry.unpack(&path)?;
            files += 1;
            continue;
        }
        // 其他类型（设备文件等）一律跳过，桌面开发用不到。
    }

    Ok(Extracted {
        files,
        bytes,
        top_level_dir: pick_top(tops),
    })
}

fn pick_top(mut tops: Vec<PathBuf>) -> Option<PathBuf> {
    if tops.len() == 1 {
        Some(tops.remove(0))
    } else {
        None
    }
}

/// 在解压结果里找到真正的载荷根：按插件给出的相对路径候选逐个试。
pub fn locate_payload_root(scanned: &Path, rel_candidates: &[PathBuf]) -> Option<PathBuf> {
    for rel in rel_candidates {
        // 先看 scanned 根下，再看唯一的那个子目录里。
        for base in candidate_bases(scanned) {
            if base.join(rel).exists() {
                return Some(base);
            }
        }
    }
    None
}

fn candidate_bases(scanned: &Path) -> Vec<PathBuf> {
    let mut out = vec![scanned.to_path_buf()];
    if let Ok(entries) = std::fs::read_dir(scanned) {
        let dirs: Vec<PathBuf> = entries
            .flatten()
            .filter(|e| e.path().is_dir())
            .map(|e| e.path())
            .collect();
        if dirs.len() == 1 {
            out.push(dirs[0].clone());
        }
    }
    out
}

/// 把 tar/zip 里的一层包裹目录去掉后挪到目标位置。
pub fn move_payload(from: &Path, to: &Path) -> Result<()> {
    if to.exists() {
        let _ = std::fs::remove_dir_all(to);
    }
    if let Some(parent) = to.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::rename(from, to).or_else(|_| {
        // 跨盘（数据目录和网络盘/不同分区）时改名会失败，退回复制。
        copy_tree(from, to).map(|_| ())
    })
}

pub fn copy_tree(from: &Path, to: &Path) -> Result<u64> {
    let mut copied = 0u64;
    std::fs::create_dir_all(to)?;
    let mut stack = vec![from.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir)? {
            let entry = entry?;
            let src = entry.path();
            let dst = to.join(entry.file_name());
            if src.is_dir() {
                stack.push(src);
            } else {
                if let Some(parent) = dst.parent() {
                    std::fs::create_dir_all(parent)?;
                }
                std::fs::copy(&src, &dst)?;
                copied += 1;
            }
        }
    }
    Ok(copied)
}

/// 读取包内某个文本文件的前若干字节，用于校验离线包的类型。
pub fn peek_first_bytes(path: &Path, n: usize) -> Result<Vec<u8>> {
    let mut f = File::open(path)?;
    let mut buf = vec![0u8; n];
    let read = f.read(&mut buf)?;
    buf.truncate(read);
    Ok(buf)
}
