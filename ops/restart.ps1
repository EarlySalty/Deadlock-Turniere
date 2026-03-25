# ============================================================================
# Deadlock Tournament Platform — Zentrales Service-Management Script
# ============================================================================
# Verwaltet alle Platform-Services (via NSSM) zentral.
# Nutzung:
#   .\ops\restart.ps1                        # Alle Services neustarten
#   .\ops\restart.ps1 -Service backend       # Nur Backend neustarten
#   .\ops\restart.ps1 -Service steam -Action stop
#   .\ops\restart.ps1 -Action status         # Status aller Services
# ============================================================================

param(
    [Parameter(Position = 0)]
    [ValidateSet("all", "backend", "steam", "caddy", "discord")]
    [string]$Service = "all",

    [Parameter(Position = 1)]
    [ValidateSet("restart", "start", "stop", "status")]
    [string]$Action = "restart"
)

# --- Konfiguration -----------------------------------------------------------

$NSSM = "C:\ProgramData\chocolatey\bin\nssm.exe"

# Service-Mapping: Alias → Windows-Servicename
$ServiceMap = [ordered]@{
    "backend" = "DeadlockTurniere"
    "steam"   = "DeadlockSteamBot"
    "caddy"   = "Caddy"
    "discord" = "DeadlockBot"
}

# Start-Reihenfolge (Abhängigkeiten zuerst)
$StartOrder = @("caddy", "discord", "steam", "backend")

# Stop-Reihenfolge (Backend zuerst, dann Rest)
$StopOrder = @("backend", "steam", "discord", "caddy")

# Health-Check Endpoint
$HealthUrl = "http://127.0.0.1:8900/api/health"

# --- Hilfsfunktionen ---------------------------------------------------------

function Write-Info  ([string]$Msg) { Write-Host "[INFO]  $Msg" -ForegroundColor Cyan }
function Write-Ok    ([string]$Msg) { Write-Host "[OK]    $Msg" -ForegroundColor Green }
function Write-Warn  ([string]$Msg) { Write-Host "[WARN]  $Msg" -ForegroundColor Yellow }
function Write-Err   ([string]$Msg) { Write-Host "[FEHLER] $Msg" -ForegroundColor Red }

function Write-Header ([string]$Msg) {
    $line = "=" * 60
    Write-Host ""
    Write-Host $line -ForegroundColor Cyan
    Write-Host "  $Msg" -ForegroundColor Cyan
    Write-Host $line -ForegroundColor Cyan
}

function Test-AdminRights {
    $identity = [Security.Principal.WindowsIdentity]::GetCurrent()
    $principal = New-Object Security.Principal.WindowsPrincipal($identity)
    return $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
}

function Test-NssmExists {
    if (-not (Test-Path $NSSM)) {
        Write-Err "NSSM nicht gefunden unter: $NSSM"
        Write-Err "Bitte NSSM installieren: choco install nssm"
        return $false
    }
    return $true
}

function Test-ServiceExists ([string]$WinServiceName) {
    $svc = Get-Service -Name $WinServiceName -ErrorAction SilentlyContinue
    return ($null -ne $svc)
}

function Get-ServiceStatus ([string]$Alias) {
    $winName = $ServiceMap[$Alias]
    if (-not (Test-ServiceExists $winName)) {
        Write-Warn "Service '$winName' ($Alias) ist nicht installiert."
        return
    }

    $svc = Get-Service -Name $winName
    $statusColor = switch ($svc.Status) {
        "Running"  { "Green" }
        "Stopped"  { "Red" }
        default    { "Yellow" }
    }
    Write-Host "  $($Alias.PadRight(10)) | $($winName.PadRight(22)) | " -NoNewline
    Write-Host "$($svc.Status)" -ForegroundColor $statusColor
}

function Show-ServiceStatus ([string[]]$Aliases) {
    Write-Host ""
    Write-Host "  Service     | Windows-Name             | Status" -ForegroundColor White
    Write-Host "  ------------|--------------------------|--------" -ForegroundColor DarkGray
    foreach ($alias in $Aliases) {
        Get-ServiceStatus $alias
    }
    Write-Host ""
}

function Stop-SingleService ([string]$Alias) {
    $winName = $ServiceMap[$Alias]

    if (-not (Test-ServiceExists $winName)) {
        Write-Warn "Service '$winName' ($Alias) ist nicht installiert — überspringe."
        return $true
    }

    $svc = Get-Service -Name $winName
    if ($svc.Status -eq "Stopped") {
        Write-Info "'$Alias' ist bereits gestoppt."
        return $true
    }

    Write-Info "Stoppe '$Alias' ($winName)..."
    try {
        & $NSSM stop $winName 2>&1 | Out-Null
        # Warte bis gestoppt (max 15 Sekunden)
        $timeout = 15
        $elapsed = 0
        while ($elapsed -lt $timeout) {
            $svc = Get-Service -Name $winName
            if ($svc.Status -eq "Stopped") {
                Write-Ok "'$Alias' erfolgreich gestoppt."
                return $true
            }
            Start-Sleep -Seconds 1
            $elapsed++
        }
        Write-Warn "'$Alias' konnte nicht innerhalb von ${timeout}s gestoppt werden."
        return $false
    }
    catch {
        Write-Err "Fehler beim Stoppen von '$Alias': $_"
        return $false
    }
}

