# 更新日志 · Changelog

本文件记录每个版本改了什么。格式参考 [Keep a Changelog](https://keepachangelog.com/zh-CN/1.1.0/)，
版本号遵循 [语义化版本](https://semver.org/lang/zh-CN/)。GitHub Release 的正文以这里对应版本的内容为准。

简体中文 | [English](#english)

---

## [0.1.0] - 2026-10-30

首个公开版本。一台电脑上的 Node.js / Python / Go / PyPy 只装一份，由图形界面统一托管，
终端里照常敲 `node`、`python`、`go`，谁在哪个目录就用哪个版本。

### 新增

- **四个运行时**：Node.js（含 npm、npx 转发）、Python（python-build-standalone）、Go（含 gofmt、go vet 转发）、PyPy（归在 Python 下）。
- **版本托管**：从官方源或国内镜像列版本、下压缩包、校验 SHA256、解压到位，全程有进度，可以取消。
- **断点续传与并发分片**：大文件下载中断不用从头再来；同一个包已经下全就不会重复下载。
- **离线包导入**：内网机器拷来的官方压缩包可以直接装成受管版本。
- **项目级版本锁**：识别 `.tool-versions`（与 asdf 同一份格式，`node 22.14.0` 一行即可），进入目录自动生效；也可以在「项目级版本锁」页选目录后点「锁到这个」。
- **全局锁定**：开启后所有目录统一用指定版本，项目里的声明暂不生效，适合团队对齐口径。
- **PATH 接管与回滚**：每次写 PATH 前先备份，「环境与终端」页可按任意一条历史备份退回（退回前会再备份当前值）；需要管理员权限时提权。
- **终端集成**：往 PowerShell / bash / zsh 的登录文件写入托管片段，新开终端立刻认得命令入口，`cd` 之后播报当前目录用的是哪一版。
- **冲突检测**：认出 nvm、pyenv 以及散在各处的旧安装，可以选择接管也可以共存，不会悄悄删别人的东西。
- **生命周期提醒**：读 endoflife.date 的数据，提示某个版本已经停止维护。
- **首页体检**：一屏查看存放位置、命令入口、终端集成、版本档案四类状态，能自动修的一键修。
- **AI 客户端支持**：一键检测本机的 Claude Code、Codex CLI、Cursor、Qoder 等客户端，把统一的运行时说明写进它们的规则文件；检测不到时手动选目录兜底。
- **命令行工具 `envswitch-shim`**：与图形界面共用同一份配置，支持 `status` / `use` / `unuse` / `global` / `shims sync` / `hooks` / `help`。
- **设置导入导出**：换机器时把配置整体搬走。
- **四种界面语言**：简体中文、繁體中文、English、日本語，可跟随系统；托盘菜单与窗口标题一起切换。
- **启动可靠性**：主进程拉起校验 + 内置守护，起不来时能看到原因而不是静默失败。

### 已知限制

- 主要在 Windows 10/11 上开发与实跑验证。Linux 与 macOS 的路径处理、PATH 落地（写 `~/.profile` 的托管片段，全体用户走 `/etc/paths.d/`）都已实现，但还没逐平台实跑，遇到偏差请开 Issue。
- 安装包未做代码签名：Windows 上 SmartScreen 会拦一次，macOS 需要在「系统设置」的「隐私与安全性」里允许打开。
- 界面有四种语言，但命令行输出与程序内部报错里的路径、命令按约定保持中文，不做翻译。
- 官网的下载页与更新日志页直接读 GitHub Releases，本仓库有第一个 release 之前它们是空的。

---

## English

### [0.1.0] - 2026-10-30

First public release. One copy of each runtime per machine, managed from a GUI: keep typing `node`, `python` and `go` in your terminal, and the version used is the one that fits the folder you are standing in.

#### Added

- **Four runtimes**: Node.js (npm and npx forwarded too), Python (python-build-standalone), Go (gofmt, go vet forwarded) and PyPy (filed under Python).
- **Version hosting**: list versions from official sources or China mirrors, download, verify SHA256 and unpack into place, with progress reporting and cancel.
- **Resumable, chunked downloads**: an interrupted download picks up where it stopped; a package that is already complete is never fetched twice.
- **Offline import**: turn an official archive copied in from another machine into a managed version.
- **Per-project pinning**: reads `.tool-versions` (the same format asdf uses, one line such as `node 22.14.0`) and applies it as soon as you enter the folder; there is also a picker page.
- **Global lock**: force one version everywhere, ignoring project declarations; handy for aligning a team.
- **PATH takeover with rollback**: the previous value is backed up before every write, and any backup can be restored from the Environment & Terminal page; elevates when admin rights are needed.
- **Terminal integration**: writes a managed snippet into PowerShell / bash / zsh login files, so a fresh terminal knows the command entry points and announces the active version after each `cd`.
- **Conflict detection**: finds nvm, pyenv and loose older installs, and lets you take them over or coexist; it never quietly deletes someone else's setup.
- **End-of-life notices**: reads endoflife.date and flags versions that no longer get maintenance.
- **Home health check**: storage location, command entry points, terminal integration and version records on one screen, with one-click fixes where they can be automated.
- **AI client support**: detects Claude Code, Codex CLI, Cursor, Qoder and friends on the machine and writes the shared runtime notes into their rule files, with a manual folder picker as fallback.
- **`envswitch-shim` CLI**: shares the GUI's configuration and covers `status` / `use` / `unuse` / `global` / `shims sync` / `hooks` / `help`.
- **Settings import and export** for moving to another machine.
- **Four UI languages**: Simplified Chinese, Traditional Chinese, English and Japanese, following the system setting; the tray menu and window title switch with them.
- **Startup reliability**: the main process is verified as it comes up, with a built-in guard, so a failed start shows why instead of vanishing.

#### Known limitations

- Developed and tested mainly on Windows 10/11. The Linux and macOS path handling and PATH landing (a managed snippet in `~/.profile`, `/etc/paths.d/` for all users) are implemented but not yet exercised on every platform; please open an Issue when you hit a difference.
- Bundles are unsigned: SmartScreen warns once on Windows, and macOS needs an "Open" approval under System Settings, Privacy & Security.
- The UI speaks four languages, but terminal output and the paths inside internal errors stay in Chinese by design.
- The website's download and changelog pages read GitHub Releases directly, so they stay empty until this repository has its first release.

---
<!-- 上面每个版本的说明都以 --- 结尾；.github/workflows/release.yml 按这个约定把对应版本切成 Release 正文。 -->
