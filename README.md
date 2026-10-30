# 灵匣 · EnvSwitch

一台电脑上的 Node.js / Python / Go / PyPy，只装一份，谁用谁挑对的那版。

简体中文 | [繁體中文](README.zh-TW.md) | [English](README.en.md) | [日本語](README.ja.md)

官网：<https://envswitch.iamap.cn>　·　[更新日志](CHANGELOG.md)　·　问题反馈：<https://github.com/lichuxin90/LingXia_EnvSwitch/issues>

---

## 为什么做这个

装了三个 Node、两个 Python，命令行里 `node -v` 到底用哪个，全看 PATH 顺序撞运气；
切项目要 `nvm use`、换虚拟环境要 `source`，AI 助手写代码时又只会照着仓库里的旧说明走。

EnvSwitch 把这些收进一个图形界面：版本按统一的目录规则存放，终端里仍然直接敲 `node`、`python`、`go`，
由一层转发文件（shim）在启动那一刻看你在哪个文件夹，再决定交给哪个版本。项目里只需要一行 `.tool-versions`。

## 主要能力

- **版本托管**：从官方或国内镜像列版本、下压缩包、校验 SHA256、解压到位，全程有进度和取消。
- **断点续传与并发分片**：大文件下载中断不用从头再来，同一个包已下全就不重复下。
- **离线包导入**：内网机器拷来的官方压缩包，直接装成受管版本。
- **项目级版本**：识别 `.tool-versions`（与 asdf 同一份格式，`node 22.14.0` 一行即可），进目录自动生效。
- **全局锁定**：开启后所有目录都用你指定的那一版，项目里的版本声明暂不生效，适合团队统一口径。
- **PATH 接管与回滚**：改 PATH 前先备份，随时能退回去；需要管理员权限时会提权。
- **终端集成**：往 PowerShell / bash / zsh 的登录文件里写一段托管脚本，新开终端立刻认得命令入口，`cd` 之后还会播报当前目录用的是哪一版。
- **冲突检测**：认出 nvm、pyenv 以及散在各处的旧安装，可以接管也可以共存，不会悄悄删别人的东西。
- **生命周期提醒**：读 endoflife.date 的数据，告诉你某个版本已经停止维护。
- **首页体检**：一屏看存放位置、命令入口、终端集成、版本档案四类状态，能自动修的一键修。
- **AI 客户端支持**：一键检测本机的 Claude Code、Codex CLI、Cursor、Qoder 等客户端，把统一的运行时说明写进它们的规则文件。
- **设置导入导出**：换机器时把配置整体搬走。
- **四种界面语言**：简体中文、繁體中文、English、日本語，还有托盘菜单和窗口标题一起跟着切。

## 支持的运行时

| 运行时 | 版本来源 | 说明 |
| --- | --- | --- |
| Node.js | nodejs.org 官方 dist，可切 npmmirror 镜像 | 含 npm、npx 等附带命令的转发 |
| Python | python-build-standalone 的发布产物 | 可经 gh-proxy 之类的加速前缀下载；子导航里带 PyPy |
| Go | go.dev/dl，可切 golang.google.cn | 含 gofmt、go vet 一类命令的转发 |
| PyPy | downloads.python.org/pypy | 归在 Python 下面，按加速版单列 |

其他环境在计划中。

## 上手

1. 从官网下载安装包（正式发布后也能从 GitHub Releases 取），装上并打开。
2. 第一次启动会让你选一个存放运行时的文件夹。默认放在程序同级的 `envswitch-data`，
   也可以选某个非系统盘（例如 `D:\envswitch-data`）。建议避开 C 盘：一个版本动辄几百 MB。
3. 首页体检会告诉你命令入口有没有进 PATH、终端集成开没开，能一键处理的直接点。
4. 到"运行时"页选版本、点安装。装完后新开一个终端：

   ```powershell
   node -v
   python -V
   go version
   ```

5. 项目里要固定版本，就在项目根目录写 `.tool-versions`：

   ```text
   node 22.14.0
   python 3.12.11
   ```

   也可以在「项目级版本锁」页选上这个目录，对着想要的版本点「锁到这个」，效果一样。

## 命令行

图形界面和命令行共用同一份配置，转发文件本身就是一个命令行工具：

| 命令 | 作用 |
| --- | --- |
| `envswitch-shim` | 播报当前目录会用到哪个版本，适合给终端脚本调 |
| `envswitch-shim status` | 列出每个工具的当前版本、生效原因，以及 PATH 的真实排位 |
| `envswitch-shim use <工具> [版本]` | 在当前目录锁定版本，写进 `.tool-versions` |
| `envswitch-shim unuse <工具>` | 取消当前目录里某个工具的锁定 |
| `envswitch-shim global <工具> <版本>` | 设置全局默认版本 |
| `envswitch-shim shims sync` | 重新生成转发文件 |
| `envswitch-shim hooks [on|off]` | 查看或开关终端集成 |
| `envswitch-shim help` | 完整用法 |

`status` 里的 PATH 排位会分两条说：当前终端这一份才是这条命令真正会撞上的顺序，
注册表里那一份决定没开终端集成的新窗口认不认得命令入口。

