//! Shell 集成：往 PATH 里补 shim 目录，并在 cd 后播报当前目录会用到哪个版本。
//!
//! 版本切换本身靠 shim 在启动时读当前工作目录，不依赖 shell；这里的脚本只做两件事：
//! 让新开终端立刻认得 shim 目录，以及把"进目录后用的是哪个版本"显示出来。
use std::path::{Path, PathBuf};

use serde::Serialize;

use super::error::{Error, Result};
use super::{paths, settings::Settings, shim, util};

pub const HOOK_DIR: &str = "hooks";
pub const MARK_BEGIN: &str = "# >>> EnvSwitch hook >>>";
pub const MARK_END: &str = "# <<< EnvSwitch hook <<<";

/// Windows PowerShell 5.1 读没有 BOM 的 .ps1 时按系统 ANSI 代码页解码，
/// 数据目录路径里有中文就会被读成乱码，脚本引用因此失效。PowerShell 的文件一律带 BOM 写。
const PS_BOM: &[u8] = &[0xEF, 0xBB, 0xBF];

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HookStatus {
    pub shell: &'static str,
    pub label: &'static str,
    /// 生成出来的脚本本体，放在数据目录里。
    pub script: PathBuf,
    /// 引用该脚本的登录文件（~/.zshrc、PowerShell $PROFILE 之类）。
    pub rc_file: Option<PathBuf>,
    /// 这台电脑上是否认得出这个 shell。
    pub detected: bool,
    pub installed: bool,
    /// 脚本本体是否已经写好（内容随数据目录位置变化，任何时候都能重新生成）。
    pub script_ready: bool,
    pub note: Option<String>,
}

#[derive(Debug, Clone, Copy)]
struct ShellSpec {
    id: &'static str,
    label: &'static str,
    file: &'static str,
    posix: bool,
}

const SHELLS: &[ShellSpec] = &[
    ShellSpec { id: "powershell", label: "PowerShell", file: "envswitch.ps1", posix: false },
    ShellSpec { id: "bash", label: "Bash / Git Bash", file: "envswitch.sh", posix: true },
    ShellSpec { id: "zsh", label: "Zsh", file: "envswitch.sh", posix: true },
];

pub fn ids() -> Vec<&'static str> {
    SHELLS.iter().map(|s| s.id).collect()
}

fn spec(id: &str) -> Result<ShellSpec> {
    SHELLS
        .iter()
        .find(|s| s.id == id)
        .copied()
        .ok_or_else(|| {
            Error::NotFound(format!(
                "不认识名为 {id} 的终端，可选：{}",
                ids().join("、")
            ))
        })
}

/// 脚本本体所在目录：跟着数据目录走，整个文件夹搬走也不会留下悬空引用。
fn hook_dir() -> Result<PathBuf> {
    Ok(paths::layout()?.config.join(HOOK_DIR))
}

/// 幂等生成脚本内容；数据目录挪位后重新调用即可修正引用路径。
fn write_script(s: &ShellSpec) -> Result<PathBuf> {
    let dir = hook_dir()?;
    std::fs::create_dir_all(&dir)?;
    // 脚本里调用的命令行程序放在 shim 目录，顺带在这里补齐。
    let _ = shim::ensure_cli_copy();
    let file = dir.join(s.file);
    let mut out: Vec<u8> = Vec::new();
    if !s.posix {
        out.extend_from_slice(PS_BOM);
    }
    out.extend_from_slice(if s.posix { posix_script() } else { ps_script() }.as_bytes());
    // 内容一致就不重写，避免每次打开设置页都碰一下文件时间。
    if std::fs::read(&file).ok().as_deref() != Some(out.as_slice()) {
        std::fs::write(&file, out)?;
    }
    Ok(file)
}

fn posix_script() -> String {
    let layout = match paths::layout() {
        Ok(l) => l,
        Err(_) => return "# EnvSwitch：数据目录还没选好，先在界面里完成首次设置\n".to_string(),
    };
    let shim_dir = sh_quote(&layout.shims);
    let bin = sh_quote(&cli_path(&layout.shims));
    format!(
        r#"# EnvSwitch 终端集成（自动生成，改动会在下次生成时被覆盖）
ENVSWITCH_SHIM_DIR={shim_dir}
ENVSWITCH_BIN={bin}

case ":$PATH:" in
  *":$ENVSWITCH_SHIM_DIR:"*) ;;
  *) PATH="$ENVSWITCH_SHIM_DIR:$PATH"; export PATH ;;
