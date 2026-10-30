# LingXia · EnvSwitch

Node.js / Python / Go / PyPy on one machine, each installed once, and every folder picks the version it needs.

简体中文 | [繁體中文](README.zh-TW.md) | English | [日本語](README.ja.md)

Website: <https://envswitch.iamap.cn> · [Changelog](CHANGELOG.md) · Issues: <https://github.com/lichuxin90/LingXia_EnvSwitch/issues>

---

## Why this exists

Three Node installs and two Python installs on one box, and which one `node -v` runs depends on PATH order and luck;
switching projects means `nvm use`, switching a virtual environment means `source`, and an AI assistant writing code just follows the stale instructions committed in the repo.

EnvSwitch gathers all of this into one graphical interface: versions live under one shared folder layout, and you still type `node`, `python`, `go` in the terminal,
while a layer of forwarder files (shims) looks at which folder you are in at launch and decides which version answers. A project needs only one `.tool-versions` line.

## Key features

- **Version hosting**: lists versions from the official source or a Chinese mirror, downloads the archive, verifies SHA256, extracts it into place, with progress and cancel all the way through.
- **Resume and segmented download**: an interrupted large download continues where it stopped instead of starting over, and a package already downloaded in full is never fetched twice.
- **Offline package import**: an official archive copied over from a machine on a closed network installs directly as a managed version.
- **Project pin**: recognises `.tool-versions` (the same format asdf uses, one line such as `node 22.14.0` is enough) and applies it as soon as you enter the folder.
- **Global pin**: while it is on, every folder uses the version you set and the declarations inside projects are ignored for now, which suits a team that needs one agreed version.
- **PATH takeover and rollback**: PATH is backed up before it is written and can be rolled back at any time; administrator rights are requested through elevation when they are needed.
- **Terminal integration**: writes a managed snippet into the login files of PowerShell / bash / zsh, so a new terminal knows the command entry at once and `cd` announces which version the current folder uses.
- **Conflict detection**: finds nvm, pyenv and old installs scattered around the machine, and lets you take them over or keep living side by side; nothing you installed is deleted quietly.
- **Lifecycle notices**: reads the endoflife.date data and tells you when a version has stopped being maintained.
- **Home checkup**: one screen covers Storage, Command entry, Shell integration and Version records, and anything fixable automatically has a one-click fix.
- **AI client support**: detects the clients installed on the machine in one click, such as Claude Code, Codex CLI, Cursor and Qoder, and writes the shared runtime note into their rules files.
- **Config import and export**: move the whole configuration across when you change machines.
- **Four interface languages**: 简体中文, 繁體中文, English and 日本語, with the tray menu and the window title switching along.

## Supported runtimes

| Runtime | Version source | Notes |
| --- | --- | --- |
| Node.js | Official nodejs.org dist, switchable to the npmmirror mirror | Forwarding for the bundled commands such as npm and npx |
| Python | Release artifacts of python-build-standalone | Can be fetched through an acceleration prefix such as gh-proxy; PyPy sits under it in the sub-navigation |
| Go | go.dev/dl, switchable to golang.google.cn | Forwarding for commands such as gofmt and go vet |
| PyPy | downloads.python.org/pypy | Filed under Python, listed as its own accelerated branch |

Other runtimes are planned.

## Getting started

1. Download the installer from the website (once releases are out, GitHub Releases works too), install it and open it.
2. The first launch asks you to pick the folder that holds the runtimes. It defaults to `envswitch-data` next to the program,
   and you can choose a drive that is not the system drive (for example `D:\envswitch-data`). Keeping it off drive C is a good idea: one version is easily a few hundred MB.
3. The Home checkup tells you whether the command entry is in PATH and whether terminal integration is on; anything it can handle in one click is a button.
4. On the "Runtimes" page pick a version and press Install. When it finishes, open a new terminal:

   ```powershell
   node -v
   python -V
   go version
   ```

5. To hold a project to a version, write `.tool-versions` at the project root:

   ```text
   node 22.14.0
   python 3.12.11
   ```

   You can also pick this folder on the "Project-level version pin" page and press "Pin to this" on the version you want; the result is the same.

## Command line

The graphical interface and the command line share one configuration, and the forwarder files are themselves a command-line tool:

| Command | What it does |
| --- | --- |
| `envswitch-shim` | Announces which version the current folder would use, meant for calling from terminal scripts |
| `envswitch-shim status` | Lists the current version of each tool, why it took effect, and the real PATH positions |
| `envswitch-shim use <tool> [version]` | Pins the current folder to a version by writing it into `.tool-versions` |
| `envswitch-shim unuse <tool>` | Removes one tool's pin from the current folder |
| `envswitch-shim global <tool> <version>` | Sets the global default version |
| `envswitch-shim shims sync` | Regenerates the forwarder files |
| `envswitch-shim hooks [on|off]` | Shows or toggles terminal integration |
| `envswitch-shim help` | Full usage |

The PATH positions in `status` are reported on two lines: the current terminal's PATH is the order this command actually hits,
and the one in the registry decides whether a new window without terminal integration recognises the command entry.