function Start-SingleService ([string]$Alias) {
    $winName = $ServiceMap[$Alias]

    if (-not (Test-ServiceExists $winName)) {
        Write-Warn "Service '$winName' ($Alias) ist nicht installiert — überspringe."
        return $true
    }

    $svc = Get-Service -Name $winName
    if ($svc.Status -eq "Running") {
        Write-Info "'$Alias' läuft bereits."
        return $true
    }

    Write-Info "Starte '$Alias' ($winName)..."
    try {
        & $NSSM start $winName 2>&1 | Out-Null
        # Warte bis gestartet (max 15 Sekunden)
        $timeout = 15
        $elapsed = 0
        while ($elapsed -lt $timeout) {
            $svc = Get-Service -Name $winName
            if ($svc.Status -eq "Running") {
                Write-Ok "'$Alias' erfolgreich gestartet."
                return $true
            }
            Start-Sleep -Seconds 1
            $elapsed++
        }
        Write-Warn "'$Alias' konnte nicht innerhalb von ${timeout}s gestartet werden."
        return $false
    }
    catch {
        Write-Err "Fehler beim Starten von '$Alias': $_"
        return $false
    }
}

function Invoke-HealthCheck {
    Write-Info "Health-Check: $HealthUrl ..."
    Start-Sleep -Seconds 2  # Kurz warten bis Backend bereit ist

    try {
        $response = Invoke-WebRequest -Uri $HealthUrl -UseBasicParsing -TimeoutSec 10
        if ($response.StatusCode -eq 200) {
            Write-Ok "Backend Health-Check bestanden (HTTP $($response.StatusCode))"
            $body = $response.Content
            if ($body) {
                Write-Host "  Response: $body" -ForegroundColor DarkGray
            }
        }
        else {
            Write-Warn "Backend Health-Check: HTTP $($response.StatusCode)"
        }
    }
    catch {
        Write-Err "Backend Health-Check fehlgeschlagen: $_"
    }
}

# --- Hauptlogik ---------------------------------------------------------------

# Admin-Check
if (-not (Test-AdminRights)) {
    Write-Warn "Script läuft OHNE Administrator-Rechte!"
    Write-Warn "Service-Befehle benötigen Admin-Rechte und könnten fehlschlagen."
    Write-Host ""
}

# NSSM-Check
if (-not (Test-NssmExists)) {
    exit 1
}

# Bestimme welche Services betroffen sind
if ($Service -eq "all") {
    $targetAliases = $ServiceMap.Keys | ForEach-Object { $_ }
}
else {
    $targetAliases = @($Service)
}

Write-Header "Deadlock Tournament Platform — Service Manager"
Write-Info "Aktion: $($Action.ToUpper())  |  Ziel: $Service"

switch ($Action) {
    "status" {
        Show-ServiceStatus $targetAliases
    }

    "stop" {
        if ($Service -eq "all") {
            foreach ($alias in $StopOrder) {
                Stop-SingleService $alias
            }
        }
        else {
            Stop-SingleService $Service
        }
        Write-Header "Status nach Stop"
        Show-ServiceStatus $targetAliases
    }

    "start" {
        if ($Service -eq "all") {
            foreach ($alias in $StartOrder) {
                Start-SingleService $alias
            }
        }
        else {
            Start-SingleService $Service
        }

        # Health-Check wenn Backend gestartet wurde
        if ($Service -eq "all" -or $Service -eq "backend") {
            Invoke-HealthCheck
        }

        Write-Header "Status nach Start"
        Show-ServiceStatus $targetAliases
    }

    "restart" {
        # --- Stop-Phase ---
        Write-Header "Phase 1: Services stoppen"
        if ($Service -eq "all") {
            foreach ($alias in $StopOrder) {
                Stop-SingleService $alias
            }
        }
        else {
            Stop-SingleService $Service
        }

        Start-Sleep -Seconds 1

        # --- Start-Phase ---
        Write-Header "Phase 2: Services starten"
        if ($Service -eq "all") {
            foreach ($alias in $StartOrder) {
                Start-SingleService $alias
            }
        }
        else {
            Start-SingleService $Service
        }

        # Health-Check wenn Backend neugestartet wurde
        if ($Service -eq "all" -or $Service -eq "backend") {
            Invoke-HealthCheck
        }

        Write-Header "Status nach Restart"
        Show-ServiceStatus $targetAliases
    }
}

Write-Ok "Fertig."
