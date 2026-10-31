# Releasing Auxlane

This checklist keeps the version, executable and installer in sync.

## 1. Bump the version

| File | Field |
| --- | --- |
| `Cargo.toml` | `[package] version` |
| `src/main.rs` | `APP_VERSION` (shown in Settings → About) |
| `build.rs` | `FileVersion` / `ProductVersion` resources |
| `installer/auxlane.nsi` | `VIProductVersion` / `FileVersion` / `ProductVersion` |
| `README.md`, `README.zh-TW.md`, `README.en.md` | Memory table/build examples if they mention a version |

Keep them identical. `Cargo.lock` updates on the next build.

## 2. Run the checks

```bash
cargo fmt --check
cargo test
cargo build --release
```

The Rust tests cover error-code mapping, the audio open-status mapping, i18n
dictionary parity and placeholder preservation, settings sanitization and the
favorites cap, the Devices-card visibility rules, and locale mapping.

## 3. Build the installer

The app must be built first (`cargo build --release`). Then run NSIS from the
repository root:

```bash
makensis installer/auxlane.nsi
```

The script installs per-user into `%LOCALAPPDATA%\Programs\Auxlane`, creates
Start Menu and desktop shortcuts, and registers an uninstaller under
`HKCU\Software\Microsoft\Windows\CurrentVersion\Uninstall\Auxlane`.

Output: `target\installer\Auxlane_<version>_x64-setup.exe`.

Keep the installer outside the source tree (for example in a local
`release/` folder that is not committed) and record its SHA-256:

```powershell
Get-FileHash .\release\Auxlane_<version>_x64-setup.exe -Algorithm SHA256
```

## 4. Code signing (not configured yet)

The installer and executable are unsigned; Windows SmartScreen will warn on
first run. To sign, obtain an Authenticode certificate and run:

```powershell
signtool sign /fd SHA256 /tr http://timestamp.digicert.com /td SHA256 `
  /f path\to\certificate.pfx /p $env:CERT_PASSWORD `
  .\release\Auxlane_<version>_x64-setup.exe
```

Verify with `Get-AuthenticodeSignature`. Keep the certificate and password out
of the repository; use encrypted CI secrets.

## 5. Smoke test

1. Install the new build as a standard user (no elevation).
2. Launch it twice: the second launch should focus the first window and exit.
3. Pair a phone in Windows Settings, refresh the Devices card, connect, and
   confirm audio plays through the default output.
4. Disconnect from the row or the central button.
5. With more than three paired devices, confirm the card shows three; open
   **More devices** with the ⋯ button, star up to three devices, and confirm
   favorites appear first on the card. Starring a fourth device should show
   the limit message. Restart and confirm favorites persist.
6. Switch all three languages, both themes, and all four accents; restart and
   confirm the settings persist.
7. Resize the window from an edge, drag it by the custom title bar, minimize
   it, then maximize it with **Win+Up** (or drag it to the top edge) and
   restore it. The custom header must fill the client area with no native
   title bar at any point, including while maximized.
8. With audio streaming, close the window with the custom X. The window should
   hide to the system tray, the process should stay alive, and audio should
   keep playing. Launch the app again to reveal the window, then use the tray
   menu (right-click) to test Show, Disconnect, and Quit. Quit should exit the
   process and release the A2DP connection.
9. Check `%APPDATA%\Auxlane\settings.json` and
   `%LOCALAPPDATA%\Auxlane\logs\auxlane.log`.
10. Run the uninstaller; confirm shortcuts and the install directory are
    removed (user settings and logs are intentionally kept).

Corner rounding is provided by DWM on Windows 11 and by the app's rounded
window region on Windows 10; both paths are exercised automatically, but run
this smoke test on the oldest supported Windows 10 build when the window chrome
changes.

## 6. Publish

- Attach the installer and its SHA-256 to the GitHub release.
- Include the GPL-3.0 source (the repository tag) with every binary release.
- Mention the Windows 10 2004+ requirement and the system CJK font requirement
  for Chinese UI text.
- Do not publish `reference/`, `_tools/`, `target/`, or any extracted original
  Store package.
