param(
  [int]$UdpPort = 5015,
  [int]$HttpPort = 3000,
  [int]$WsPort = 3001,
  [string]$Source = "esp32",
  [string]$ModelPath = ""
)

$ErrorActionPreference = "Stop"

function Stop-PortOwner {
  param([int]$Port)

  $tcp = Get-NetTCPConnection -LocalPort $Port -State Listen -ErrorAction SilentlyContinue
  $udp = Get-NetUDPEndpoint -LocalPort $Port -ErrorAction SilentlyContinue

  $pids = @()
  if ($tcp) { $pids += ($tcp | Select-Object -ExpandProperty OwningProcess) }
  if ($udp) { $pids += ($udp | Select-Object -ExpandProperty OwningProcess) }

  $pids = $pids | Sort-Object -Unique
  foreach ($pid in $pids) {
    if ($pid -and $pid -ne $PID) {
      try {
        Stop-Process -Id $pid -Force -ErrorAction Stop
        Write-Host "Stopped PID $pid on port $Port"
      } catch {
        Write-Host "Could not stop PID $pid on port $Port: $($_.Exception.Message)"
      }
    }
  }
}

$repoRoot = Resolve-Path (Join-Path $PSScriptRoot "..\\..")
$serverDir = Join-Path $repoRoot "rust-port\\wifi-densepose-rs"
$serverExe = Join-Path $serverDir "target\\debug\\sensing-server.exe"

if (!(Test-Path $serverExe)) {
  throw "sensing-server.exe not found at $serverExe. Build first: cargo build -p wifi-densepose-sensing-server"
}

Stop-PortOwner -Port $UdpPort
Stop-PortOwner -Port $HttpPort
Stop-PortOwner -Port $WsPort

Set-Location $serverDir

$args = @(
  "--source", $Source,
  "--udp-port", "$UdpPort",
  "--http-port", "$HttpPort",
  "--ws-port", "$WsPort"
)

if ($ModelPath.Trim().Length -gt 0) {
  $resolvedModel = (Resolve-Path $ModelPath).Path
  $args += @("--model", $resolvedModel, "--progressive")
}

Write-Host "Starting sensing-server on UDP=$UdpPort HTTP=$HttpPort WS=$WsPort SOURCE=$Source"
& $serverExe @args
