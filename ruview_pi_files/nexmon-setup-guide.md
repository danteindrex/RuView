# Nexmon CSI Setup Guide for Raspberry Pi 4B

A step-by-step guide to setting up [Nexmon CSI](https://github.com/seemoo-lab/nexmon_csi) on a Raspberry Pi 4B running a recent kernel (5.15+). This enables Channel State Information (CSI) extraction over WiFi for sensing applications.

---

## Prerequisites

- Raspberry Pi 4B (the BCM4345C0 chip is required — other Pi models use different chips)
- Raspbian/Raspberry Pi OS installed (32-bit or 64-bit)
- Internet access (Ethernet recommended — WiFi will be taken down during setup)
- A second terminal/SSH session available, or a physical keyboard + monitor connected

> **Warning:** Step 10 (`unmanage`) takes the WiFi interface down. If you are connected via WiFi only, you will lose SSH access. Connect via Ethernet before proceeding past Step 9.

---

## Step 1 — Verify Kernel Version

Nexmon CSI requires kernel 5.15 or newer on recent Raspberry Pi OS builds.

```bash
uname -r
```

Expected output example: `6.1.21-v8+`

---

## Step 1.1 — Verify BCM4345 Firmware Version

The firmware version on your device should be higher than `7_45_189`. Confirm it:

```bash
dmesg | grep "Firmware: BCM4345"
```

Note the version string returned — you'll need it when selecting the correct patch directory in Step 8.

---

## Step 2 — Kill wpa_supplicant

Stop the wireless supplicant so it does not interfere with the build:

```bash
sudo pkill wpa_supplicant
```

---

## Step 3 — Update System and Install Dependencies

```bash
sudo apt update
sudo apt full-upgrade
sudo apt install git libgmp3-dev gawk qpdf bison flex make autoconf libtool texinfo xxd \
  libnl-3-dev libnl-genl-3-dev bc libssl-dev tcpdump
```

---

## Step 4 — Add 32-bit (armhf) Architecture Support (64-bit OS Only)

Skip this step if you are running a 32-bit OS image.

```bash
sudo dpkg --add-architecture armhf
sudo apt update
sudo apt-get install libc6:armhf libisl23:armhf libmpfr6:armhf libmpc3:armhf libstdc++6:armhf
sudo ln -s /usr/lib/arm-linux-gnueabihf/libisl.so.23  /usr/lib/arm-linux-gnueabihf/libisl.so.10
sudo ln -s /usr/lib/arm-linux-gnueabihf/libmpfr.so.6  /usr/lib/arm-linux-gnueabihf/libmpfr.so.4
```

---

## Step 5 — Install Python 2.7

Python 2.7 is required by the `bcm43` build tool used inside Nexmon. It is no longer in current Debian/Raspbian repos, so you must temporarily add the Debian Stretch archive:

```bash
sudo cp /etc/apt/sources.list /tmp/
echo 'deb http://archive.debian.org/debian/ stretch contrib main non-free' | sudo tee -a /etc/apt/sources.list
sudo apt update
sudo apt install python2.7
sudo mv /tmp/sources.list /etc/apt/
sudo apt update
```

> **Note:** The `sources.list` is restored immediately after installation so your system continues to use current repos.

---

## Step 6 — Clone and Initialise the Nexmon Repository

```bash
git clone --depth=1 https://github.com/seemoo-lab/nexmon.git
cd nexmon
source setup_env.sh
sed -i '1 s/$/2.7/' $NEXMON_ROOT/buildtools/b43-v3/debug/b43-beautifier
make
```

`source setup_env.sh` sets the `$NEXMON_ROOT` environment variable and other build environment variables needed by subsequent steps.

> **Note:** `make` will display a number of warnings. As long as it completes without actual error messages, this is fine. If you see `arm-none-eabi-gcc: not found`, go back and complete Step 4 — ensure the armhf architecture and its libraries are properly installed.

---

## Step 7 — Build and Install nexutil

```bash
cd $NEXMON_ROOT/utilities/nexutil
sudo -E make install USE_VENDOR_CMD=1
sudo setcap cap_net_admin+ep /usr/bin/nexutil
```

`setcap` grants `nexutil` the `CAP_NET_ADMIN` capability so it can run without `sudo`.

---

## Step 8 — Clone the nexmon_csi Repository

```bash
cd $NEXMON_ROOT/patches/bcm43455c0/7_45_189
git clone --depth=1 https://github.com/seemoo-lab/nexmon_csi.git
cd nexmon_csi
```

> **Note:** This command must be run from the `7_45_189` directory, as the Makefile scripts in the next step are built for this firmware version. If your firmware version differs, adjust the directory path accordingly.

---

## Step 9 — Install the nexmon_csi Firmware Patch

```bash
make -f Makefile.rpi install-firmware
```

### Troubleshooting 9.1 — "recipe commences before first target"

If you see:

```
Makefile.rpi:2: *** recipe commences before first target.  Stop.
```

The build environment was lost. Re-source `setup_env.sh` and retry:

```bash
source $NEXMON_ROOT/setup_env.sh
make -f Makefile.rpi install-firmware
```

### Troubleshooting 9.2 — `arm-none-eabi-gcc: not found`

If you see:

```
/home/pi/nexmon/buildtools/gcc-arm-none-eabi-5_4-2016q2-linux-armv7l/bin/arm-none-eabi-gcc: not found
make: *** [Makefile.rpi:76: obj/console.o] Fehler 127
```

This means the 32-bit ARM cross-compiler cannot execute. Go back and complete **Step 4** (adding `armhf` architecture and its libraries).

---

## Step 10 — Unmanage Interface and Reload Firmware

> **Warning:** Running `unmanage` takes `wlan0` down. If you are connected via WiFi only and there are no other SSIDs the Pi can connect to, you will lose access unless peripherals are connected. Connect via Ethernet before running this step.

```bash
make -f Makefile.rpi unmanage
make -f Makefile.rpi reload-full
```

---

## Step 13 — Generate CSI Parameters

Navigate to the `makecsiparams` utility and compile it:

```bash
cd utils/makecsiparams
make
```

Generate a config string for your desired channel and bandwidth. For example, channel 36, 80 MHz, 1 core, 1 spatial stream:

```bash
./makecsiparams -c 36/80 -C 1 -N 1
```

This outputs a Base64 config string and closes immediately, for example:

```
KuABEQAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA==
```

Return to the `nexmon_csi` root:

```bash
cd ../..
```

Copy this string — you will need it in Step 14.

---

## Step 14 — Configure CSI Extractor and Enable Monitor Mode

```bash
nexutil -s500 -b -l34 -v<your-config-generated-with-makecsiparams>
nexutil -m1
```

Replace `<your-config-generated-with-makecsiparams>` with the Base64 string from Step 13.

---

## Step 16 — Capture CSI UDP Packets (Demo)

```bash
sudo tcpdump -i wlan0 dst port 5500
```

CSI packets are sent as UDP to port 5500 and will be printed to the terminal.

---

## Step 17 — Reboot

```bash
sudo reboot
```

---

## Step 18 — Post-Reboot Verification

After the Pi comes back up, the WiFi interface will still be present but **unmanaged** — you won't be able to join WiFi networks via NetworkManager, but monitor mode and CSI extraction still work.

Verify the state:

```bash
nmcli device status
```

`wlan0` should show as `unmanaged`.

Check the active firmware version:

```bash
dmesg | grep "Firmware: BCM4345"
```

The firmware version will now show as `7_45_189` — this is expected; the patch swaps the firmware for you.

---

## Step 19 — Unblock wlan0 with rfkill

After reboot, the interface may be blocked by rfkill and appear down. Unblock it and bring it up:

```bash
rfkill list                  # confirm wlan0 is blocked
sudo rfkill unblock all      # unblock all wireless interfaces
sudo ip link set wlan0 up    # bring the interface up
```

---

## Step 20 — Repeat CSI Capture

Repeat Steps 14 and 16 to configure the CSI extractor and start streaming again:

```bash
# Step 14 — apply config and enable monitor mode
nexutil -s500 -b -l34 -v<your-config-string>
nexutil -m1

# Step 16 — capture CSI UDP packets
sudo tcpdump -i wlan0 dst port 5500
```

---

## Resetting to Default Firmware

To restore the original WiFi firmware and hand control back to NetworkManager:

```bash
make -f Makefile.rpi restore-wifi
```

---

## Quick Reference

| Step | What it does |
|------|--------------|
| 1–1.1 | Verify kernel (5.15+) and firmware version (> 7_45_189) |
| 2 | Stop wpa_supplicant |
| 3 | Install build dependencies |
| 4 | Add armhf support (64-bit OS only) |
| 5 | Install Python 2.7 from Debian Stretch |
| 6 | Clone + init Nexmon (warnings in make are expected) |
| 7 | Build + install nexutil |
| 8 | Clone nexmon_csi patch (must run from 7_45_189 dir) |
| 9 | Patch and flash CSI firmware |
| 10 | Unmanage interface, reload firmware |
| 13 | Generate CSI config params |
| 14 | Apply config + enable monitor mode |
| 16 | Capture CSI packets with tcpdump |
| 17 | Reboot |
| 18 | Verify unmanaged state + firmware version post-reboot |
| 19 | rfkill unblock + bring wlan0 up |
| 20 | Repeat steps 14 & 16 to resume capture |