## Where the data lives

```text
envswitch-data/
  installs/     Extracted versions
  shims/        Command entry (forwarder files such as node.exe and python.exe)
  cache/        Download cache
  partial/      Temporary pieces of unfinished downloads
  backups/      PATH backups taken before each change
  config/       Settings and version records
  logs/         Runtime logs
  tmp/          Intermediate extraction output, cleared as soon as an install finishes
```

To move it elsewhere: change the storage location in Settings, and the program leaves only a pointer file of a few dozen bytes next to the exe,
`envswitch-data.location`. You can also point the way directly by setting the environment variable `ENVSWITCH_HOME` for the process.
Candidate locations are ordered as next to the program, then other non-system drives, and only last a shared folder on drive C; on non-Windows systems, `~/.envswitch` comes right after the location next to the program.

Platform note: development and verification happen mainly on Windows 10/11. The Linux and macOS handling of paths and of PATH
(a managed snippet written to `~/.profile`, and `/etc/paths.d/` for all users) is implemented but has not been run through on each platform, so open an Issue when you hit a difference.

## Download sources, mirrors and proxy

- Every runtime can switch between its official source and a Chinese mirror on the "Download sources" page; when "Manual mirror root" is filled in it wins over the list, which suits a private mirror on the company network.
- Packages that only come from GitHub (for example the standalone Python builds) support an acceleration prefix, shaped like `https://gh-proxy.com/{url}`, where you put the full official address into `{url}`.
- For a machine behind the company proxy, set the proxy address in Settings; listing versions and checking for updates both follow it.
- Every download is verified by SHA256 once it completes. A failed check means no extraction, and certainly no overwrite of a version that is already installed.

## About interface language

The interface, the tray menu and the window title follow the language in Settings (简体中文 / 繁體中文 / English / 日本語, with an option to follow the system).
Two places stay in Chinese by convention and are not translated:

- The output of the command line and of terminal integration: that is text inside a terminal, and the text you paste verbatim when reporting a problem.
- The file paths and commands inside the program's own error messages: translating them makes them harder to match against what is on disk.

## Development

Requirements: Node.js (this project is developed on 24), pnpm, Rust (edition 2024, the latest stable release from `rustup` is enough), and on Windows also the WebView2 runtime (Win10/11 usually ships it).

```bash
pnpm install
pnpm tauri dev     # Development run, the frontend is at http://localhost:1420
pnpm check         # Type check with svelte-check
pnpm tauri build   # Produce the installer
```

Releases come from `.github/workflows/release.yml`: tag a version such as `v0.1.0` and the workflow attaches the installers for all three platforms, plus the Windows portable zip, to a GitHub Release whose body is the matching section of `CHANGELOG.md`. Before bundling, `pnpm tauri build` runs `scripts/stage-shim.mjs`, which compiles the `envswitch-shim` forwarder and puts it inside the package. Without it the app installs but `node`, `python` and `go` still do not start in a terminal.

Tech stack: Tauri 2 + Svelte 5 (runes) + TypeScript + Tailwind CSS v4 + a component layer in the shadcn/ui style.
Directory split: `src/` is the interface, and `src-tauri/src/core/` holds the modules that do the actual work, such as runtime hosting, PATH, download and shim,
while the two sides talk only through the commands declared in `src/lib/ipc.ts`. Rust returns facts (whether something exists, in which folder, what the version number is),
and every piece of interface text stays in the four language files under `src/lib/i18n/`.

## FAQ

**Installed a version, but the terminal still runs the old one?**
Open a new terminal and try again; if it still fails, look at the position on the 当前终端 ("current terminal") line of `envswitch-shim status`.
Sitting behind other entries means a command of the same name already on the system is found first, and the Home checkup can move ours to the front.

**Want PATH back the way it was?**
The original value is saved before every PATH write. In the PATH backups list on the "Environment & shell" page, press "Restore this one" on any entry to roll back to it (the current value is backed up once more before the restore). Before uninstalling, roll back first and then delete the `shims` folder.

**The `.tool-versions` in the project did not take effect?**
Check whether the global pin is on; while it is, project declarations are ignored for now, and the checkup on Home says so.

**Can two versions coexist?**
Yes. EnvSwitch does not delete the Node or Python you already have; it only makes the versions it manages come first.

## License

This project uses the **MIT License + Commons Clause** combination; the full text is in [LICENSE](LICENSE). The essentials:

- You are free to use, copy, modify, merge, publish, redistribute, sublicense this software, and to release modified versions of it.
- You may not sell this software itself as a product, including as paid hosting, paid consulting or packaged delivery.
- When you redistribute it, keep the copyright notice, mark what you changed and where, and show the attribution "灵匣 · EnvSwitch" in a prominent place.
- To use EnvSwitch in a commercial sale, get the author's permission first.

## Contributing

Bugs and feature requests go to the Issue tracker: <https://github.com/lichuxin90/LingXia_EnvSwitch/issues>
If this saves you time, a Star on the repository is welcome: <https://github.com/lichuxin90/LingXia_EnvSwitch>
