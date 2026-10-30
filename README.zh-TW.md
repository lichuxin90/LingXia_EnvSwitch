# 灵匣 · EnvSwitch

一台電腦上的 Node.js / Python / Go / PyPy，只裝一份，誰用誰挑對的那版。

[简体中文](README.md) | 繁體中文 | [English](README.en.md) | [日本語](README.ja.md)

官網：<https://envswitch.iamap.cn> · [更新日誌](CHANGELOG.md) · 問題回報：<https://github.com/lichuxin90/LingXia_EnvSwitch/issues>

---

## 為什麼做這個

裝了三個 Node、兩個 Python，命令列裡 `node -v` 到底用哪個，全看 PATH 順序撞運氣；
切專案要 `nvm use`、換虛擬環境要 `source`，AI 助手寫程式碼時又只會照著倉庫裡的舊說明走。

EnvSwitch 把這些收進一個圖形介面：版本按統一的目錄規則存放，終端機裡仍然直接輸入 `node`、`python`、`go`，
由一層轉送檔案（shim）在啟動那一刻看你在哪個資料夾，再決定交給哪個版本。專案裡只需要一行 `.tool-versions`。

## 主要能力

- **版本託管**：從官方來源或國內鏡像列出版本、下載壓縮包、核對 SHA256、解壓縮到位，全程有進度與取消。
- **中斷續傳與分片並連**：大檔案下載中斷不用從頭再來，同一個套件已下全就不重複下載。
- **離線套件匯入**：內網機器拷來的官方壓縮包，直接裝成託管版本。
- **專案層級版本**：辨識 `.tool-versions`（與 asdf 同一份格式，`node 22.14.0` 一行即可），進入目錄自動生效。
- **全域鎖定**：開啟後所有目錄都用你指定的那一版，專案裡的版本宣告暫不生效，適合團隊統一口徑。
- **PATH 接管與回滾**：改 PATH 前先備份，隨時能退回去；需要管理員權限時會跳一次授權視窗。
- **終端機整合**：往 PowerShell / bash / zsh 的登入設定檔裡寫一段託管腳本，新開終端機立刻認得命令入口，`cd` 之後還會播報目前目錄用的是哪一版。
- **衝突偵測**：認出 nvm、pyenv 以及散在各處的舊安裝，可以接管也可以共存，不會悄悄刪別人的東西。
- **生命週期提醒**：讀 endoflife.date 的資料，告訴你某個版本已經停止更新。
- **首頁檢查**：一屏看齊存放位置、命令入口、終端整合、版本記錄四類狀態，能自動修的一鍵修。
- **AI 用戶端支援**：一鍵偵測本機的 Claude Code、Codex CLI、Cursor、Qoder 等用戶端，把統一的執行時說明寫進它們的規則檔案。
- **設定匯入匯出**：換電腦時把設定整體搬走。
- **四種介面語言**：简体中文、繁體中文、English、日本語，還有系統匣選單和視窗標題一起跟著切。

## 支援的執行時

| 執行時 | 版本來源 | 說明 |
| --- | --- | --- |
| Node.js | nodejs.org 官方 dist，可切到 npmmirror 鏡像 | 含 npm、npx 等隨附命令的轉送 |
| Python | python-build-standalone 的發布產物 | 可經 gh-proxy 之類的加速前綴下載；子導覽裡有 PyPy |
| Go | go.dev/dl，可切到 golang.google.cn | 含 gofmt、go vet 一類命令的轉送 |
| PyPy | downloads.python.org/pypy | 歸在 Python 下面，按加速版單列 |

其他環境規劃中。

## 上手

1. 從官網下載安裝包（正式發布後也能從 GitHub Releases 取），裝上並打開。
2. 第一次啟動會讓你選一個存放執行時的資料夾。預設放在程式同級的 `envswitch-data`，
   也可以選某個非系統碟（例如 `D:\envswitch-data`）。建議避開 C 槽：一個版本動輒幾百 MB。
3. 首頁檢查會告訴你命令入口有沒有進 PATH、終端機整合開沒開，能一鍵處理的直接點。
4. 到「執行時」頁選版本、點安裝。裝完後新開一個終端機：

   ```powershell
   node -v
   python -V
   go version
   ```

5. 專案裡要固定版本，就在專案根目錄寫 `.tool-versions`：

   ```text
   node 22.14.0
   python 3.12.11
   ```

   也可以在「專案層級的版本鎖定」頁選上這個目錄，對著想要的版本點「鎖到這個」，效果一樣。

## 命令列

圖形介面和命令列共用同一份設定，轉送檔案本身就是一個命令列工具：

| 命令 | 作用 |
| --- | --- |
| `envswitch-shim` | 播報目前目錄會用到哪個版本，適合給終端機腳本呼叫 |
| `envswitch-shim status` | 列出每個工具的目前版本、生效原因，以及 PATH 的真實排位 |
| `envswitch-shim use <工具> [版本]` | 在目前目錄鎖定版本，寫進 `.tool-versions` |
| `envswitch-shim unuse <工具>` | 取消目前目錄裡某個工具的鎖定 |
| `envswitch-shim global <工具> <版本>` | 設定全域預設版本 |
| `envswitch-shim shims sync` | 重新產生轉送檔案 |
| `envswitch-shim hooks [on|off]` | 檢視或開關終端機整合 |
| `envswitch-shim help` | 完整用法 |

