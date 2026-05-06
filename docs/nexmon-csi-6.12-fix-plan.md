# Nexmon CSI 6.12 Fix Plan

> For this Raspberry Pi 4B on Linux 6.12.75+rpt-rpi-v8, the CSI firmware patch loads but no UDP packets are emitted on port 5500. The verified failure is in the host driver/runtime path for kernel 6.12, not in `makecsiparams`, `tcpdump`, or the setup shell script.

## Verified State

- Firmware patch is active:
  - `dmesg` shows `7.45.189 (nexmon.org/csi: a975-dirty-7)`.
- Firmware-side CSI control exists and is correct:
  - `nexmon_csi/src/ioctl.c`
  - `nexmon_csi/src/csi_extractor.c`
- Live Pi verification failed across multiple runtime combinations:
  - unmanaged `wlan0`
  - power save off
  - CSI config via `nexutil -s500`
  - monitor values including UDP mode
  - channel sweep `1` through `11`
  - result: `0 packets captured` on port `5500`
- Local driver trees only cover:
  - `brcmfmac_4.19.y-nexmon`
  - `brcmfmac_5.4.y-nexmon`
  - `brcmfmac_5.10.y-nexmon`
- Target kernel is newer:
  - `6.12.75+rpt-rpi-v8`

## External Evidence

- Upstream Nexmon has an open PR named `Added driver for kernel 6.12`:
  - `#664`, opened June 5, 2025
- Kali ships `brcmfmac-nexmon-dkms` versions for `6.12` and `6.12.2`
- Kali package notes for `6.12` mention:
  - compatibility with `6.12`
  - `cfg80211.c` changes for `6.13`
  - `need debug on raspi`

## Root Cause

The firmware patch is running, but the 6.12 host-side driver/runtime path that is required to drive CSI extraction correctly is missing from this workspace. The current `Makefile.rpi` path assumes recent kernels can work with the vendor-command path alone, but live verification on this Pi disproves that assumption.

## Solve Strategy

Port or import the real 6.12-compatible `brcmfmac` Nexmon driver layer into this repo, then switch installation and runtime to use that path instead of relying only on the stock `brcmfmac_wcc` plugin plus firmware replacement.

## Files To Study First

- Firmware-side CSI logic:
  - `nexmon_csi/src/ioctl.c`
  - `nexmon_csi/src/csi_extractor.c`
- Current driver integration:
  - `nexmon_csi/Makefile.rpi`
  - `nexmon_csi/brcmfmac_5.10.y-nexmon/core.c`
  - `nexmon_csi/brcmfmac_5.10.y-nexmon/cfg80211.c`
  - `nexmon_csi/brcmfmac_5.10.y-nexmon/vendor.c`
  - `nexmon_csi/brcmfmac_5.10.y-nexmon/fwil.h`

## Missing Inputs To Pull In

At least one of these needs to be imported into the workspace:

- Upstream Nexmon PR `#664` driver tree for `6.12`
- Kali `brcmfmac-nexmon-dkms_6.12.tar.xz`
- Kali `brcmfmac-nexmon-dkms_6.12.2.tar.xz`

## Implementation Tasks

### Task 1: Import the 6.12 driver source

- Create a vendor source area, for example:
  - `vendor/brcmfmac-nexmon-dkms-6.12.2/`
- Preserve the imported source exactly as reference material.
- Do not patch the imported files in place initially.

### Task 2: Diff 6.12 against the local 5.10 tree

Compare these categories:

- monitor-mode setup
- promisc propagation
- vendor-command plumbing
- cfg80211 interface-mode handling
- monitor channel setting
- Raspberry Pi specific guards
- API compatibility fixes for kernel 6.12 and 6.13

Primary target files:

- `cfg80211.c`
- `core.c`
- `vendor.c`
- `fwil.h`
- `feature.c`
- `feature.h`
- `Makefile`
- any module glue required by DKMS packaging

### Task 3: Add a local 6.12 driver tree

Create a new driver directory:

- `nexmon_csi/brcmfmac_6.12.y-nexmon/`

Populate it from the verified 6.12 source, then adapt only the repo-local paths and build expectations needed for this workspace.

### Task 4: Update build/install flow

Extend the current Raspberry Pi flow so it can choose the 6.12 driver path explicitly.

Required outcomes:

- build the `6.12` driver module
- install it without depending on the old `5.10` tree
- avoid the current broken assumption that firmware replacement alone is sufficient

Likely file to modify:

- `nexmon_csi/Makefile.rpi`

### Task 5: Make runtime deterministic

After the 6.12 driver is installed, the runtime sequence must do all of the following:

- stop `wpa_supplicant`
- set `wlan0` unmanaged
- disable power save
- set CSI parameters with `nexutil -s500`
- enable the correct monitor/UDP mode for this driver path
- confirm the radio is tuned to the expected chanspec

### Task 6: Verify with packet capture

Use a fresh verification sequence on the Pi:

```bash
dmesg | grep "Firmware: BCM4345"
nexutil -Iwlan0 -k
sudo timeout 20 tcpdump -n -i wlan0 udp dst port 5500
```

Success condition:

- at least one UDP packet to port `5500` appears during the capture window

### Task 7: Only then repair the user script

Once CSI actually works on 6.12:

- clean `nexmon_setp.sh`
- remove stale assumptions
- remove broken example text
- make the script idempotent
- make the script use the real 6.12 driver/install path

## Risks

- `brcmfmac_wcc` unload/reload instability may still need code changes or an alternate module handling path.
- Raspberry Pi downstream kernel deltas may differ from the Kali 6.12 driver assumptions.
- There may be a second RasPi-specific bug even after importing the 6.12 driver line, since Kali’s package notes explicitly said `need debug on raspi`.

## First Practical Next Step

Get the `6.12` or `6.12.2` driver source into the workspace, then diff it against `brcmfmac_5.10.y-nexmon`. That diff will determine whether this is a straightforward port into `brcmfmac_6.12.y-nexmon` or whether Raspberry Pi specific compatibility patches are also required.
