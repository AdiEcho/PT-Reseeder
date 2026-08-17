# PT-Reseeder Windows GUI build script
# Run from the repository root:  .\scripts\build-gui.ps1
# Prerequisites: Rust, cargo, Node.js, npm, git

$ErrorActionPreference = "Stop"
Set-Location $PSScriptRoot\..

Write-Host "=== Step 1/4: Install build tools ===" -ForegroundColor Cyan
$tauriCli = cargo install --list | Select-String "tauri-cli"
if (-not $tauriCli) { cargo install tauri-cli --version "^2" }

Write-Host "=== Step 2/4: Generate Windows icon ===" -ForegroundColor Cyan
$icoPath = "crates\desktop\icons\icon.ico"
if (-not (Test-Path $icoPath)) {
    Write-Host "icon.ico not found, generating from icon.png..."
    cargo tauri icon crates\desktop\icons\icon.png -o crates\desktop\icons
    git checkout -- crates\desktop\icons\icon.png 2>$null
    $junk = @(
        "crates\desktop\icons\android",
        "crates\desktop\icons\ios",
        "crates\desktop\icons\Square*Logo.png",
        "crates\desktop\icons\StoreLogo.png",
        "crates\desktop\icons\32x32.png",
        "crates\desktop\icons\64x64.png",
        "crates\desktop\icons\128x128.png",
        "crates\desktop\icons\128x128@2x.png",
        "crates\desktop\icons\icon.icns"
    )
    Remove-Item -Recurse -Force $junk -ErrorAction SilentlyContinue
    Write-Host "icon.ico generated" -ForegroundColor Green
} else {
    Write-Host "$icoPath already exists, skip generation" -ForegroundColor Green
}

Write-Host "=== Step 3/4: Build frontend + server ===" -ForegroundColor Cyan
npm --prefix web ci
npm --prefix web run build
cargo build --release --features headless-browser -p pt-reseeder-server

Write-Host "=== Step 4/4: Build Tauri desktop bundle ===" -ForegroundColor Cyan
Push-Location crates\desktop
try {
    cargo tauri build --bundles nsis
} finally {
    Pop-Location
}

Write-Host ""
Write-Host "=== Build complete ===" -ForegroundColor Green
Write-Host "Installer:   target\release\bundle\nsis\PT-Reseeder_0.1.0_x64-setup.exe"
Write-Host "Standalone:  target\release\pt-reseeder-desktop.exe"