esac

envswitch_explain() {{
  [ -x "$ENVSWITCH_BIN" ] && "$ENVSWITCH_BIN" explain 2>/dev/null
}}

cd() {{
  if [ "$#" -eq 0 ]; then
    builtin cd && envswitch_explain
  else
    builtin cd "$@" && envswitch_explain
  fi
}}

envswitch() {{
  "$ENVSWITCH_BIN" "$@"
}}

envswitch_explain
"#
    )
}

fn ps_script() -> String {
    let layout = match paths::layout() {
        Ok(l) => l,
        Err(_) => return "# EnvSwitch：数据目录还没选好，先在界面里完成首次设置\n".to_string(),
    };
    let shim_dir = ps_path(&layout.shims);
    let bin = ps_path(&cli_path(&layout.shims));
    format!(
        r#"# EnvSwitch 终端集成（自动生成，改动会在下次生成时被覆盖）
$env:ENVSWITCH_SHIM_DIR = '{shim_dir}'
$env:ENVSWITCH_BIN = '{bin}'

if ((Test-Path $env:ENVSWITCH_SHIM_DIR) -and (($env:Path -split [IO.Path]::PathSeparator) -notcontains $env:ENVSWITCH_SHIM_DIR)) {{
    $env:Path = "$($env:ENVSWITCH_SHIM_DIR)$([IO.Path]::PathSeparator)$($env:Path)"
}}

function global:envswitch-explain {{
    if (Test-Path $env:ENVSWITCH_BIN) {{
        & $env:ENVSWITCH_BIN explain 2>$null | Out-Host
    }}
}}

function global:cd {{
    param([Parameter(Position = 0)][string]$Path = (Get-Location).Path)
    Set-Location -Path $Path
    envswitch-explain
}}

function global:envswitch {{
    & $env:ENVSWITCH_BIN $args
}}

envswitch-explain
"#
    )
}

fn cli_path(shims: &Path) -> PathBuf {
    shims.join(shim::SHIM_BIN_NAME)
}

/// 单引号里的路径：MSYS 写法不带反斜杠，只剩单引号需要拼出来。
fn sh_quote(p: &Path) -> String {
    format!("'{}'", msys_path(p).replace('\'', r"'\''"))
}

/// Git Bash / MSYS 里的路径写法。
///
/// 反斜杠形式在 MSYS 里会被拆坏（`F:\a\b` 认成相对路径），命令行了也找不到文件，
/// 所以给它的一律换成 `/f/a/b`；本机是 Linux/macOS 时原样返回。
fn msys_path(p: &Path) -> String {
    let raw = util::strip_verbatim(p).to_string_lossy().replace('\\', "/");
    let bytes = raw.as_bytes();
    if bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':' {
        return format!("/{}", raw[0..1].to_lowercase()) + &raw[2..];
    }
    raw
}

/// 单引号里的路径：反斜杠原样保留，只有单引号需要翻倍。
fn ps_path(p: &Path) -> String {
    p.display().to_string().replace('\'', "''")
}

/// 登录文件位置：PowerShell 用 $PROFILE，其余放在用户主目录。
fn rc_file(s: &ShellSpec) -> Option<PathBuf> {
    let home = dirs::home_dir()?;
    match s.id {
        "powershell" => {
            let docs = dirs::document_dir().unwrap_or_else(|| home.clone());
            // pwsh 读 Documents\PowerShell，Windows PowerShell 5.1 读 Documents\WindowsPowerShell。
            // 写进另一个文件夹里的那个文件，那个版本的 PowerShell 永远不会加载。
            let dir = if which("pwsh").is_some() { "PowerShell" } else { "WindowsPowerShell" };
            Some(docs.join(dir).join("Microsoft.PowerShell_profile.ps1"))
        }
        "bash" => Some(home.join(".bashrc")),
        "zsh" => Some(home.join(".zshrc")),
        _ => None,
    }
}

