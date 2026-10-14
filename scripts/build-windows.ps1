# Builds ScreenCraft for Windows and packs the installer.
#
# Needs, on a Windows 10/11 PC:
#   - Rust with the MSVC toolchain  (https://rustup.rs, and the "Desktop development
#     with C++" workload from the Visual Studio Build Tools)
#   - NSIS                          (winget install NSIS.NSIS)
#   - CMake                         (winget install Kitware.CMake) - builds whisper.cpp for captions
#
# Usage (from the repository root, in PowerShell):
#   powershell -ExecutionPolicy Bypass -File scripts\build-windows.ps1
#   powershell -ExecutionPolicy Bypass -File scripts\build-windows.ps1 -Out ScreenCraft-Setup.exe
param([string]$Out = "ScreenCraft-Setup.exe")

$ErrorActionPreference = "Stop"
Set-Location (Join-Path $PSScriptRoot "..")

cargo build --release -p screencraft
if ($LASTEXITCODE -ne 0) { throw "cargo build failed" }

$conf = Get-Content src-tauri\tauri.conf.json -Raw | ConvertFrom-Json
$version = $conf.version

$makensis = (Get-Command makensis -ErrorAction SilentlyContinue).Source
if (-not $makensis) {
    foreach ($dir in @($env:ProgramFiles, ${env:ProgramFiles(x86)})) {
        $candidate = Join-Path $dir "NSIS\makensis.exe"
        if (Test-Path $candidate) { $makensis = $candidate; break }
    }
}
if (-not $makensis) { throw "makensis not found - install NSIS first (winget install NSIS.NSIS)" }

$exe = (Resolve-Path "target\release\screencraft.exe").Path
$ico = (Resolve-Path "src-tauri\icons\icon.ico").Path
$outPath = Join-Path (Get-Location) $Out

& $makensis "/DVERSION=$version" "/DEXE_PATH=$exe" "/DICON_PATH=$ico" "/DOUT_FILE=$outPath" "scripts\installer.nsi"
if ($LASTEXITCODE -ne 0) { throw "makensis failed" }

Write-Host ""
Write-Host "Built $outPath"
Write-Host "SHA-256: $((Get-FileHash $outPath -Algorithm SHA256).Hash)"
