# setup-esp32-node.ps1 -- One-shot ESP32-S3 CSI node setup for Wave.
#
# Flashes the latest released firmware, provisions WiFi from the PC's current
# connection, and verifies the node comes online. Run from the repo root:
#
#   .\scripts\setup-esp32-node.ps1 -NodeId 2
#
# Parameters (all optional):
#   -NodeId    Unique node ID (default 1). Use 2, 3, ... for additional nodes.
#   -ComPort   Serial port (auto-detected if omitted)
#   -Ssid      WiFi SSID (default: the network this PC is connected to; must be 2.4 GHz)
#   -Password  WiFi password (default: read from the Windows profile for -Ssid)
#   -TargetIp  Where the node streams CSI (default: this PC's WiFi IPv4)
#   -TdmSlot   This node's TDM slot index, 0-based (default NodeId-1 when -TdmTotal is set)
#   -TdmTotal  Total nodes in the TDM schedule (omit for single free-running node)
#   -SkipFlash Only re-provision (keep existing firmware)
#   -Release   Firmware release tag (default v0.8.3-esp32)
#
# NOTE: keep this file pure ASCII. PowerShell 5.1 reads BOM-less files as
# ANSI, and multi-byte UTF-8 punctuation decodes into curly quotes that
# PowerShell treats as string delimiters (parse errors far from the cause).

param(
    [int]$NodeId = 1,
    [string]$ComPort = "",
    [string]$Ssid = "",
    [string]$Password = "",
    [string]$TargetIp = "",
    [int]$TdmSlot = -1,
    [int]$TdmTotal = 0,
    [switch]$SkipFlash,
    [string]$Release = "v0.8.3-esp32"
)

$ErrorActionPreference = "Stop"
$repoRoot = Split-Path -Parent $PSScriptRoot

function Fail($msg) { Write-Host "ERROR: $msg" -ForegroundColor Red; exit 1 }
function Step($msg) { Write-Host "`n==> $msg" -ForegroundColor Cyan }

# -- 0. TDM parameter validation ----------------------------------------------
# provision.py requires --tdm-slot and --tdm-total together, slot < total.
if ($TdmSlot -ge 0 -and $TdmTotal -le 0) {
    Fail "-TdmSlot requires -TdmTotal (total nodes in the TDM schedule)."
}
if ($TdmTotal -gt 0) {
    if ($TdmSlot -lt 0) { $TdmSlot = $NodeId - 1 }
    if ($TdmSlot -lt 0 -or $TdmSlot -ge $TdmTotal) {
        Fail "TDM slot $TdmSlot is out of range [0..$($TdmTotal-1)]. Slots are 0-based (node 1 -> slot 0); when -TdmSlot is omitted it defaults to NodeId-1."
    }
}

# -- 1. Detect serial port ---------------------------------------------------
Step "Detecting ESP32 serial port"
if (-not $ComPort) {
    $ports = Get-CimInstance Win32_PnPEntity | Where-Object {
        $_.Name -match 'COM\d+' -and $_.Name -match 'CH34|CP210|USB.*Serial|Silicon Labs'
    }
    if (-not $ports) { Fail "No USB-serial device found. Plug in the ESP32 (use a data cable) and check Device Manager." }
    if (@($ports).Count -gt 1) {
        $ports | ForEach-Object { Write-Host "  $($_.Name)" }
        Fail "Multiple serial devices found. Re-run with -ComPort COMx"
    }
    $ComPort = [regex]::Match(@($ports)[0].Name, 'COM\d+').Value
}
Write-Host "  Using $ComPort"

# -- 2. Network parameters ---------------------------------------------------
Step "Resolving WiFi credentials and target IP"
$wlan = netsh wlan show interfaces
if (-not $Ssid) {
    $m = $wlan | Select-String '^\s+SSID\s+:\s+(.+)$'
    if (-not $m) { Fail "PC is not connected to WiFi. Pass -Ssid/-Password explicitly." }
    $Ssid = $m.Matches[0].Groups[1].Value.Trim()
}
$band = $wlan | Select-String 'Band\s+:\s+(.+)$' | Select-Object -First 1
if ($band -and $band.Matches[0].Groups[1].Value.Trim() -eq '5 GHz') {
    Write-Host "  WARNING: current network is 5 GHz. The ESP32 is 2.4 GHz-only." -ForegroundColor Yellow
    Write-Host "  Make sure '$Ssid' also broadcasts on 2.4 GHz, or pass a 2.4 GHz -Ssid." -ForegroundColor Yellow
}
if (-not $Password) {
    $m = netsh wlan show profile name="$Ssid" key=clear | Select-String 'Key Content\s+:\s+(.+)$'
    if (-not $m) { Fail "Could not read the password for '$Ssid'. Pass -Password explicitly." }
    $Password = $m.Matches[0].Groups[1].Value.Trim()
}
if (-not $TargetIp) {
    $TargetIp = (Get-NetIPAddress -AddressFamily IPv4 | Where-Object {
        $_.InterfaceAlias -eq 'Wi-Fi' -and $_.IPAddress -notmatch '^169\.254'
    } | Select-Object -First 1).IPAddress
    if (-not $TargetIp) { Fail "Could not determine this PC's WiFi IP. Pass -TargetIp explicitly." }
}
Write-Host "  SSID: $Ssid | Target: ${TargetIp}:5005 | Node ID: $NodeId"

