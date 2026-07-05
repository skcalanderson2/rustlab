# Build the RustLab Windows installer (MSI) and a portable zip.
#
# Run on a Windows machine from the repo root (or anywhere):
#   powershell -ExecutionPolicy Bypass -File packaging\windows\build-installer.ps1
#
# Prerequisites:
#   - Rust (rustup, MSVC toolchain): https://rustup.rs
#   - WiX Toolset v3.14: https://github.com/wixtoolset/wix3/releases
#     (candle.exe/light.exe on PATH, or WIX env var set — the installer does this)
#   - cargo-wix: installed automatically below if missing
#   - Optional for the smoke test: python with ipykernel installed
#     (pip install ipykernel && python -m ipykernel install --user)
$ErrorActionPreference = "Stop"

$Root = Resolve-Path (Join-Path $PSScriptRoot "..\..")
Set-Location $Root

# --- prerequisite checks -------------------------------------------------
if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) {
    throw "cargo not found - install Rust from https://rustup.rs"
}
$wixFound = (Get-Command candle.exe -ErrorAction SilentlyContinue) -or $env:WIX
if (-not $wixFound) {
    throw "WiX v3 not found - install from https://github.com/wixtoolset/wix3/releases"
}
if (-not (Get-Command cargo-wix -ErrorAction SilentlyContinue)) {
    Write-Host "==> Installing cargo-wix"
    cargo install cargo-wix
}

$Version = (Select-String -Path "Cargo.toml" -Pattern '^version = "(.+)"' |
    Select-Object -First 1).Matches[0].Groups[1].Value

# --- build ---------------------------------------------------------------
Write-Host "==> Building release binary"
cargo build --release

# --- smoke test (best effort: needs a python kernel installed) -----------
Write-Host "==> Headless kernel smoke test"
& "target\release\rustlab.exe" --headless-test 2>&1 | Tee-Object -Variable smoke
if ($LASTEXITCODE -ne 0) {
    Write-Warning "headless test failed (no python kernel installed?) - continuing"
}

# --- MSI -----------------------------------------------------------------
Write-Host "==> Building MSI with cargo-wix"
cargo wix --nocapture --no-build

New-Item -ItemType Directory -Force -Path "dist" | Out-Null
Copy-Item "target\wix\*.msi" "dist\"

# --- portable zip --------------------------------------------------------
Write-Host "==> Creating portable zip"
$ZipPath = "dist\RustLab-$Version-windows-x86_64.zip"
Compress-Archive -Force -Path "target\release\rustlab.exe", "LICENSE", "README.md" -DestinationPath $ZipPath

Write-Host ""
Write-Host "Done:"
Get-ChildItem dist\*.msi, dist\*.zip | ForEach-Object { Write-Host "  $($_.FullName)" }
Write-Host ""
Write-Host "Note: the MSI is unsigned - SmartScreen will show 'Windows protected"
Write-Host "your PC' on first run (More info -> Run anyway)."
