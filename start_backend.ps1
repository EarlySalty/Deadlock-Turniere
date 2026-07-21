# Deadlock Turniere — Rust Backend starten
$ErrorActionPreference = "Stop"

$ProjectDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$BackendDir = Join-Path $ProjectDir "backend"
$RustDir = Join-Path $ProjectDir "rust"
$Binary = Join-Path $RustDir "target\release\turnier-bot.exe"
$DataDir = Join-Path $BackendDir "data"

# Data-Verzeichnis erstellen falls nötig
if (-not (Test-Path $DataDir)) {
    New-Item -ItemType Directory -Path $DataDir -Force | Out-Null
    Write-Host "Data-Verzeichnis erstellt: $DataDir"
}

if (-not (Test-Path $Binary)) {
    Write-Host "Baue Rust Backend..."
    Set-Location $RustDir
    cargo build --release -p turnier-bot
}

# Starten
Write-Host "Starte Deadlock Turniere Backend auf Port 8900..."
Set-Location $BackendDir
& $Binary
