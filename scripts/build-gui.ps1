# PT-Reseeder Windows GUI build script
# Run from the repository root:  .\scripts\build-gui.ps1
# Prerequisites: Rust, cargo, git

$ErrorActionPreference = "Stop"
Set-Location $PSScriptRoot\..

Write-Host "=== Step 1/5: Install build tools ===" -ForegroundColor Cyan
rustup target add wasm32-unknown-unknown
$cargoLeptos = cargo install --list | Select-String "cargo-leptos"
if (-not $cargoLeptos) { cargo install cargo-leptos }
$tauriCli = cargo install --list | Select-String "tauri-cli"
if (-not $tauriCli) { cargo install tauri-cli --version "^2" }

Write-Host "=== Step 2/5: Generate Windows icon ===" -ForegroundColor Cyan
$icoPath = "crates\desktop\icons\icon.ico"
if (-not (Test-Path $icoPath)) {
    Write-Host "icon.ico not found, generating from icon.png..."
    cargo tauri icon crates\desktop\icons\icon.png -o crates\desktop\icons
    # cargo tauri icon rewrites icon.png and generates android/ios/uwp artifacts.
    # Restore the original icon.png and keep only the .ico.
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

Write-Host "=== Step 3/5: Build server + frontend site ===" -ForegroundColor Cyan
cargo leptos build --release

Write-Host "=== Step 4/5: Copy frontend artifacts ===" -ForegroundColor Cyan
Copy-Item crates\frontend\index.html target\site\index.html -Force

$pkgDir = "target\site\pkg"
$wasm = Join-Path $pkgDir "pt-reseeder.wasm"
$bgWasm = Join-Path $pkgDir "pt-reseeder_bg.wasm"
if ((Test-Path $wasm) -and -not (Test-Path $bgWasm)) {
    Copy-Item $wasm $bgWasm -Force
    Write-Host "Created $bgWasm" -ForegroundColor Green
} elseif (Test-Path $bgWasm) {
    Write-Host "$bgWasm already exists, skip copy" -ForegroundColor Green
} else {
    Write-Error "$pkgDir missing pt-reseeder.wasm — cargo-leptos build may have failed"
}

Write-Host "=== Step 5/5: Build Tauri desktop bundle ===" -ForegroundColor Cyan
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
