**简体中文** | [繁體中文](README.zh-TW.md) | [English](README.en.md)

# Auxlane — 蓝牙音频接收端

Auxlane 可以把一台 Windows 电脑变成已配对手机的蓝牙音箱。它是 Microsoft Store 上 UWP 应用 **Bluetooth Audio Receiver**（Mark Smirnov，v1.1.5）的净室重实现，使用原生 **Slint + Rust** 桌面技术重写。

界面是 Material 3 仪表盘，布局参考 [MicYou](https://github.com/LanRhyme/MicYou) 的桌面端设计。原 UWP 二进制（.NET Native AOT）仅用于互操作性分析，未使用其中任何代码。

## 功能

- 列出 Windows 暴露的可用于播放的蓝牙音频端点。
- 一键打开/关闭 `AudioPlaybackConnection`；音频通过电脑的默认扬声器播放。
- 设备卡片最多显示 **三台** 设备。最多给三台设备加星标以固定到卡片；卡片标题栏的 **⋯** 按钮会打开 **更多设备**，列出所有已配对端点。收藏会保存在设置中。
- 后台重新扫描会原地更新列表；扫描面板只在首次加载时出现，因此 2 秒刷新不会让界面跳动。
- Material 3 仪表盘，支持浅色/深色/跟随系统主题、四种强调色，以及 24 条径向音频环。
- 支持英语、简体中文和繁体中文；默认跟随系统语言。CJK 文本优先请求 Microsoft YaHei，不可用时由 Slint 回退到其他已安装的 CJK 字体。
- 设置：主题、强调色、语言、设备刷新间隔和最多三台收藏设备。保存在 `%APPDATA%\Auxlane\settings.json`。
- 单实例：再次启动会聚焦已有（可能已隐藏）的窗口。
- 系统托盘提供显示 / 断开连接 / 退出。关闭窗口会隐藏到托盘并保持 A2DP 串流；退出请使用托盘菜单。
- 自定义无边框标题栏、**圆角窗口**、可调整大小、最小化/关闭，并保留 Windows 正常最大化/贴靠行为。窗口子类化让客户区覆盖原生非客户区（包括最大化时），因此不会绘制原生标题栏；Windows 11 由 DWM 绘制圆角，Windows 10 回退到圆角窗口区域。
- 结构化错误信息，包含 Windows HRESULT（`0x...`）。
- 发布日志位于 `%LOCALAPPDATA%\Auxlane\logs\auxlane.log`。
- NSIS 安装包，按用户安装，无需管理员权限。

## 系统要求

- Windows 10 版本 2004（内部版本 10.0.19041）或更高。
- 蓝牙适配器，以及已在 Windows 设置中配对的手机。
- 构建需要 Rust 1.92 或更高版本。
- 用于中文界面的系统 CJK 字体（优先请求 Microsoft YaHei；不可用时使用 Microsoft JhengHei 或其他已安装的 CJK 字体回退）。中文版 Windows 通常自带；英文版 Windows 可能需要添加中文语言/字体功能。

## 构建

```bash
cargo build --release
cargo run --release
cargo test
cargo fmt --check
```

可执行文件为 `target\release\auxlane.exe`。

开发时，`AUXLANE_SETTINGS` 可指定另一份设置文件，`AUXLANE_INSTANCE` 可指定独立的单实例互斥量，从而让测试构建与已安装副本同时运行，且不共享窗口或设置。

## 安装包

先构建可执行文件，然后在仓库根目录运行 NSIS：

```bash
makensis installer/auxlane.nsi
```

安装包输出到 `target\installer\Auxlane_0.1.0_x64-setup.exe`，按用户安装到 `%LOCALAPPDATA%\Programs\Auxlane`。参见 [docs/RELEASING.md](docs/RELEASING.md)（英文）。

## 架构

```
src/
  main.rs      UI 状态机、回调、定时器、单实例、日志，
               Win32 窗口外观（无边框客户区、圆角）
  audio.rs     WinRT 设备枚举与 AudioPlaybackConnection 生命周期
  error.rs     结构化命令错误（代码 + 可选 HRESULT）
  i18n.rs      英语 / 简体中文 / 繁体中文词典查找
  settings.rs  %APPDATA%\Auxlane\settings.json 持久化
  theme.rs     Material 3 调色板、强调色预设、Windows 主题检测
  system.rs    通过 ShellExecuteW 打开 ms-settings:bluetooth
ui/
  app.slint    仪表盘、设置浮层、提示条、音频环
  i18n.json    供 src/i18n.rs 使用的三套词典
  icons/       Lucide SVG 图标以及 Auxlane 音频标记（audio.svg）
assets/
  auxlane.ico  安装包/可执行文件图标
  icon.svg     品牌图标源文件
installer/
  auxlane.nsi  NSIS 按用户安装脚本
```

### 窗口外观

Slint 渲染无边框窗口（`no-frame: true`），winit 以 `decorations(false)` 创建原生窗口。winit 的实现方式是保留原生边框样式（DWM 需要它们来实现动画、贴靠和圆角），并在 `WM_NCCALCSIZE` 中隐藏非客户区。Auxlane 安装自己的 `SetWindowSubclass` 过程来接管 `WM_NCCALCSIZE`：客户区覆盖整个窗口；最大化时，客户区保留窗口顶部的外扩部分（盖住标题栏），同时左右和底部对齐显示器工作区。Windows 11 通过 `DWMWA_WINDOW_CORNER_PREFERENCE` 实现圆角；Windows 10 不支持该属性，改用 `SetWindowRgn` 设置圆角区域。子类化过程在定时器第一次 tick 时安装，因为 Slint 是在事件循环内而不是在 `show()` 期间创建 HWND 的。

### 连接生命周期

1. `TryCreateFromId` 创建设备端点。
2. `StartAsync` 启用端点以接收音频。
3. `OpenAsync` 等待连接并返回状态。
4. 成功后，后端最多等待 5 秒，直到 `State == Opened` 才报告成功。
5. `StartAsync`（10 秒）、`OpenAsync`（30 秒）和设备枚举（10 秒）都有超时；卡住的操作会被取消并报告为超时。
6. 退出应用会关闭所有跟踪的连接（自定义 X 只是把窗口隐藏到托盘）。`shutting_down` 标志防止仍在打开的连接在最终清理后被插入。

UI 每 800 毫秒轮询一次连接状态，并每 2/3/5/10 秒重新扫描设备（可配置）。第二个进程通过命名互斥量和 `FindWindowW` 聚焦已有窗口。

## 重建的 UWP 行为映射

| 原 UWP 成员 | 原生替代 |
| --- | --- |
| `MainGrid_Loaded` 设备监视器 | `audio::list_devices` 轮询 |
| `DeviceWatcher_Added` / `Removed` | `refresh` 中的设备列表差异更新 |
| `DeviceListView_SelectionChanged` | `select_device` |
| `OpenAudioPlaybackConnectionButtonButton_Click` | `connect` |
| `CloseAudioPlaybackConnectionButtonButton_Click` | `disconnect` |
| `AudioPlaybackConnection_ConnectionStateChanged` | 800 毫秒 `connection_is_open` 轮询 |
| `setStateConnected/Disconnected/Connecting/Error` | `status-*` 属性 + UI 状态 |
| `audioPlaybackConnections` 字典 | `AppState.connections` |
| `audioPlaybackDeviceNames` 字典 | 设备列表查找 |

## 音量行为

应用不调用任何音量 API，也不修改系统的蓝牙策略。手机媒体音量与电脑输出音量是否联动，由 Windows 的蓝牙 **绝对音量**（AVRCP）策略或手机自身的媒体音量同步设置决定。应用保持系统/手机当前的行为不变。

## 说明

- 关闭窗口会把 Auxlane 隐藏到系统托盘，并保持 A2DP 连接串流。要退出请使用托盘菜单的 **退出**；再次启动应用也会显示隐藏的窗口。托盘菜单可以在不打开窗口的情况下断开连接。
- 窗口遵循正常的 Windows 窗口行为：从任意边缘调整大小、拖动自定义标题栏、最小化/还原，以及使用 Win+Up 或拖到屏幕顶部来最大化。这些状态下自定义标题栏都保持原位，不会绘制原生标题栏。
- Windows 11 可能会把新托盘图标放进隐藏图标溢出区（`^`）。把 Auxlane 图标拖到任务栏即可保持可见。
- 打开失败会包含 Windows HRESULT。`UnknownFailure` 且 `0x8007001F` 通常表示手机的 A2DP sink 端点缺失或已失效；请在 Windows 蓝牙设置中删除并重新配对手机、重新连接手机，或重启后重试。
- “添加设备”会打开 Windows 设置 → 蓝牙；配对和 PIN 确认仍由 Windows 完成。
- 软件渲染器用 GPU 加速换取更低内存。音频环只在串流时以 20 fps 动画；其余 UI 仅在属性变化时重绘。
- 应用仅支持 Windows，目前构建为 x64。
- 安装包默认未签名；在配置证书前，SmartScreen 会提示警告。参见 [docs/RELEASING.md](docs/RELEASING.md)（英文）。
- 暂无自动更新。

## 许可证

Auxlane 采用 **GPL-3.0-only** 许可证。参见 [LICENSE](LICENSE)。

原始 Bluetooth Audio Receiver 为专有软件（Mark Smirnov），未包含在本项目中。界面布局参考 [MicYou](https://github.com/LanRhyme/MicYou)（GPL-3.0）作为设计参考；未包含 MicYou 源代码。

## 致谢

- UI 工具包：[Slint](https://slint.dev/) 1.18，按 GPL-3.0 使用。
- 图标：[Lucide](https://lucide.dev/)（通过 `lucide-static`），ISC 许可。参见 [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md)。
- 蓝牙 A2DP sink：`Windows.Media.Audio.AudioPlaybackConnection`。
- 设计参考：[MicYou](https://github.com/LanRhyme/MicYou)。