# -- 3. Tools ------------------------------------------------------------------
Step "Checking tools (esptool, nvs-partition-gen)"
python -m esptool version *> $null
if ($LASTEXITCODE -ne 0) { python -m pip install --quiet esptool pyserial esp-idf-nvs-partition-gen }

# -- 4. Firmware download (cached) ---------------------------------------------
if (-not $SkipFlash) {
    Step "Fetching firmware $Release"
    $fwDir = Join-Path $env:LOCALAPPDATA "Wave\firmware\$Release"
    if (-not (Test-Path (Join-Path $fwDir 'esp32-csi-node-s3-8mb.bin'))) {
        New-Item -ItemType Directory -Force $fwDir | Out-Null
        gh release download $Release --dir $fwDir --clobber
        if ($LASTEXITCODE -ne 0) { Fail "gh release download failed. Is GitHub CLI logged in?" }
    }
    Write-Host "  Firmware cached at $fwDir"

    # -- 5. Flash (offsets from the release partition table) --------------------
    Step "Flashing firmware to $ComPort"
    python -m esptool --chip esp32s3 --port $ComPort --baud 460800 write-flash `
        --flash-mode dio --flash-size 8MB `
        0x0     (Join-Path $fwDir 'bootloader.bin') `
        0x8000  (Join-Path $fwDir 'partition-table.bin') `
        0xf000  (Join-Path $fwDir 'ota_data_initial.bin') `
        0x20000 (Join-Path $fwDir 'esp32-csi-node-s3-8mb.bin')
    if ($LASTEXITCODE -ne 0) { Fail "Flash failed. Check the cable/port and try again." }
}

# -- 6. Provision NVS ----------------------------------------------------------
Step "Provisioning WiFi + target (node_id=$NodeId)"
$provArgs = @(
    '--port', $ComPort, '--ssid', $Ssid, '--password', $Password,
    '--target-ip', $TargetIp, '--node-id', $NodeId
)
if ($TdmTotal -gt 0) {
    Write-Host "  TDM: slot $TdmSlot of $TdmTotal"
    $provArgs += @('--tdm-slot', $TdmSlot, '--tdm-total', $TdmTotal)
}
python (Join-Path $repoRoot 'firmware\esp32-csi-node\provision.py') @provArgs
if ($LASTEXITCODE -ne 0) { Fail "Provisioning failed." }

Write-Host ""
Write-Host "  NOTE: the node streams CSI to $TargetIp and re-resolves it only at" -ForegroundColor Yellow
Write-Host "  boot (or on a WAVE_HUB re-announcement). If your router hands this" -ForegroundColor Yellow
Write-Host "  PC a different DHCP address later, the node goes silent. Set a DHCP" -ForegroundColor Yellow
Write-Host "  reservation for $TargetIp in your router's admin page." -ForegroundColor Yellow

# -- 7. Verify: watch serial for WiFi connection --------------------------------
Step "Verifying node comes online (30 s)"
$verifyPy = Join-Path $env:TEMP 'wave-verify-node.py'
@(
    'import serial, sys, time',
    "s = serial.Serial('$ComPort', 115200, timeout=1)",
    's.setDTR(False); s.setRTS(True); time.sleep(0.1); s.setRTS(False)',
    'end = time.time() + 30',
    'ok = False',
    "buf = b''",
    'while time.time() < end:',
    '    buf += s.read(4096)',
    "    text = buf.decode('utf-8', errors='replace')",
    "    if 'Connected to WiFi' in text:",
    '        ok = True',
    '        break',
    's.close()',
    "print('CONNECTED' if ok else 'TIMEOUT')"
) | Set-Content $verifyPy -Encoding ascii
$result = python -X utf8 $verifyPy
if ($result -match 'CONNECTED') {
    Write-Host "`nNode $NodeId is ONLINE - joined '$Ssid', streaming CSI to ${TargetIp}:5005" -ForegroundColor Green
    Write-Host "It now works from any USB power source. Unplug and place it anywhere in WiFi range."
} else {
    Fail "Node did not join '$Ssid' within 30 s. Check the password and that the network is 2.4 GHz, then re-run with -SkipFlash."
}
