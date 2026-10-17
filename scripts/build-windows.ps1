# Build a self-contained Windows x64 .exe of Rust Image Viewer + zip it.
#
# Run this ON A WINDOWS MACHINE (or in GitHub Actions). It builds an optimized
# release binary with the MSVC toolchain, copies the C/C++ runtime DLLs next to
# the exe so it runs on a bare install, and archives the result into ./dist/.
#
# Usage:
#   powershell -ExecutionPolicy Bypass -File .\scripts\build-windows.ps1
#
# Output (in .\dist\):
#   rust-image-viewer-win-x64\rust-image-viewer.exe   + the runtime DLLs
#   rust-image-viewer-win-x64.zip
#
# Notes:
#   * No VC++ Redistributable: with the default MSVC target the exe imports the
#     `api-ms-win-crt-*` contracts that ship with Windows 10+, AND deluan/deploy
#     copies the MSVC+UCRT DLLs too, so the folder is fully self-contained on both
#     Windows 10+ and older releases (Windows 7/8).
#   * The window/taskbar icon is embedded at runtime (src/main.rs decodes
#     assets/AppIcon.png), so no .ico sidecar is needed in the package.
#   * eframe/egui uses the wgpu backend; on Windows wgpu-hal dynamically loads
#     d3d12.dll at runtime, so D3D12 is not a static import of the exe.
$ErrorActionPreference = "Stop"
Set-Location (Split-Path -Parent $PSScriptRoot)   # project root

$BIN = "rust-image-viewer"
$VER = (Select-String -Path Cargo.toml -Pattern '^version\s*=\s*"([^"]+)"' |
        ForEach-Object { $_.Matches.Groups[1].Value } | Select-Object -First 1)
$VER = if ($VER) { $VER } else { "0.1.0" }
$PKG  = "dist\$BIN-win-x64"
$ZIP  = "dist\$BIN-win-x64.zip"

Write-Host ">> cargo build --release (MSVC, target x86_64-pc-windows-msvc)"
cargo build --release

# ---- Bundle the runtime (so the exe runs without the VC++ Redistributable) ----
# Prefer deluan/deploy (matches .github/workflows/release.yml). Fall back to
# warning-only if it can't be installed, so the exe still gets packaged.
$deployExe = if (Get-Command deploy -ErrorAction SilentlyContinue) { "deploy" }
             elseif (Test-Path "$env:USERPROFILE\.cargo\bin\deploy.exe") { "$env:USERPROFILE\.cargo\bin\deploy.exe" }
             else { $null }

if (-not $deployExe) {
    Write-Host ">> installing deluan/deploy to make the folder self-contained..."
    cargo install deploy 2>&1 | Out-Null
    if (Test-Path "$env:USERPROFILE\.cargo\bin\deploy.exe") {
        $deployExe = "$env:USERPROFILE\.cargo\bin\deploy.exe"
    }
}

# ---- Assemble the self-contained folder -------------------------------------
New-Item -ItemType Directory -Force $PKG | Out-Null
Copy-Item "target\release\$BIN.exe" "$PKG\$BIN.exe" -Force

if ($deployExe -and (Test-Path "target\release")) {
    Write-Host ">> $deployExe : copying MSVC/UCRT runtime DLLs next to the exe"
    & $deployExe -d "target\release" -b $PKG --strip
    # deploy may target the release dir; mirror any DLLs it produced into $PKG.
    Get-ChildItem "target\release" -Filter *.dll -ErrorAction SilentlyContinue |
        ForEach-Object { Copy-Item $_.FullName $PKG -Force }
} else {
    Write-Warning "deluan/deploy unavailable -> the exe needs the Microsoft Visual C++ " +
        "Redistributable (x64) installed on the target machine."
}

# ---- Package ----------------------------------------------------------------
if (Test-Path $ZIP) { Remove-Item $ZIP }
Compress-Archive -Path $PKG\* -DestinationPath $ZIP

Write-Host ""
Write-Host "Built (self-contained): $PKG\$BIN.exe"
Write-Host "Zip:                     $ZIP  ($VER)"
Write-Host "Run: $PKG\$BIN.exe  (optionally pass an image or folder path as arg 1)"
