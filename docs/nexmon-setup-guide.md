# Nexmon CSI Setup Guide for Raspberry Pi 4B on Kernel 6.12

This guide is for Raspberry Pi 4B systems using the `bcm43455c0` Wi-Fi chip on Raspberry Pi OS with kernel `6.12.x`.

The important correction is this:
- the stock upstream `nexmon_csi` firmware patch was not enough by itself in live testing on `6.12.75+rpt-rpi-v8`
- the Pi also needs the replacement `brcmfmac` driver path that is now staged in this workspace under `nexmon_csi/brcmfmac_6.12.y-nexmon`

Use Ethernet while doing this. CSI setup deliberately takes normal Wi-Fi management down.

---

## Prerequisites

- Raspberry Pi 4B with `bcm43455c0`
- Raspberry Pi OS with kernel `6.12.x`
- Ethernet access to the Pi
- `nexmon_csi` from this workspace, not only a fresh upstream clone

---

## Step 1 - Verify Kernel and Firmware

```bash
uname -r
dmesg | grep "Firmware: BCM4345"
```

You want to confirm:
- the Pi is really on `6.12.x`
- the firmware family is `7.45.189`

---

## Step 2 - Install Dependencies

```bash
sudo apt update
sudo apt full-upgrade
sudo apt install git libgmp3-dev gawk qpdf bison flex make autoconf libtool texinfo xxd \
  libnl-3-dev libnl-genl-3-dev bc libssl-dev tcpdump raspberrypi-kernel-headers dkms
```

If you are on 64-bit Raspberry Pi OS, also add the old armhf runtime libs used by the firmware build tools:

```bash
sudo dpkg --add-architecture armhf
sudo apt update
sudo apt install libc6:armhf libisl23:armhf libmpfr6:armhf libmpc3:armhf libstdc++6:armhf
sudo ln -sf /usr/lib/arm-linux-gnueabihf/libisl.so.23 /usr/lib/arm-linux-gnueabihf/libisl.so.10
sudo ln -sf /usr/lib/arm-linux-gnueabihf/libmpfr.so.6 /usr/lib/arm-linux-gnueabihf/libmpfr.so.4
```

---

## Step 3 - Install Python 2.7 for the Old Build Tool

```bash
sudo cp /etc/apt/sources.list /tmp/sources.list
echo 'deb http://archive.debian.org/debian/ stretch contrib main non-free' | sudo tee -a /etc/apt/sources.list
sudo apt update
sudo apt install python2.7
sudo mv /tmp/sources.list /etc/apt/sources.list
sudo apt update
```

---

## Step 4 - Build Nexmon Base Tools

```bash
git clone --depth=1 https://github.com/seemoo-lab/nexmon.git
cd nexmon
source setup_env.sh
sed -i '1 s/$/2.7/' $NEXMON_ROOT/buildtools/b43-v3/debug/b43-beautifier
make
```

If you see `arm-none-eabi-gcc: not found`, the armhf compatibility libraries are still incomplete.

---

## Step 5 - Build and Install nexutil

```bash
cd $NEXMON_ROOT/utilities/nexutil
sudo -E make install USE_VENDOR_CMD=1
sudo setcap cap_net_admin+ep /usr/bin/nexutil
```

---

## Step 6 - Stage the Patched nexmon_csi Tree

Do not rely on a plain upstream `git clone` of `nexmon_csi` for kernel `6.12`.

The Pi should use the patched tree from this workspace, which already contains:
- `brcmfmac_6.12.y-nexmon`
- the updated `Makefile.rpi`

Place that tree at:

```bash
$NEXMON_ROOT/patches/bcm43455c0/7_45_189/nexmon_csi
```

---

## Step 7 - Install the 6.12 Driver and CSI Firmware

```bash
cd $NEXMON_ROOT/patches/bcm43455c0/7_45_189/nexmon_csi
source $NEXMON_ROOT/setup_env.sh
make -f Makefile.rpi install-all-6.12
```

This does two things:
- builds and installs the replacement `brcmfmac` module for kernel `6.12`
- installs the CSI firmware image

Use a reboot after this step. In live testing, hot-reloading the old `brcmfmac_wcc` stack was unstable.

---

## Step 8 - Reboot into the New Driver

```bash
sudo reboot
```

---

## Step 9 - Verify the Replacement Driver Loaded

After reconnecting over Ethernet:

```bash
dmesg | grep "Firmware: BCM4345"
modinfo brcmfmac | grep ^version
```

You want to see:
- `nexmon.org/csi` in the firmware string
- a Nexmon-flavoured `brcmfmac` module version

---

## Step 10 - Disable Normal Wi-Fi Management

```bash
cd $NEXMON_ROOT/patches/bcm43455c0/7_45_189/nexmon_csi
make -f Makefile.rpi unmanage
sudo rfkill unblock wifi
sudo ip link set wlan0 up
sudo iw dev wlan0 set power_save off
nexutil -s86 -i -v0
```

At this point, `wlan0` should stay unmanaged and usable for CSI work.

---

## Step 11 - Generate CSI Parameters

```bash
cd utils/makecsiparams
make
./makecsiparams -c 7/20 -C 1 -N 1
cd ../..
```

Use a channel and bandwidth that match the traffic you expect to capture. For example, if `iw dev` shows channel `7` and width `20 MHz`, then `7/20` is the correct starting point.

---

## Step 12 - Configure the Extractor and Create mon0

```bash
nexutil -Iwlan0 -s500 -b -l34 -v<your-config-string>
iw phy `iw dev wlan0 info | gawk '/wiphy/ {printf "phy" $2}'` interface add mon0 type monitor
sudo ifconfig mon0 up
```

Important:
- `mon0` is used to switch the chip into monitor mode on this path
- CSI packets are still captured on `wlan0`, not `mon0`

If `iw phy ... interface add mon0 type monitor` fails with `Operation not supported (-95)`, the replacement driver did not load correctly.

---

## Step 13 - Confirm CSI Output

```bash
sudo timeout 20 tcpdump -n -i wlan0 udp dst port 5500
```

Working output means at least one UDP packet appears before timeout.

If you still get `0 packets captured`, check:
- the `makecsiparams` channel matches the actual Wi-Fi channel
- traffic is really present on that channel
- `mon0` creation succeeded
- `wlan0` is still up and unmanaged

---

## Step 14 - Restore Normal Wi-Fi

```bash
cd $NEXMON_ROOT/patches/bcm43455c0/7_45_189/nexmon_csi
make -f Makefile.rpi restore-wifi-6.12
```

---

## Quick Verification Commands

```bash
uname -r
dmesg | grep "Firmware: BCM4345"
modinfo brcmfmac | grep ^version
nmcli device status
iw dev
ip link show wlan0
sudo timeout 20 tcpdump -n -i wlan0 udp dst port 5500
```
