# Deadlock Turniere — Backend Server starten
$ErrorActionPreference = "Stop"

$ProjectDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$BackendDir = Join-Path $ProjectDir "backend"
$VenvDir = Join-Path $ProjectDir "venv"
$DataDir = Join-Path $BackendDir "data"

# Data-Verzeichnis erstellen falls nötig
if (-not (Test-Path $DataDir)) {
    New-Item -ItemType Directory -Path $DataDir -Force | Out-Null
    Write-Host "Data-Verzeichnis erstellt: $DataDir"
}

# Venv erstellen falls nötig
if (-not (Test-Path "$VenvDir\Scripts\activate.ps1")) {
    Write-Host "Erstelle Python Virtual Environment..."
    python -m venv $VenvDir
    & "$VenvDir\Scripts\pip.exe" install -r "$BackendDir\requirements.txt"
    Write-Host "Dependencies installiert."
}

# Starten
Write-Host "Starte Deadlock Turniere Backend auf Port 8900..."
Set-Location $BackendDir
& "$VenvDir\Scripts\python.exe" -m uvicorn main:app --host 127.0.0.1 --port 8900 --reload
