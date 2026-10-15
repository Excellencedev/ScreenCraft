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
  !define ICON_PATH "..\assets\icons\icon.ico"
!endif

Name "ScreenCraft"
OutFile "${OUT_FILE}"
InstallDir "$LOCALAPPDATA\Programs\ScreenCraft"
InstallDirRegKey HKCU "Software\ScreenCraft" "InstallDir"
RequestExecutionLevel user
SetCompressor /SOLID lzma
BrandingText "ScreenCraft ${VERSION}"

Icon "${ICON_PATH}"
UninstallIcon "${ICON_PATH}"
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

Section "ScreenCraft"
  SetOutPath "$INSTDIR"
  File "/oname=ScreenCraft.exe" "${EXE_PATH}"
  File "/oname=ScreenCraft.ico" "${ICON_PATH}"

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