/// 这台电脑上是否装了该 shell；没装就不在列表里铺一堆用不上的开关。
fn detected(s: &ShellSpec) -> bool {
    match s.id {
        "powershell" => which("pwsh")
            .or_else(|| which("powershell"))
            .or_else(system_powershell)
            .is_some(),
        "bash" => which("bash").is_some() || PathBuf::from("/bin/bash").is_file() || git_bash().is_some(),
        "zsh" => PathBuf::from("/bin/zsh").is_file() || which("zsh").is_some(),
        _ => false,
    }
}

/// Windows PowerShell 的位置是系统固定的。进程 PATH 可能只是启动时的旧快照，
/// 光靠它找会误报"这台电脑上没找到 PowerShell"，所以按 SystemRoot 兜一次。
#[cfg(windows)]
fn system_powershell() -> Option<PathBuf> {
    let root = std::env::var_os("SystemRoot").map(PathBuf::from)?;
    let cand = root
        .join("System32")
        .join("WindowsPowerShell")
        .join("v1.0")
        .join("powershell.exe");
    cand.is_file().then_some(cand)
}

#[cfg(not(windows))]
fn system_powershell() -> Option<PathBuf> {
    None
}

/// Git for Windows 默认装在 %ProgramFiles%\Git，但它常常只注册了右键菜单、不进 PATH，
/// 光靠 PATH 找会把"装了 Git Bash"读成"这台电脑上没找到"。
#[cfg(windows)]
fn git_bash() -> Option<PathBuf> {
    let root = std::env::var_os("ProgramFiles").map(PathBuf::from)?;
    let cand = root.join("Git").join("bin").join("bash.exe");
    cand.is_file().then_some(cand)
}

#[cfg(not(windows))]
fn git_bash() -> Option<PathBuf> {
    None
}

fn which(exe: &str) -> Option<PathBuf> {
    let raw = std::env::var_os("PATH")?;
    let sep = util::path_list_sep();
    for dir in raw.to_string_lossy().split(sep) {
        let dir = dir.trim().trim_matches('"');
        if dir.is_empty() {
            continue;
        }
        let cand = PathBuf::from(dir).join(exe);
        if cand.is_file() {
            return Some(cand);
        }
        #[cfg(windows)]
        {
            let with_ext = PathBuf::from(dir).join(format!("{exe}.exe"));
            if with_ext.is_file() {
                return Some(with_ext);
            }
        }
    }
    None
}

fn line_for(s: &ShellSpec, script: &Path) -> String {
    if s.posix {
        let p = sh_quote(script);
        format!("[ -f {p} ] && . {p}")
    } else {
        ps_line(script)
    }
}

/// PowerShell 的引用行。
///
/// 登录文件归用户所有，谁都可能把它另存成 GBK；那时我们写的 BOM 会跟着一起没掉，
/// 路径里的中文又会被读乱。所以路径带中文时改成整行纯 ASCII 的通配符写法，
/// 跟文件编码彻底无关。全 ASCII 的路径用字面量，最准。
fn ps_line(script: &Path) -> String {
    match ascii_glob(&script.display().to_string()) {
        Some(glob) => format!(
            "$EnvSwitchHook = Get-Item -Path '{glob}' -ErrorAction SilentlyContinue | Select-Object -First 1\n\
             if ($EnvSwitchHook) {{ . $EnvSwitchHook.FullName }}"
        ),
        None => format!(". '{}'", ps_path(script)),
    }
}

/// 把非 ASCII 字符逐个换成 ?（一个中文字符正好是一个 ?）。
/// 路径里已经有通配符字符、或者有多于一个字节的非 BMP 字符时不换——那种情况交给 BOM。
fn ascii_glob(raw: &str) -> Option<String> {
    if raw.is_ascii() {
        return None;
    }
    let mut out = String::with_capacity(raw.len());
    for c in raw.chars() {
        match c {
            '?' | '*' | '[' | ']' => return None,
            c if c.len_utf16() != 1 => return None,
            c if c.is_ascii() => out.push(c),
            _ => out.push('?'),
        }
    }
    Some(out)
}

fn block_for(s: &ShellSpec, script: &Path) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(MARK_BEGIN.as_bytes());
    out.push(b'\n');
    out.extend_from_slice(line_for(s, script).as_bytes());
    out.push(b'\n');
    out.extend_from_slice(MARK_END.as_bytes());
    out.push(b'\n');
    out
}

