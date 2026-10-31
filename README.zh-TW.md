[简体中文](README.md) | **繁體中文** | [English](README.en.md)

# Auxlane — 藍牙音訊接收端

Auxlane 可以將一台 Windows 電腦變成已配對手機的藍牙喇叭。它是 Microsoft Store 上 UWP 應用程式 **Bluetooth Audio Receiver**（Mark Smirnov，v1.1.5）的淨室重寫，使用原生 **Slint + Rust** 桌面技術重新實作。

介面是 Material 3 儀表板，版面配置參考 [MicYou](https://github.com/LanRhyme/MicYou) 的桌面端設計。原始 UWP 二進位檔（.NET Native AOT）僅用於互通性分析，未使用其中任何程式碼。

## 為什麼要重寫

舊版 Tauri/WebView2 建置在閒置時約佔用 **375 MB 工作集 / 222 MB 私有記憶體**（7 個行程）。原生 Slint 建置以單一行程僅約 **37 MB 工作集 / 12 MB 私有記憶體**：

| 建置 | 行程數 | 工作集 | 私有記憶體 |
| --- | --- | --- | --- |
| Tauri / WebView2 | 7 | 375.7 MB | 222.5 MB |
| Tauri + 瀏覽器啟動參數 | 7 | 384.6 MB | 221.3 MB |
| Slint / femtovg | 1 | 80.4 MB | 108.4 MB |
| **Slint / 軟體轉譯（本建置）** | **1** | **36.6 MB** | **12.3 MB** |

應用程式強制使用 Slint 軟體轉譯器，只在內容變更時重繪，因此大部分時間靜止的儀表板開銷很低；串流時音訊環動畫限制為 20 fps。

## 功能

- 列出 Windows 所提供、可用於播放的藍牙音訊端點。
- 一鍵開啟/關閉 `AudioPlaybackConnection`；音訊透過電腦的預設喇叭播放。
- 裝置卡片最多顯示 **三台** 裝置。最多將三台裝置加入收藏以固定到卡片；卡片標題列的 **⋯** 按鈕會開啟 **更多裝置**，列出所有已配對端點。收藏會儲存在設定中。
- 背景重新掃描會就地更新清單；掃描面板只在首次載入時出現，因此 2 秒重新整理不會讓介面跳動。
- Material 3 儀表板，支援淺色/深色/跟隨系統主題、四種強調色，以及 24 條徑向音訊環。
- 支援英文、簡體中文與繁體中文；預設跟隨系統語言。CJK 文字優先要求 Microsoft YaHei，無法使用時由 Slint 退回其他已安裝的 CJK 字型。
- 設定：主題、強調色、語言、裝置重新整理間隔，以及最多三台收藏裝置。儲存在 `%APPDATA%\Auxlane\settings.json`。
- 單一執行個體：再次啟動會聚焦現有（可能已隱藏）的視窗。
- 系統匣提供顯示 / 中斷連線 / 結束。關閉視窗會隱藏至系統匣並保持 A2DP 串流；結束請使用系統匣功能表。
- 自訂無邊框標題列、**圓角視窗**、可調整大小、最小化/關閉，並保留 Windows 正常最大化/貼齊行為。視窗子類化讓用戶端區域覆蓋原生非用戶端區域（包括最大化時），因此不會繪製原生標題列；Windows 11 由 DWM 繪製圓角，Windows 10 退回圓角視窗區域。
- 結構化錯誤訊息，包含 Windows HRESULT（`0x...`）。
- 發佈日誌位於 `%LOCALAPPDATA%\Auxlane\logs\auxlane.log`。
- NSIS 安裝程式，按使用者安裝，無需系統管理員權限。

## 系統需求

- Windows 10 版本 2004（組建 10.0.19041）或更新版本。
- 藍牙適配器，以及已在 Windows 設定中配對的手機。
- 建置需要 Rust 1.92 或更新版本。
- 用於中文介面的系統 CJK 字型（優先要求 Microsoft YaHei；無法使用時使用 Microsoft JhengHei 或其他已安裝的 CJK 字型退回）。中文版 Windows 通常內建；英文版 Windows 可能需要新增中文語言/字型功能。

## 建置

```bash
cargo build --release
cargo run --release
cargo test
cargo fmt --check
```

可執行檔為 `target\release\auxlane.exe`。

開發時，`AUXLANE_SETTINGS` 可指定另一份設定檔，`AUXLANE_INSTANCE` 可指定獨立的單一執行個體互斥鎖，讓測試建置能與已安裝副本同時執行，且不共用視窗或設定。

## 安裝程式

先建置可執行檔，然後在儲存庫根目錄執行 NSIS：

```bash
makensis installer/auxlane.nsi
```

安裝程式輸出到 `target\installer\Auxlane_0.1.0_x64-setup.exe`，按使用者安裝到 `%LOCALAPPDATA%\Programs\Auxlane`。參見 [docs/RELEASING.md](docs/RELEASING.md)（英文）。

## 架構

```
src/
  main.rs      UI 狀態機、回呼、計時器、單一執行個體、記錄，
               Win32 視窗外觀（無邊框用戶端、圓角）
  audio.rs     WinRT 裝置列舉與 AudioPlaybackConnection 生命週期
  error.rs     結構化命令錯誤（代碼 + 可選 HRESULT）
  i18n.rs      英文 / 簡體中文 / 繁體中文詞典查找
  settings.rs  %APPDATA%\Auxlane\settings.json 持久化
  theme.rs     Material 3 調色盤、強調色預設、Windows 主題偵測
  system.rs    透過 ShellExecuteW 開啟 ms-settings:bluetooth
ui/
  app.slint    儀表板、設定覆蓋層、提示列、音訊環
  i18n.json    供 src/i18n.rs 使用的三套詞典
  icons/       Lucide SVG 圖示以及 Auxlane 音訊標記（audio.svg）
assets/
  auxlane.ico  安裝程式/可執行檔圖示
  icon.svg     品牌圖示來源檔
installer/
  auxlane.nsi  NSIS 按使用者安裝腳本
```

### 視窗外觀

Slint 轉譯無邊框視窗（`no-frame: true`），winit 以 `decorations(false)` 建立原生視窗。winit 的做法是保留原生邊框樣式（DWM 需要它們來實現動畫、貼齊與圓角），並在 `WM_NCCALCSIZE` 中隱藏非用戶端區域。Auxlane 安裝自己的 `SetWindowSubclass` 程序來接管 `WM_NCCALCSIZE`：用戶端區域覆蓋整個視窗；最大化時，用戶端保留視窗頂部的外擴部分（蓋住標題列），同時左右與底部對齊顯示器工作區。Windows 11 透過 `DWMWA_WINDOW_CORNER_PREFERENCE` 實現圓角；Windows 10 不支援該屬性，改用 `SetWindowRgn` 設定圓角區域。子類化程序在計時器第一次 tick 時安裝，因為 Slint 是在事件迴圈內而不是在 `show()` 期間建立 HWND 的。

### 連線生命週期

1. `TryCreateFromId` 建立裝置端點。
2. `StartAsync` 啟用端點以接收音訊。
3. `OpenAsync` 等待連線並傳回狀態。
4. 成功後，後端最多等待 5 秒，直到 `State == Opened` 才回報成功。
5. `StartAsync`（10 秒）、`OpenAsync`（30 秒）與裝置列舉（10 秒）都有期限；卡住的操作會被取消並回報為逾時。
6. 結束應用程式會關閉所有追蹤的連線（自訂 X 只是將視窗隱藏至系統匣）。`shutting_down` 旗標可防止仍在開啟的連線在最終清理後被插入。

UI 每 800 毫秒輪詢一次連線狀態，並每 2/3/5/10 秒重新掃描裝置（可設定）。第二個行程透過具名互斥鎖與 `FindWindowW` 聚焦現有視窗。

## 重建的 UWP 行為對應

| 原始 UWP 成員 | 原生替代 |
| --- | --- |
| `MainGrid_Loaded` 裝置監看器 | `audio::list_devices` 輪詢 |
| `DeviceWatcher_Added` / `Removed` | `refresh` 中的裝置清單差異更新 |
| `DeviceListView_SelectionChanged` | `select_device` |
| `OpenAudioPlaybackConnectionButtonButton_Click` | `connect` |
| `CloseAudioPlaybackConnectionButtonButton_Click` | `disconnect` |
| `AudioPlaybackConnection_ConnectionStateChanged` | 800 毫秒 `connection_is_open` 輪詢 |
| `setStateConnected/Disconnected/Connecting/Error` | `status-*` 屬性 + UI 狀態 |
| `audioPlaybackConnections` 字典 | `AppState.connections` |
| `audioPlaybackDeviceNames` 字典 | 裝置清單查找 |

## 音量行為

應用程式不呼叫任何音量 API，也不修改系統的藍牙原則。手機媒體音量與電腦輸出音量是否連動，由 Windows 的藍牙 **絕對音量**（AVRCP）原則或手機本身的媒體音量同步設定決定。應用程式保持系統/手機目前的行為不變。

## 說明

- 關閉視窗會將 Auxlane 隱藏至系統匣，並保持 A2DP 連線串流。若要結束，請使用系統匣功能表的 **結束**；再次啟動應用程式也會顯示隱藏的視窗。系統匣功能表可以在不開啟視窗的情況下中斷連線。
- 視窗遵循正常的 Windows 視窗行為：從任一邊緣調整大小、拖曳自訂標題列、最小化/還原，以及使用 Win+Up 或拖曳到螢幕頂端來最大化。這些狀態下自訂標題列都保持原位，不會繪製原生標題列。
- Windows 11 可能會將新的系統匣圖示放入隱藏圖示的溢出區（`^`）。將 Auxlane 圖示拖曳到工作列即可保持可見。
- 開啟失敗會包含 Windows HRESULT。`UnknownFailure` 且 `0x8007001F` 通常表示手機的 A2DP sink 端點遺失或已失效；請在 Windows 藍牙設定中移除並重新配對手機、重新連線手機，或重新開機後再試。
- 「新增裝置」會開啟 Windows 設定 → 藍牙；配對與 PIN 確認仍由 Windows 完成。
- 軟體轉譯器以 GPU 加速換取更低記憶體。音訊環只在串流時以 20 fps 動畫；其餘 UI 僅在屬性變更時重繪。
- 應用程式僅支援 Windows，目前建置為 x64。
- 安裝程式預設未簽署；在設定憑證前，SmartScreen 會顯示警告。參見 [docs/RELEASING.md](docs/RELEASING.md)（英文）。
- 尚無自動更新。

## 授權

Auxlane 採用 **GPL-3.0-only** 授權。參見 [LICENSE](LICENSE)。

原始 Bluetooth Audio Receiver 為專有軟體（Mark Smirnov），未包含在本專案中。介面版面參考 [MicYou](https://github.com/LanRhyme/MicYou)（GPL-3.0）作為設計參考；未包含 MicYou 原始碼。

## 致謝

- UI 工具包：[Slint](https://slint.dev/) 1.18，依 GPL-3.0 使用。
- 圖示：[Lucide](https://lucide.dev/)（透過 `lucide-static`），ISC 授權。參見 [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md)。
- 藍牙 A2DP sink：`Windows.Media.Audio.AudioPlaybackConnection`。
- 設計參考：[MicYou](https://github.com/LanRhyme/MicYou)。
