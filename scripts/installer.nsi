; ScreenCraft Windows installer (NSIS). Normally built through scripts\build-windows.ps1,
; which is equivalent to:
;   makensis /DVERSION=0.1.0 /DEXE_PATH=<screencraft.exe> /DICON_PATH=<icon.ico> /DOUT_FILE=<out.exe> scripts\installer.nsi
; Installs per user (no administrator rights) into %LOCALAPPDATA%\Programs\ScreenCraft.

Unicode true
!include "MUI2.nsh"
!include "LogicLib.nsh"

!ifndef VERSION
  !define VERSION "0.1.0"
!endif
!ifndef EXE_PATH
  !define EXE_PATH "..\target\release\screencraft.exe"
!endif
!ifndef OUT_FILE
  !define OUT_FILE "ScreenCraft-Setup.exe"
!endif
!ifndef ICON_PATH
  !define ICON_PATH "..\src-tauri\icons\icon.ico"
!endif

Name "ScreenCraft"
OutFile "${OUT_FILE}"
InstallDir "$LOCALAPPDATA\Programs\ScreenCraft"
InstallDirRegKey HKCU "Software\ScreenCraft" "InstallDir"
RequestExecutionLevel user
SetCompressor /SOLID lzma
BrandingText "ScreenCraft ${VERSION}"

!define MUI_ICON "${ICON_PATH}"
!define MUI_UNICON "${ICON_PATH}"
!define MUI_ABORTWARNING
!define MUI_FINISHPAGE_RUN "$INSTDIR\ScreenCraft.exe"
!define MUI_FINISHPAGE_RUN_TEXT "Start ScreenCraft"

!insertmacro MUI_PAGE_WELCOME
!insertmacro MUI_PAGE_DIRECTORY
!insertmacro MUI_PAGE_INSTFILES
!insertmacro MUI_PAGE_FINISH
!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_INSTFILES
!insertmacro MUI_LANGUAGE "English"

VIProductVersion "${VERSION}.0"
VIAddVersionKey "ProductName" "ScreenCraft"
VIAddVersionKey "FileDescription" "ScreenCraft installer"
VIAddVersionKey "FileVersion" "${VERSION}"
VIAddVersionKey "LegalCopyright" "MIT License"

; The app draws its UI with the Microsoft Edge WebView2 runtime, which ships with
; Windows 11 and current Windows 10. If it is missing, run Microsoft's own bootstrapper.
Function EnsureWebView2
  ReadRegStr $0 HKLM "SOFTWARE\WOW6432Node\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}" "pv"
  ${If} $0 != ""
    Return
  ${EndIf}
  ReadRegStr $0 HKCU "Software\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}" "pv"
  ${If} $0 != ""
    Return
  ${EndIf}
  DetailPrint "Installing the WebView2 runtime..."
  nsExec::ExecToLog 'powershell -NoProfile -ExecutionPolicy Bypass -Command "Invoke-WebRequest -UseBasicParsing https://go.microsoft.com/fwlink/p/?LinkId=2124703 -OutFile $\'$TEMP\MicrosoftEdgeWebview2Setup.exe$\'"'
  ExecWait '"$TEMP\MicrosoftEdgeWebview2Setup.exe" /silent /install'
  Delete "$TEMP\MicrosoftEdgeWebview2Setup.exe"
FunctionEnd

Section "ScreenCraft"
  SetOutPath "$INSTDIR"
  File "/oname=ScreenCraft.exe" "${EXE_PATH}"
  File "/oname=ScreenCraft.ico" "${ICON_PATH}"
  Call EnsureWebView2

  WriteUninstaller "$INSTDIR\Uninstall.exe"
  CreateShortCut "$SMPROGRAMS\ScreenCraft.lnk" "$INSTDIR\ScreenCraft.exe" "" "$INSTDIR\ScreenCraft.ico"
  CreateShortCut "$DESKTOP\ScreenCraft.lnk" "$INSTDIR\ScreenCraft.exe" "" "$INSTDIR\ScreenCraft.ico"

  WriteRegStr HKCU "Software\ScreenCraft" "InstallDir" "$INSTDIR"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\ScreenCraft" "DisplayName" "ScreenCraft"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\ScreenCraft" "DisplayVersion" "${VERSION}"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\ScreenCraft" "DisplayIcon" "$INSTDIR\ScreenCraft.ico"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\ScreenCraft" "UninstallString" '"$INSTDIR\Uninstall.exe"'
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\ScreenCraft" "InstallLocation" "$INSTDIR"
  WriteRegDWORD HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\ScreenCraft" "NoModify" 1
  WriteRegDWORD HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\ScreenCraft" "NoRepair" 1
SectionEnd

Section "Uninstall"
  Delete "$INSTDIR\ScreenCraft.exe"
  Delete "$INSTDIR\ScreenCraft.ico"
  Delete "$INSTDIR\Uninstall.exe"
  RMDir "$INSTDIR"
  Delete "$SMPROGRAMS\ScreenCraft.lnk"
  Delete "$DESKTOP\ScreenCraft.lnk"
  DeleteRegKey HKCU "Software\ScreenCraft"
  DeleteRegKey HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\ScreenCraft"
  ; recordings in %USERPROFILE%\ScreenCraft Projects are left untouched
SectionEnd