fn find_mark(bytes: &[u8], mark: &str) -> Option<usize> {
    bytes
        .windows(mark.len())
        .position(|w| w == mark.as_bytes())
}

fn has_block(path: &Path) -> bool {
    std::fs::read(path)
        .map(|b| find_mark(&b, MARK_BEGIN).is_some())
        .unwrap_or(false)
}

/// 登录文件当前的内容（BOM 去掉）。文件不是 UTF-8 时直接报错：
/// 那种内容我们一旦重写就会把用户已有的中文弄坏，宁可不动。
fn rc_text(path: &Path, s: &ShellSpec, script: &Path) -> Result<Vec<u8>> {
    let bytes = match std::fs::read(path) {
        Ok(b) => b,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => {
            return Err(Error::Permission(format!(
                "读不了 {}：{e}",
                path.display()
            )))
        }
    };
    let body = bytes.strip_prefix(PS_BOM).unwrap_or(&bytes);
    std::str::from_utf8(body).map_err(|_| {
        Error::Unsupported(format!(
            "{} 的登录文件 {} 不是 UTF-8 编码，改了会把里面的中文弄乱，所以没有动它。把它另存为 UTF-8 再开，或者手动加上这一段：\n{}",
            s.label,
            path.display(),
            line_for(s, script)
        ))
    })?;
    Ok(bytes)
}

fn write_block(path: &Path, s: &ShellSpec, script: &Path) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let raw = rc_text(path, s, script)?;
    let mut body = cut_bytes(&raw);
    if !s.posix {
        body = body.strip_prefix(PS_BOM).unwrap_or(&body).to_vec();
    }
    if !body.is_empty() && !body.ends_with(b"\n") {
        body.push(b'\n');
    }
    let mut out = Vec::new();
    if !s.posix {
        out.extend_from_slice(PS_BOM);
    }
    out.extend_from_slice(&body);
    out.extend_from_slice(&block_for(s, script));
    std::fs::write(path, out)
        .map_err(|e| Error::Permission(format!("写不进 {}：{e}", path.display())))
}

/// 去掉引用块并写回；返回是否真的动过文件。按字节裁剪，用户的编码原样保留。
fn remove_block(path: &Path) -> Result<bool> {
    let text = match std::fs::read(path) {
        Ok(b) => b,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(e) => {
            return Err(Error::Permission(format!(
                "读不了 {}：{e}",
                path.display()
            )))
        }
    };
    if find_mark(&text, MARK_BEGIN).is_none() {
        return Ok(false);
    }
    let cleaned = cut_bytes(&text);
    // 摘完什么都不剩说明这文件本来就是我们建的，别在用户文档目录里留个空壳。
    if cleaned.is_empty() {
        return std::fs::remove_file(path)
            .map(|_| true)
            .map_err(|e| Error::Permission(format!("删不掉 {}：{e}", path.display())));
    }
    std::fs::write(path, cleaned)
        .map_err(|e| Error::Permission(format!("写不进 {}：{e}", path.display())))?;
    Ok(true)
}

fn cut_bytes(bytes: &[u8]) -> Vec<u8> {
    let keep_bom = bytes.starts_with(PS_BOM);
    let body = bytes.strip_prefix(PS_BOM).unwrap_or(bytes);
    let mut out = match find_mark(body, MARK_BEGIN) {
        None => body.to_vec(),
        Some(start) => {
            let tail = &body[start..];
            let end = find_mark(tail, MARK_END)
                .map(|i| start + i + MARK_END.len())
                .unwrap_or(body.len());
            let cut_end = if body.get(end) == Some(&b'\n') { end + 1 } else { end };
            let before = trim_end_bytes(&body[..start], b'\n');
            let after = trim_start_bytes(&body[cut_end..], b'\n');
            let mut v = before.to_vec();
            if !before.is_empty() && !after.is_empty() {
                v.push(b'\n');
            }
            v.extend_from_slice(after);
            if !v.is_empty() && v.last() != Some(&b'\n') {
                v.push(b'\n');
            }
            v
        }
    };
    if keep_bom && !out.is_empty() {
        out.splice(0..0, PS_BOM.iter().copied());
    }
    out
}

fn trim_end_bytes(bytes: &[u8], byte: u8) -> &[u8] {
    let mut end = bytes.len();
    while end > 0 && bytes[end - 1] == byte {
        end -= 1;
    }
    &bytes[..end]
}

