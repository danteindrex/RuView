# start-ruview.ps1 -- Start the RuView sensing stack (standalone mode).
#
#   .\scripts\start-ruview.ps1                  start the sensing server if not running
#   .\scripts\start-ruview.ps1 -RegisterStartup also auto-start it at every Windows logon
#   .\scripts\start-ruview.ps1 -Unregister      remove the auto-start task
#
# NOTE: Use EITHER this standalone server OR the Wave Desktop app's managed
# server (Control Plane -> Start) -- not both. They compete for ports
# 4000 (HTTP), 4001 (WebSocket), 5005 (ESP32 UDP), and 5500 (nexmon UDP).
# Stop this one with:  Get-Process sensing-server | Stop-Process
#
# NOTE: keep this file pure ASCII. PowerShell 5.1 reads BOM-less files as
# ANSI, and multi-byte UTF-8 punctuation decodes into curly quotes that
# PowerShell treats as string delimiters (parse errors far from the cause).

param(
    [switch]$RegisterStartup,
    [switch]$Unregister
)

$repoRoot = Split-Path -Parent $PSScriptRoot
$serverExe = Join-Path $repoRoot 'rust-port\wifi-densepose-rs\target\debug\sensing-server.exe'
$taskName = 'RuView Sensing Server'
$httpPort = 4000

if ($Unregister) {
    schtasks /Delete /TN $taskName /F
    exit $LASTEXITCODE
}

if (-not (Test-Path $serverExe)) {
    Write-Host "Server binary not found. Build it first:" -ForegroundColor Red
    Write-Host "  cd rust-port\wifi-densepose-rs; cargo build -p wifi-densepose-sensing-server --no-default-features"
    exit 1
}

if ($RegisterStartup) {
    schtasks /Create /F /TN $taskName /SC ONLOGON /TR "`"$serverExe`" --http-port $httpPort --source esp32" /RL LIMITED
    Write-Host "Registered '$taskName' to start at logon."
}

# A server may already be listening on the HTTP port -- either a previous run
# of this script or the desktop app's managed server. Probe /health before
# starting a second instance (they would fight over 4000/4001/5005/5500).
$alreadyServing = $false
try {
    $null = Invoke-WebRequest -Uri "http://localhost:$httpPort/health" -UseBasicParsing -TimeoutSec 2
    $alreadyServing = $true
} catch {}

if ($alreadyServing) {
    Write-Host "A sensing server is already responding on port $httpPort (possibly the desktop app's managed server)." -ForegroundColor Yellow
    Write-Host "Not starting another instance. UI: http://localhost:$httpPort/ui/index.html"
} elseif (Get-Process sensing-server -ErrorAction SilentlyContinue) {
    Write-Host "A sensing-server process is already running but not answering on port $httpPort yet." -ForegroundColor Yellow
    Write-Host "Give it a few seconds, or stop it with:  Get-Process sensing-server | Stop-Process"
} else {
    Start-Process $serverExe -ArgumentList '--http-port',"$httpPort",'--source','esp32' -WindowStyle Hidden
    Start-Sleep -Seconds 3
    try {
        $h = Invoke-WebRequest -Uri "http://localhost:$httpPort/health" -UseBasicParsing -TimeoutSec 5
        Write-Host "Sensing server running: $($h.Content)" -ForegroundColor Green
        Write-Host "UI: http://localhost:$httpPort/ui/index.html"
    } catch {
        Write-Host "Server started but health check failed -- check if ports $httpPort/4001/5005/5500 are in use." -ForegroundColor Yellow
    }
}
