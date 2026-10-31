[简体中文](README.md) | [繁體中文](README.zh-TW.md) | **English**

# Auxlane — Bluetooth audio receiver

Auxlane turns a Windows PC into a Bluetooth speaker for a paired phone. It is
a clean-room reimplementation of the Microsoft Store UWP app **Bluetooth Audio
Receiver** (Mark Smirnov, v1.1.5), rebuilt as a native **Slint + Rust** desktop
application.

The UI is a Material 3 dashboard inspired by
[MicYou](https://github.com/LanRhyme/MicYou)'s desktop layout. The original
UWP binary (.NET Native AOT) was inspected for interoperability only; no code
from it is used.

## Why this build exists

The previous Tauri/WebView2 build used about **375 MB working set / 222 MB
private memory** while idle (seven processes). The native Slint build uses
about **37 MB working set / 12 MB private memory**, in a single process:

| Build | Processes | Working set | Private |
| --- | --- | --- | --- |
| Tauri / WebView2 | 7 | 375.7 MB | 222.5 MB |
| Tauri + browser flags | 7 | 384.6 MB | 221.3 MB |
| Slint / femtovg | 1 | 80.4 MB | 108.4 MB |
| **Slint / software (this build)** | **1** | **36.6 MB** | **12.3 MB** |

The app forces Slint's software renderer. It redraws only when something
changes, so a mostly static dashboard stays cheap; the audio ring animation
is limited to 20 fps while streaming.

## Features

- Lists Bluetooth audio endpoints Windows exposes for playback.
- One click to open/close an `AudioPlaybackConnection`; audio plays through the
  PC's default speakers.
- The Devices card shows at most **three** devices. Star up to three devices
  to pin them to the card; the **⋯** button in the card header opens **More
  devices** for every paired endpoint. Favorites are stored in settings.
- Background rescans update the list in place; the scanning panel only appears
  on the first load, so a 2-second refresh does not make the UI jump.
- Material 3 dashboard with light/dark/system themes, four accent colors, and
  a 24-bar radial audio ring.
- English, Simplified Chinese, and Traditional Chinese; follows the system
  language by default. Requests Microsoft YaHei for CJK text and lets Slint
  fall back to another installed CJK font when it is unavailable.
- Settings: theme, accent, language, device refresh interval, and up to three
  favorite devices. Stored in `%APPDATA%\Auxlane\settings.json`.
- Single instance: launching again focuses the existing (possibly hidden)
  window.
- System tray with Show / Disconnect / Quit. Closing the window hides it to the
  tray and keeps the A2DP connection streaming; use the tray menu to quit.
- Custom frameless title bar, **rounded window corners**, resizable window,
  minimize/close, and normal Windows maximize/snap behavior. A window subclass
  keeps the client area over the native non-client area (including while
  maximized), so no native title bar is ever drawn; Windows 11 rounds the
  corners with DWM, Windows 10 falls back to a rounded window region.
- Structured error messages with Windows HRESULTs (`0x...`).
- Release logging in `%LOCALAPPDATA%\Auxlane\logs\auxlane.log`.
- NSIS installer, per-user, no admin required.

## Requirements

- Windows 10 version 2004 (build 10.0.19041) or later.
- A Bluetooth adapter and a phone paired in Windows Settings.
- Rust 1.92 or newer to build.
- A system CJK font for Chinese UI text (Microsoft YaHei is requested;
  Microsoft JhengHei or another installed CJK font is used as a fallback).
  Chinese Windows installations include these; English installs may need to
  add the Chinese language/font feature.

## Build

```bash
cargo build --release
cargo run --release
cargo test
cargo fmt --check
```

The executable is `target\release\auxlane.exe`.

For development, `AUXLANE_SETTINGS` selects an alternate settings file and
`AUXLANE_INSTANCE` selects a separate single-instance mutex, so a test build
can run alongside an installed copy without sharing its window or settings.

## Installer

Build the executable first, then run NSIS from the repository root:

```bash
makensis installer/auxlane.nsi
```

The installer is written to `target\installer\Auxlane_0.1.0_x64-setup.exe` and
installs per-user to `%LOCALAPPDATA%\Programs\Auxlane`. See
[docs/RELEASING.md](docs/RELEASING.md).

## Architecture

```
src/
  main.rs      UI state machine, callbacks, timers, single instance, logging,
               Win32 window chrome (frameless client, rounded corners)
  audio.rs     WinRT device enumeration and AudioPlaybackConnection lifecycle
  error.rs     Structured command errors (code + optional HRESULT)
  i18n.rs      English / Simplified / Traditional Chinese dictionary lookup
  settings.rs  %APPDATA%\Auxlane\settings.json persistence
  theme.rs     Material 3 palette, accent presets, Windows theme detection
  system.rs    Opens ms-settings:bluetooth through ShellExecuteW
ui/
  app.slint    Dashboard, settings overlay, snackbar, audio ring
  i18n.json    Three dictionaries used by src/i18n.rs
  icons/       Lucide SVG icons plus the Auxlane audio mark (audio.svg)
assets/
  auxlane.ico  Installer/executable icon
  icon.svg     Brand icon source
installer/
  auxlane.nsi  NSIS per-user installer
```

### Window chrome

Slint renders a frameless window (`no-frame: true`) and winit creates the
native window with `decorations(false)`. winit implements that by keeping the
native frame styles (DWM uses them for animations, snapping and rounded
corners) and hiding the non-client area from `WM_NCCALCSIZE`. Auxlane installs
its own `SetWindowSubclass` procedure that owns `WM_NCCALCSIZE`: the client
area covers the whole window, and when the window is maximized the client
keeps the window's top overhang (covering the caption) while aligning with the
monitor work area at the sides and bottom. Windows 11 performs corner rounding
through `DWMWA_WINDOW_CORNER_PREFERENCE`; on Windows 10, where that attribute
is unavailable, a rounded `SetWindowRgn` region is used instead. The subclass
is installed from the timer's first tick because Slint creates the HWND inside
the event loop rather than during `show()`.

### Connection lifecycle

1. `TryCreateFromId` creates the endpoint.
2. `StartAsync` enables it for incoming audio.
3. `OpenAsync` waits for the connection and returns a status.
4. After success, the backend waits up to 5 seconds for
   `State == Opened` before reporting success.
5. `StartAsync` (10 s), `OpenAsync` (30 s), and device enumeration (10 s) have
   deadlines; a stuck operation is cancelled and reported as a timeout.
6. Quitting the app closes every tracked connection (the custom X only hides
   the window to the tray). A `shutting_down` flag prevents a connection that
   is still opening from being inserted after the final drain.

The UI polls the connection state every 800 ms and rescans devices every
2/3/5/10 seconds (configurable). A second process focuses the existing window
via a named mutex and `FindWindowW`.

## Reconstructed UWP behavior mapping

| Original UWP member | Native replacement |
| --- | --- |
| `MainGrid_Loaded` device watcher | `audio::list_devices` polling |
| `DeviceWatcher_Added` / `Removed` | Device list diffing in `refresh` |
| `DeviceListView_SelectionChanged` | `select_device` |
| `OpenAudioPlaybackConnectionButtonButton_Click` | `connect` |
| `CloseAudioPlaybackConnectionButtonButton_Click` | `disconnect` |
| `AudioPlaybackConnection_ConnectionStateChanged` | 800 ms `connection_is_open` poll |
| `setStateConnected/Disconnected/Connecting/Error` | `status-*` properties + UI state |
| `audioPlaybackConnections` dictionary | `AppState.connections` |
| `audioPlaybackDeviceNames` dictionary | device list lookup |

## Volume behavior

The app does not call any volume API and does not change the system's
Bluetooth policy. Whether the phone's media volume and the PC output volume
move together is controlled by Windows' Bluetooth **Absolute Volume** (AVRCP)
policy, or by the phone's own media-volume-sync setting. The app preserves
whatever the system/phone currently does.

## Notes

- Closing the window hides Auxlane to the system tray and keeps the A2DP
  connection streaming. Use the tray menu's **Quit** item to exit; launching
  the app again also reveals the hidden window. The tray menu can disconnect
  without opening the window.
- The window follows normal Windows window behavior: resize from any edge,
  drag it by the custom header, minimize/restore, and maximize with Win+Up or
  by dragging it to the top edge. The custom header stays in place and no
  native title bar is drawn in any of these states.
- Windows 11 may place a new tray icon in the hidden-icons overflow (`^`).
  Drag the Auxlane icon onto the taskbar to keep it visible.
- Failed opens include the Windows HRESULT. `UnknownFailure` with
  `0x8007001F` usually means the phone's A2DP sink endpoint is missing or
  stale; remove/re-pair the phone in Windows Bluetooth settings, reconnect the
  phone, or reboot before retrying.
- "Add device" opens Windows Settings → Bluetooth; pairing and PIN
  confirmation stay with Windows.
- The software renderer trades GPU acceleration for memory. The audio ring
  animates at 20 fps only while streaming; the rest of the UI redraws on
  property changes only.
- The app is Windows-only and currently built for x64.
- The installer is unsigned by default; SmartScreen will warn until a
  certificate is configured. See [docs/RELEASING.md](docs/RELEASING.md).
- No auto-update yet.

## License

Auxlane is licensed under **GPL-3.0-only**. See [LICENSE](LICENSE).

The original Bluetooth Audio Receiver is proprietary (Mark Smirnov) and is not
included. The UI layout is modeled after
[MicYou](https://github.com/LanRhyme/MicYou) (GPL-3.0) as a design reference;
no MicYou source code is included.

## Credits

- UI toolkit: [Slint](https://slint.dev/) 1.18, used under GPL-3.0.
- Icons: [Lucide](https://lucide.dev/) via `lucide-static`, ISC licensed. See
  [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md).
- Bluetooth A2DP sink: `Windows.Media.Audio.AudioPlaybackConnection`.
- Design reference: [MicYou](https://github.com/LanRhyme/MicYou).
