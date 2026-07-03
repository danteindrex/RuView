# start-ruview.ps1 — Start the RuView sensing stack (standalone mode).
#
#   .\scripts\start-ruview.ps1                  start the sensing server if not running
#   .\scripts\start-ruview.ps1 -RegisterStartup also auto-start it at every Windows logon
#   .\scripts\start-ruview.ps1 -Unregister      remove the auto-start task
#
# NOTE: Use EITHER this standalone server OR the Wave Desktop app's managed
# server (Control Plane -> Start) — not both. They compete for ports
# 3000/8765/5005. Stop this one with:  Get-Process sensing-server | Stop-Process

param(
    [switch]$RegisterStartup,
    [switch]$Unregister
)

$repoRoot = Split-Path -Parent $PSScriptRoot
$serverExe = Join-Path $repoRoot 'rust-port\wifi-densepose-rs\target\debug\sensing-server.exe'
$taskName = 'RuView Sensing Server'

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
    schtasks /Create /F /TN $taskName /SC ONLOGON /TR "`"$serverExe`" --http-port 3000 --source esp32" /RL LIMITED
    Write-Host "Registered '$taskName' to start at logon."
}

if (Get-Process sensing-server -ErrorAction SilentlyContinue) {
    Write-Host "Sensing server already running."
} else {
    Start-Process $serverExe -ArgumentList '--http-port','3000','--source','esp32' -WindowStyle Hidden
    Start-Sleep -Seconds 3
    try {
        $h = Invoke-WebRequest -Uri http://localhost:3000/health -UseBasicParsing -TimeoutSec 5
        Write-Host "Sensing server running: $($h.Content)" -ForegroundColor Green
        Write-Host "UI: http://localhost:3000/ui/index.html"
    } catch {
        Write-Host "Server started but health check failed — check if ports 3000/5005 are in use." -ForegroundColor Yellow
    }
}
