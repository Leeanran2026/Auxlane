!include "MUI2.nsh"

; NSIS resolves relative paths against the script directory (installer/),
; so ".." points at the repository root.
Unicode true

!define APP_NAME "Auxlane"
!define APP_VERSION "0.1.0"
!define APP_PUBLISHER "Auxlane contributors"
!define APP_EXE "auxlane.exe"
!define APP_ID "Auxlane"
!define UNINST_KEY "Software\Microsoft\Windows\CurrentVersion\Uninstall\${APP_ID}"

Name "${APP_NAME}"
OutFile "..\target\installer\Auxlane_${APP_VERSION}_x64-setup.exe"
InstallDir "$LOCALAPPDATA\Programs\Auxlane"
InstallDirRegKey HKCU "Software\Auxlane" "InstallDir"
RequestExecutionLevel user
SetCompressor /SOLID lzma

!define MUI_ABORTWARNING
!define MUI_ICON "..\assets\auxlane.ico"
!define MUI_UNICON "..\assets\auxlane.ico"
!define MUI_FINISHPAGE_RUN "$INSTDIR\${APP_EXE}"
!define MUI_FINISHPAGE_RUN_TEXT "Launch Auxlane"

!insertmacro MUI_PAGE_WELCOME
!insertmacro MUI_PAGE_LICENSE "..\LICENSE"
!insertmacro MUI_PAGE_DIRECTORY
!insertmacro MUI_PAGE_INSTFILES
!insertmacro MUI_PAGE_FINISH

!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_INSTFILES

!insertmacro MUI_LANGUAGE "English"
!insertmacro MUI_LANGUAGE "SimpChinese"
!insertmacro MUI_LANGUAGE "TradChinese"

VIProductVersion "0.1.0.0"
VIAddVersionKey "ProductName" "${APP_NAME}"
VIAddVersionKey "FileDescription" "Auxlane Bluetooth audio receiver"
VIAddVersionKey "FileVersion" "${APP_VERSION}"
VIAddVersionKey "ProductVersion" "${APP_VERSION}"
VIAddVersionKey "LegalCopyright" "GPL-3.0-only"
VIAddVersionKey "Publisher" "${APP_PUBLISHER}"

Section "Install"
  SetShellVarContext current
  SetOutPath "$INSTDIR"
  File "..\target\release\${APP_EXE}"
  File "..\LICENSE"
  WriteUninstaller "$INSTDIR\uninstall.exe"

  CreateDirectory "$SMPROGRAMS\Auxlane"
  CreateShortcut "$SMPROGRAMS\Auxlane\Auxlane.lnk" "$INSTDIR\${APP_EXE}"
  CreateShortcut "$SMPROGRAMS\Auxlane\Uninstall Auxlane.lnk" "$INSTDIR\uninstall.exe"
  CreateShortcut "$DESKTOP\Auxlane.lnk" "$INSTDIR\${APP_EXE}"

  WriteRegStr HKCU "Software\Auxlane" "InstallDir" "$INSTDIR"
  WriteRegStr HKCU "${UNINST_KEY}" "DisplayName" "${APP_NAME}"
  WriteRegStr HKCU "${UNINST_KEY}" "DisplayVersion" "${APP_VERSION}"
  WriteRegStr HKCU "${UNINST_KEY}" "Publisher" "${APP_PUBLISHER}"
  WriteRegStr HKCU "${UNINST_KEY}" "DisplayIcon" "$INSTDIR\${APP_EXE}"
  WriteRegStr HKCU "${UNINST_KEY}" "UninstallString" "$INSTDIR\uninstall.exe"
  WriteRegStr HKCU "${UNINST_KEY}" "QuietUninstallString" "$INSTDIR\uninstall.exe /S"
  WriteRegStr HKCU "${UNINST_KEY}" "InstallLocation" "$INSTDIR"
  WriteRegDWORD HKCU "${UNINST_KEY}" "NoModify" 1
  WriteRegDWORD HKCU "${UNINST_KEY}" "NoRepair" 1
  WriteRegDWORD HKCU "${UNINST_KEY}" "EstimatedSize" 30000
SectionEnd

Section "Uninstall"
  SetShellVarContext current
  Delete "$INSTDIR\${APP_EXE}"
  Delete "$INSTDIR\LICENSE"
  Delete "$INSTDIR\uninstall.exe"
  Delete "$SMPROGRAMS\Auxlane\Auxlane.lnk"
  Delete "$SMPROGRAMS\Auxlane\Uninstall Auxlane.lnk"
  RMDir "$SMPROGRAMS\Auxlane"
  Delete "$DESKTOP\Auxlane.lnk"
  RMDir "$INSTDIR"
  DeleteRegKey HKCU "${UNINST_KEY}"
  DeleteRegKey HKCU "Software\Auxlane"
SectionEnd