`status` 裡的 PATH 排位會分兩條來說：目前終端機這一份才是這條命令真正會撞上的順序，
註冊表裡那一份決定沒開終端機整合的新視窗認不認得命令入口。

## 資料放在哪

```text
envswitch-data/
  installs/     解壓縮好的各個版本
  shims/        命令入口（node.exe、python.exe 等轉送檔案）
  cache/        下載快取
  partial/      沒下完的暫時分片
  backups/      PATH 改動前的備份
  config/       設定與版本記錄
  logs/         執行記錄
  tmp/          解壓縮中間產物，裝完即清
```

想把它挪到別處：在設定裡改存放位置，程式只會在 exe 旁邊留一個幾十字節的指路檔案
`envswitch-data.location`。也可以給處理程序設環境變數 `ENVSWITCH_HOME` 直接指路。
候選位置按「程式同級、其他非系統碟、最後才是 C 槽的共用目錄」排序；非 Windows 上同級之後是 `~/.envswitch`。

平台說明：主要在 Windows 10/11 上開發和驗證。Linux 與 macOS 的路徑、PATH 落地（寫 `~/.profile` 的託管片段，
全體使用者走 `/etc/paths.d/`）都已實作，但還沒逐平台實跑，遇到偏差請開 Issue。

## 下載來源、鏡像與代理

- 每個執行時都能在「下載來源」頁選官方來源或國內鏡像；填了「手動鏡像根位址」就優先於清單，適合公司內網的私有鏡像。
- 只能從 GitHub 拿的套件（例如 Python 的獨立建置）可以套上加速前綴，形如 `https://gh-proxy.com/{url}`，把官方完整位址填進 `{url}`。
- 走公司代理的，在設定裡填代理位址，列出版本和檢查更新都依這份設定走。
- 全部下載完成後依 SHA256 核對，核對不過不解壓縮，更不會覆蓋已裝好的版本。

## 關於介面語言

介面、系統匣選單和視窗標題跟著設定裡的語言走（简体中文 / 繁體中文 / English / 日本語，可跟隨系統）。
有兩處按約定保持中文，不做翻譯：

- 命令列與終端機整合的輸出：那是終端機裡的文字，也是排查問題時要原樣貼出來的東西。
- 程式內部錯誤裡的路徑與命令：翻了反而不好對著磁碟查。

## 開發

環境需求：Node.js（本專案在 24 上開發）、pnpm、Rust（edition 2024，用 `rustup` 裝最新穩定版即可），Windows 上還需要 WebView2 執行環境（Win10/11 一般內建）。

```bash
pnpm install
pnpm stage:shim    # 編出終端轉發用的 envswitch-shim，開發與打包都靠它
pnpm tauri dev     # 開發執行，前端在 http://localhost:1420
pnpm check         # svelte-check 型別檢查
pnpm tauri build   # 產出安裝包
```

發布由 `.github/workflows/release.yml` 負責：打上形如 `v0.1.0` 的標籤，三個平台的安裝包與 Windows 免安裝包一起掛到 GitHub Release，正文取 `CHANGELOG.md` 裡對應版本那一段。`pnpm tauri build` 打包前會先跑 `scripts/stage-shim.mjs`，把轉發用的 `envswitch-shim` 編好放進包裡；少了它，介面裝得上，終端裡的 `node`、`python`、`go` 卻起不來。

技術棧：Tauri 2 + Svelte 5（runes）+ TypeScript + Tailwind CSS v4 + shadcn/ui 風格的元件層。
目錄分工：`src/` 是介面，`src-tauri/src/core/` 是執行時託管、PATH、下載、shim 等所有實際做事的模組，
兩邊只透過 `src/lib/ipc.ts` 裡宣告的命令通訊。Rust 只回事實（在不在、在哪個目錄、版本號是多少），
所有介面文字都留在 `src/lib/i18n/` 的四份語言目錄裡。

## 常見問題

**裝了版本，終端機裡還是原來的？**
新開一個終端機再試；還是不行就看 `envswitch-shim status` 裡「当前终端」那一行的排位。排在後面就會被系統裡已有的同名命令搶先，
首頁檢查能直接把它挪到前面。

**想把 PATH 改回去？**
每次寫 PATH 前都留了一份原始值，「環境與終端機」頁的 PATH 備份清單裡點「退回這份」就能依任意一條退回（退回前會先把目前值再備份一次）。解除安裝前建議先退回，再刪 `shims` 目錄。

**專案裡的 `.tool-versions` 沒生效？**
檢查是不是開了全域鎖定：鎖定生效時專案宣告會被暫時忽略，首頁檢查會說明這一點。

**能不能兩個版本共存？**
能。EnvSwitch 不刪你已有的 Node/Python，只負責讓自己託管的版本排在前面。

## 授權條款

本專案採用 **MIT 授權條款 + Commons Clause** 組合，全文見 [LICENSE](LICENSE)。要點：

- 你可以自由使用、複製、修改、合併、出版、發行、再授權，也可以把修改後的版本發布出去。
- 不許把本軟體本身當商品賣，包括做成收費託管、收費諮詢或打包交付。
- 分發時必須保留著作權聲明，修改過的要標明改了哪裡，並在明顯位置署名「灵匣 · EnvSwitch」。
- 想把 EnvSwitch 用於商業銷售，請先取得作者許可。

## 參與

問題與需求請開 Issue：<https://github.com/lichuxin90/LingXia_EnvSwitch/issues>
覺得有幫助，歡迎到倉庫點一個 Star：<https://github.com/lichuxin90/LingXia_EnvSwitch>
