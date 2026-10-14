# Builds ScreenCraft for Windows and packs the installer.
#
# Needs, on a Windows 10/11 PC:
#   - Rust with the MSVC toolchain  (https://rustup.rs, and the "Desktop development
#     with C++" workload from the Visual Studio Build Tools)
#   - NSIS                          (winget install NSIS.NSIS)
#   - CMake                         (winget install Kitware.CMake) - builds whisper.cpp for captions
#   - LLVM (libclang)               (winget install LLVM.LLVM) - bindgen for whisper.cpp
#
# Usage (from the repository root, in PowerShell):
#   powershell -ExecutionPolicy Bypass -File scripts\build-windows.ps1
#   powershell -ExecutionPolicy Bypass -File scripts\build-windows.ps1 -Out ScreenCraft-Setup.exe
param([string]$Out = "ScreenCraft-Setup.exe")

$ErrorActionPreference = "Stop"
Set-Location (Join-Path $PSScriptRoot "..")

# Load the Visual Studio C++ environment (INCLUDE/LIB/PATH) so bindgen's clang finds stdbool.h
# and friends even when this isn't a "Developer PowerShell".
if (-not $env:INCLUDE) {
    $vswhere = Join-Path ${env:ProgramFiles(x86)} "Microsoft Visual Studio\Installer\vswhere.exe"
    if (Test-Path $vswhere) {
        $vs = & $vswhere -latest -products * -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath
        $vcvars = if ($vs) { Join-Path $vs "VC\Auxiliary\Build\vcvars64.bat" } else { $null }
        if ($vcvars -and (Test-Path $vcvars)) {
            cmd /c "`"$vcvars`" >nul && set" | ForEach-Object {
                if ($_ -match '^([^=]+)=(.*)$') { Set-Item -Path "env:$($Matches[1])" -Value $Matches[2] }
            }
        }
    }
}
if (-not $env:INCLUDE) { throw "Visual C++ environment not found - install the 'Desktop development with C++' workload" }

# CMake may be installed but not on this session's PATH yet.
if (-not (Get-Command cmake -ErrorAction SilentlyContinue)) {
    $cm = Join-Path $env:ProgramFiles "CMake\bin"
    if (Test-Path (Join-Path $cm "cmake.exe")) { $env:PATH = "$cm;$env:PATH" }
    else { throw "cmake not found - install it first (winget install Kitware.CMake), then open a new PowerShell" }
}

if (-not $env:LIBCLANG_PATH) {
    foreach ($dir in @("$env:ProgramFiles\LLVM\bin", "${env:ProgramFiles(x86)}\LLVM\bin")) {
        if (Test-Path (Join-Path $dir "libclang.dll")) { $env:LIBCLANG_PATH = $dir; break }
    }
}
if (-not $env:LIBCLANG_PATH) { throw "libclang not found - install LLVM first (winget install LLVM.LLVM)" }

# whisper.cpp comes from our fork, a git submodule under third_party/
git submodule update --init --recursive
if ($LASTEXITCODE -ne 0) { throw "git submodule update failed" }

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
