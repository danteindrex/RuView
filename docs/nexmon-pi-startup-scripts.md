# Nexmon Pi Startup Scripts

These scripts automate the exact Raspberry Pi 4B + RuView flow:
- one-time Nexmon CSI firmware setup
- per-boot CSI runtime startup + bridge to laptop
- Windows launcher for `sensing-server` with port cleanup

## 1) One-time setup on Pi

From the Pi shell:

```bash
cd /home/dante/RuView
chmod +x scripts/pi/setup_nexmon_csi_pi4.sh scripts/pi/start_nexmon_bridge.sh
./scripts/pi/setup_nexmon_csi_pi4.sh
sudo reboot
```

## 2) Start CSI + bridge on Pi (after every reboot)

Replace `192.168.1.8` with your laptop IP:

```bash
cd /home/dante/RuView
./scripts/pi/start_nexmon_bridge.sh --out-host 192.168.1.8 --out-port 5015 --channel 1/20
```

Optional diagnostics only (no bridge start):

```bash
./scripts/pi/start_nexmon_bridge.sh --out-host 192.168.1.8 --test-only
```

## 3) Start server on laptop (PowerShell)

```powershell
Set-Location "C:\Users\user\Documents\RuView"
.\scripts\windows\start_ruview_server.ps1 -UdpPort 5015 -HttpPort 3000 -WsPort 3001 -Source esp32
```

With model:

```powershell
.\scripts\windows\start_ruview_server.ps1 -UdpPort 5015 -HttpPort 3000 -WsPort 3001 -Source esp32 -ModelPath ".\data\models\trained-pretrain-20260302_173607.rvf"
```

## 4) Live UI

Open:

```text
http://localhost:3000/ui/observatory.html
```

If UI says connected but no metrics:
- ensure Pi script is still running (bridge log increments)
- run Pi test mode (`--test-only`) and confirm UDP/5500 packets
- verify laptop and Pi are on same subnet and `--out-host` points to laptop IP
