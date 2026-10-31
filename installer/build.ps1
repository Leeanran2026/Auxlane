# Build the Auxlane executable and NSIS installer.
# Usage: powershell -NoProfile -File installer\build.ps1
$ErrorActionPreference = "Stop"

$root = Split-Path -Parent $PSScriptRoot
Set-Location $root

cargo build --release
New-Item -ItemType Directory -Force -Path "target\installer" | Out-Null

$candidates = @(
    $env:MAKENSIS,
    (Get-Command makensis -ErrorAction SilentlyContinue | Select-Object -ExpandProperty Source -ErrorAction SilentlyContinue),
    "$env:LOCALAPPDATA\tauri\NSIS\makensis.exe",
    "$env:LOCALAPPDATA\tauri\NSIS\Bin\makensis.exe",
    "$env:LOCALAPPDATA\electron-builder\Cache\nsis-3.0.4.1\nsis-3.0.4.1-1mx3n\Bin\makensis.exe",
    "${env:ProgramFiles(x86)}\NSIS\makensis.exe",
    "$env:ProgramFiles\NSIS\makensis.exe"
) | Where-Object { $_ -and (Test-Path $_) }

if ($candidates.Count -eq 0) {
    throw "makensis.exe not found. Install NSIS or set the MAKENSIS environment variable."
}

$makensis = $candidates[0]
& $makensis "installer\auxlane.nsi"
if ($LASTEXITCODE -ne 0) {
    throw "makensis failed with exit code $LASTEXITCODE"
}
Write-Output "Installer written to target\installer"