fn trim_start_bytes(bytes: &[u8], byte: u8) -> &[u8] {
    let mut start = 0;
    while start < bytes.len() && bytes[start] == byte {
        start += 1;
    }
    &bytes[start..]
}

fn status_one(s: &ShellSpec) -> HookStatus {
    let script = hook_dir().map(|d| d.join(s.file)).unwrap_or_default();
    let rc = rc_file(s);
    let installed = rc.as_ref().is_some_and(|p| has_block(p));
    let note = note_for(s, rc.as_deref());
    HookStatus {
        shell: s.id,
        label: s.label,
        script_ready: script.is_file(),
        script,
        installed,
        detected: detected(s),
        rc_file: rc,
        note,
    }
}

fn note_for(s: &ShellSpec, rc: Option<&Path>) -> Option<String> {
    if !detected(s) {
        return Some(format!("这台电脑上没找到 {}，装了以后再开", s.label));
    }
    if s.id == "powershell" {
        return Some(
            "若脚本被执行策略拦住，用 Get-ExecutionPolicy -List 看看 CurrentUser，必要时执行 Set-ExecutionPolicy -Scope CurrentUser RemoteSigned"
                .to_string(),
        );
    }
    rc.filter(|p| !p.exists())
        .map(|p| format!("还没有 {}，开启后会自动建一个", p.display()))
}

pub fn status() -> Result<Vec<HookStatus>> {
    Ok(SHELLS.iter().map(status_one).collect())
}

pub fn install(id: &str) -> Result<HookStatus> {
    let s = spec(id)?;
    if let Some(p) = rc_file(&s) {
        let script = write_script(&s)?;
        write_block(&p, &s, &script)?;
    } else {
        return Err(Error::Unsupported(format!(
            "找不到 {} 的登录文件位置，请手动把数据目录 config/hooks 里的脚本引入",
            s.label
        )));
    }
    Ok(status_one(&s))
}

pub fn remove(id: &str) -> Result<HookStatus> {
    let s = spec(id)?;
    if let Some(p) = rc_file(&s) {
        remove_block(&p)?;
    }
    Ok(status_one(&s))
}

pub fn install_all() -> Result<Vec<HookStatus>> {
    let mut out = Vec::new();
    for s in SHELLS {
        if detected(s) {
            out.push(install(s.id)?);
        }
    }
    Ok(out)
}

/// 关掉集成时清掉引用；脚本本体留着无害，重置功能会一并删。
pub fn remove_all() -> Result<Vec<String>> {
    let mut removed = Vec::new();
    for s in SHELLS {
        if let Some(p) = rc_file(s)
            && remove_block(&p)?
        {
            removed.push(s.id.to_string());
        }
    }
    Ok(removed)
}

/// 数据目录换了位置后重新生成脚本并重写登录文件里的引用行。
pub fn refresh_scripts() -> Result<Vec<HookStatus>> {
    let mut out = Vec::new();
    for s in SHELLS.iter().filter(|s| status_one(s).installed) {
        out.push(install(s.id)?);
    }
    // 一个都没启用时不生成，免得数据目录里躺着没人用的脚本。
    Ok(out)
}

/// 设置页里"终端集成"开关的落点：开着就把认得出的 shell 全写上，关着一律摘掉。
pub fn apply_setting(enabled: bool) -> Result<Vec<HookStatus>> {
    if enabled {
        install_all()
    } else {
        let _ = remove_all()?;
        status()
    }
}

pub fn delete_scripts() {
    if let Ok(dir) = hook_dir() {
        let _ = std::fs::remove_dir_all(&dir);
    }
}

/// 当前是否已经具备"新开终端即用"的条件，供首次设置页提示。
pub fn any_installed() -> bool {
    Settings::load()
        .map(|s| s.auto_switch)
        .unwrap_or(false)
        && SHELLS.iter().any(|s| status_one(s).installed)
}

#[cfg(test)]
#[allow(non_snake_case)]
mod tests {
    use super::*;

    const SCRIPT: &str = r"F:\EnvSwitch_灵匣\envswitch-data\config\hooks\envswitch.ps1";

    fn ps() -> ShellSpec {
        SHELLS[0]
    }