## 数据放在哪

```text
envswitch-data/
  installs/     解压好的各个版本
  shims/        命令入口（node.exe、python.exe 等转发文件）
  cache/        下载缓存
  partial/      没下完的临时分片
  backups/      PATH 改动前的备份
  config/       设置与版本档案
  logs/         运行日志
  tmp/          解压中间产物，装完即清
```

想把它挪到别处：在设置里改存放位置，程序只会在 exe 旁边留一个几十字节的指路文件
`envswitch-data.location`。也可以给进程设环境变量 `ENVSWITCH_HOME` 直接指路。
候选位置按"程序同级、其他非系统盘、最后才是 C 盘公共目录"排序；非 Windows 上同级之后是 `~/.envswitch`。

平台说明：主要在 Windows 10/11 上开发和验证。Linux 与 macOS 的路径、PATH 落地（写 `~/.profile` 的托管片段，
全体用户走 `/etc/paths.d/`）都已实现，但还没逐平台实跑，遇到偏差请开 Issue。

## 下载源、镜像与代理

- 每个运行时都能在下载源页选官方源或国内镜像；填了"手动镜像根地址"就优先于列表，适合公司内网的私有镜像。
- 只能从 GitHub 拿的包（例如 Python 的独立构建）支持套加速前缀，形如 `https://gh-proxy.com/{url}`，把官方完整地址填进 `{url}`。
- 走公司代理的，在设置里填代理地址，列版本和检查更新都按这份设置走。
- 全部下载完成后按 SHA256 校验，校验不过不会解压，更不会覆盖已装好的版本。

## 关于界面语言

界面、托盘菜单和窗口标题跟随设置里的语言（简体中文 / 繁體中文 / English / 日本語，可跟随系统）。
有两处按约定保持中文，不做翻译：

- 命令行与终端集成的输出：那是终端里的文本，也是排查问题时要原样贴出来的东西。
- 程序内部报错里的文件路径与命令：翻了反而不好对着磁盘查。

## 开发

环境要求：Node.js（本项目在 24 上开发）、pnpm、Rust（edition 2024，用 `rustup` 装最新稳定版即可），Windows 上还需要 WebView2 运行时（Win10/11 一般自带）。

```bash
pnpm install
pnpm stage:shim    # 编出终端转发用的 envswitch-shim，开发和打包都靠它
pnpm tauri dev     # 开发运行，前端在 http://localhost:1420
pnpm check         # svelte-check 类型检查
pnpm tauri build   # 出安装包
```

发布由 `.github/workflows/release.yml` 负责：打上形如 `v0.1.0` 的标签，三个平台的安装包与 Windows 免安装包一起挂到 GitHub Release，正文取 `CHANGELOG.md` 里对应版本那一段。`pnpm tauri build` 会先跑 `scripts/stage-shim.mjs` 把转发用的 `envswitch-shim` 编好放进包里，少了它界面装得上但终端里的命令起不来。

技术栈：Tauri 2 + Svelte 5（runes）+ TypeScript + Tailwind CSS v4 + shadcn/ui 风格的组件层。
目录分工：`src/` 是界面，`src-tauri/src/core/` 是运行时托管、PATH、下载、shim 等所有实际干活的模块，
两边只通过 `src/lib/ipc.ts` 里声明的命令通信。Rust 只回事实（在不在、在哪个目录、版本号是多少），
所有界面文字都留在 `src/lib/i18n/` 的四份语言目录里。

## 常见问题

**装了版本，终端里还是原来的？**
新开一个终端再试；还不行就看 `envswitch-shim status` 里"当前终端"的排位。排在后面就会被系统里已有的同名命令抢先，
首页体检能直接把它挪到前面。

**想把 PATH 改回去？**
每次写 PATH 前都留了一份原始值，「环境与终端」页的 PATH 备份列表里点"退回这份"就能按任意一条退回（退回前会先把当前值再备份一次）。卸载前建议先退回，再删 `shims` 目录。

**项目里的 `.tool-versions` 没生效？**
检查是不是开了全局锁定：锁定生效时项目声明会被暂时忽略，体检首页会说明这一点。

**能不能两个版本共存？**
能。EnvSwitch 不删你已有的 Node/Python，只负责让自己管的版本排在前面。

## 许可证

本项目采用 **MIT 许可证 + Commons Clause** 组合，全文见 [LICENSE](LICENSE)。要点：

- 你可以自由使用、复制、修改、合并、发行、再授权，也可以把修改后的版本发布出去。
- 不许把本软件本身当商品卖，包括做成收费托管、收费咨询或打包交付。
- 分发时必须保留版权声明，修改过的要标明改了哪里，并在明显位置署名"灵匣 · EnvSwitch"（版权持有人：李初心）。
- 想把 EnvSwitch 用于商业销售，请先取得作者许可。

## 参与

问题与需求请开 Issue：<https://github.com/lichuxin90/LingXia_EnvSwitch/issues>
觉得有用，欢迎到仓库点一个 Star：<https://github.com/lichuxin90/LingXia_EnvSwitch>
