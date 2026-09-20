# Deadlock Turniere: Rust-Backend mit expliziter zentraler TOML starten.
# Secrets müssen über die bestehende geschützte Infisical-Anbindung vorliegen.
param([string]$Config = (Join-Path $PSScriptRoot "config\bot.toml"))
$ErrorActionPreference = "Stop"

$ConfigFile = (Resolve-Path -LiteralPath $Config).Path
$RustDir = Join-Path $PSScriptRoot "rust"
$Binary = Join-Path $RustDir "target\release\turnier-bot.exe"

if (-not (Test-Path -LiteralPath $Binary)) {
    Write-Host "Baue Rust-Backend."
    & cargo build --manifest-path (Join-Path $RustDir "Cargo.toml") --release -p turnier-bot -j 2
    if ($LASTEXITCODE -ne 0) { throw "Rust-Build fehlgeschlagen." }
}

# Keine Verzeichnisse oder Clients vor der Konfigurationsprüfung anlegen.
& $Binary --config $ConfigFile --check-config
if ($LASTEXITCODE -ne 0) { throw "Konfigurationsprüfung fehlgeschlagen. Kein Start." }

Write-Host "Starte Deadlock Turniere mit der geprüften Konfiguration."
& $Binary --config $ConfigFile
exit $LASTEXITCODE