    #[test]
    fn 中文路径的引用行整行纯ASCII() {
        let line = ps_line(Path::new(SCRIPT));
        assert!(line.is_ascii(), "引用行不该带非 ASCII：{line}");
        assert!(
            line.contains(r"F:\EnvSwitch_??\envswitch-data\config\hooks\envswitch.ps1"),
            "一个中文字换成一个 ?：{line}"
        );
    }

    #[test]
    fn 纯ASCII路径照旧写字面量() {
        assert_eq!(
            ps_line(Path::new(r"C:\tools\hooks\envswitch.ps1")),
            r". 'C:\tools\hooks\envswitch.ps1'"
        );
    }

    #[test]
    fn 路径里有通配符字符时不改写() {
        assert_eq!(
            ps_line(Path::new(r"F:\a[1]_灵匣\envswitch.ps1")),
            r#". 'F:\a[1]_灵匣\envswitch.ps1'"#
        );
    }

    #[test]
    fn 给bash的路径换成MSYS写法() {
        assert_eq!(msys_path(Path::new(r"F:\FunctionFiles\EnvSwitch_灵匣\shims")), "/f/FunctionFiles/EnvSwitch_灵匣/shims");
        assert_eq!(msys_path(Path::new(r"C:\tools\hooks\envswitch.sh")), "/c/tools/hooks/envswitch.sh");
        // UNC 共享在 MSYS 里是两个斜杠开头，别压成一个。
        assert_eq!(msys_path(Path::new(r"\\nas\share\shims")), "//nas/share/shims");
        // 本机不是 Windows 时路径本来就带斜杠，原样返回。
        assert_eq!(msys_path(Path::new("/home/me/envswitch/shims")), "/home/me/envswitch/shims");
    }

    #[test]
    fn bash引用不带反斜杠也不漏变量() {
        // 单引号里 $ 和空格都不会被展开，路径里再也不会留反斜杠。
        let q = sh_quote(Path::new(r"D:\my $dir\hooks\envswitch.sh"));
        assert_eq!(q, r"'/d/my $dir/hooks/envswitch.sh'");
        assert_eq!(sh_quote(Path::new(r"E:\it's\envswitch.sh")), r"'/e/it'\''s/envswitch.sh'");
    }

    #[test]
    fn 装上再摘掉能逐字回到原样() {
        let user = "\u{feff}# 我自己的配置\nSet-PSReadLineOption -PredictionSource History\n";
        let user = user.as_bytes();
        let s = ps();
        let script = Path::new(SCRIPT);

        let installed = [user, block_for(&s, script).as_slice()].concat();
        assert!(String::from_utf8_lossy(&installed).contains("EnvSwitchHook"));

        let back = cut_bytes(&installed);
        assert_eq!(back, user, "摘掉后要和原文件逐字节一样");

        // 重新生成时先把旧块裁掉，不会越堆越多。
        let mut again = back.clone();
        again.extend_from_slice(&block_for(&s, script));
        assert_eq!(cut_bytes(&again), user);
        assert_eq!(find_mark(&again, MARK_BEGIN), find_mark(&installed, MARK_BEGIN));
    }

    #[test]
    fn 用户自己的内容一行都不动() {
        let head = "Write-Host 'hi'\n";
        let tail = "function ll { ls }\n";
        let s = ps();
        let joined = [
            head.as_bytes(),
            block_for(&s, Path::new(SCRIPT)).as_slice(),
            tail.as_bytes(),
        ]
        .concat();
        assert_eq!(String::from_utf8(cut_bytes(&joined)).unwrap(), format!("{head}{tail}"));
    }

    #[test]
    fn 不是UTF8的登录文件拒绝改写() {
        let dir = std::env::temp_dir().join(format!("envswitch-hook-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("Microsoft.PowerShell_profile.ps1");
        // GBK 编码的“灵匣”，不是合法 UTF-8。
        std::fs::write(&file, [0xE9, 0xE2, 0xBB, 0xF3]).unwrap();

        let e = rc_text(&file, &ps(), Path::new(SCRIPT)).expect_err("不该接受这种文件");
        assert_eq!(e.kind(), "unsupported", "{e}");
        assert!(e.to_string().contains("UTF-8"), "{e}");
        assert_eq!(std::fs::read(&file).unwrap(), [0xE9, 0xE2, 0xBB, 0xF3]);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
