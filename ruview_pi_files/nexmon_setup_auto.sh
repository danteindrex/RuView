#!/bin/bash

# Automated Nexmon CSI setup for Raspberry Pi 4B (kernel 5.15+).
# Mirrors every step in nexmon_setup.md, inspects all console output for error
# keywords, handles the two documented errors automatically, and exits
# gracefully on any unexpected failure.
#
# Usage:
#   ./nexmon_setup_auto.sh               — full setup (steps 1–14, ends with reboot)
#   ./nexmon_setup_auto.sh --post-reboot — post-reboot step 15 (verification only)
#
# After rebooting, run nexmon_startup.sh on every Pi startup to re-enable
# monitor mode (steps 16–17 from nexmon_setup.md).

set -o pipefail

# ── colours ──────────────────────────────────────────────────────────────────
RED='\033[0;31m'; YELLOW='\033[1;33m'; GREEN='\033[0;32m'
CYAN='\033[0;36m'; BOLD='\033[1m'; NC='\033[0m'

# File where the generated CSI config string is saved for nexmon_startup.sh
CONFIG_SAVE_FILE="$HOME/.config/nexmon/csi_config"

# ── helpers ───────────────────────────────────────────────────────────────────

info()    { echo -e "${CYAN}[INFO]${NC} $*"; }
ok()      { echo -e "${GREEN}[OK]${NC}   $*"; }
warn()    { echo -e "${YELLOW}[WARN]${NC} $*"; }
die()     { echo -e "${RED}[FATAL]${NC} $*" >&2; exit 1; }

# Run a command, capture combined stdout+stderr, check for error keywords,
# and exit with a clear message if anything looks wrong.
# Usage: run_checked <description> <command...>
run_checked() {
    local desc="$1"; shift
    info "Running: $desc"
    local output
    output=$("$@" 2>&1)
    local exit_code=$?
    echo "$output"

    if echo "$output" | grep -qiE '\berror\b|\bfailed\b|\bfailure\b|\bnot found\b|\bpermission denied\b|\baborted\b'; then
        if [ $exit_code -ne 0 ]; then
            die "Step '$desc' reported an error (exit $exit_code). Output above."
        else
            warn "Step '$desc' exited 0 but output contains error keywords — review output above."
        fi
    elif [ $exit_code -ne 0 ]; then
        die "Step '$desc' failed with exit code $exit_code. Output above."
    fi

    ok "$desc"
}

# Run a command, store combined stdout+stderr in OUTPUT, print it, and exit on failure.
capture_checked() {
    local desc="$1"; shift
    info "Capturing: $desc"
    OUTPUT=$("$@" 2>&1)
    local exit_code=$?
    echo "$OUTPUT"
    if [ $exit_code -ne 0 ]; then
        die "Step '$desc' failed with exit code $exit_code."
    fi
    ok "$desc"
}

# ── Post-reboot mode ──────────────────────────────────────────────────────────
if [ "${1:-}" = "--post-reboot" ]; then
    echo -e "\n${BOLD}=== Post-reboot mode: Step 15 ===${NC}"

    # Step 15 — After reboot, verify unmanaged state and firmware version
    echo -e "\n${BOLD}=== Step 15: Post-reboot verification ===${NC}"
    info "After reboot, the wifi interface should be back up and working, but it will still be unmanaged."
    info "This means that you won't be able to connect to wifi networks using network manager,"
    info "but you can still use the interface for monitor mode and CSI extraction."

    info "Checking nmcli device status..."
    NMCLI_OUT=$(nmcli device status 2>&1 || true)
    echo "$NMCLI_OUT"
    if echo "$NMCLI_OUT" | grep -q "wlan0"; then
        if echo "$NMCLI_OUT" | grep "wlan0" | grep -q "unmanaged"; then
            ok "wlan0 is unmanaged — as expected after nexmon firmware swap."
        else
            warn "wlan0 found but may not be unmanaged. Review the nmcli output above."
        fi
    else
        warn "wlan0 not visible in nmcli output."
    fi

    info "Checking firmware version in dmesg..."
    FW_POST=$(dmesg 2>&1 | grep "Firmware: BCM4345" || true)
    if [ -z "$FW_POST" ]; then
        warn "No BCM4345 firmware line in dmesg — driver may not have loaded yet."
    else
        echo "$FW_POST"
        if echo "$FW_POST" | grep -q "7_45_189"; then
            ok "Firmware version is now 7_45_189. This is expected as it does the firmware swap for us."
        else
            warn "Unexpected firmware version. Verify the patch was applied correctly."
        fi
    fi

    echo ""
    echo -e "${GREEN}${BOLD}=== Post-reboot verification complete ===${NC}"
    echo -e "To enable monitor mode and start streaming CSI (steps 16–17), run:"
    echo -e "  ${CYAN}bash nexmon_startup.sh${NC}"
    echo -e "For further instructions on streaming CSI to a remote device, refer to udp_streaming_setup.md"
    exit 0
fi

# ══════════════════════════════════════════════════════════════════════════════
# Full setup: Steps 1–14
# ══════════════════════════════════════════════════════════════════════════════

# ── Step 1 — Verify kernel version ───────────────────────────────────────────
echo -e "\n${BOLD}=== Step 1: Kernel version ===${NC}"
capture_checked "uname -r" uname -r
KERNEL_VERSION="$OUTPUT"

KERNEL_MAJOR=$(echo "$KERNEL_VERSION" | cut -d. -f1)
KERNEL_MINOR=$(echo "$KERNEL_VERSION" | cut -d. -f2)

if [ "$KERNEL_MAJOR" -lt 5 ] || { [ "$KERNEL_MAJOR" -eq 5 ] && [ "$KERNEL_MINOR" -lt 15 ]; }; then
    warn "Kernel $KERNEL_VERSION is older than 5.15. Nexmon CSI patches are tested on 5.15+."
    warn "Proceeding, but you may encounter issues."
else
    ok "Kernel $KERNEL_VERSION meets the 5.15+ requirement."
fi

# ── Step 1.1 — Verify BCM4345 firmware version ───────────────────────────────
echo -e "\n${BOLD}=== Step 1.1: BCM4345 firmware version ===${NC}"
info "Checking dmesg for BCM4345 firmware string..."
FW_LINE=$(dmesg 2>&1 | grep "Firmware: BCM4345" || true)
if [ -z "$FW_LINE" ]; then
    warn "No 'Firmware: BCM4345' line found in dmesg."
    warn "Either the WiFi driver hasn't loaded yet or this is not a Pi 4B."
    warn "Continuing — adjust the patch directory in Step 8 if needed."
else
    echo "$FW_LINE"
    ok "Firmware line found."
    FW_VER=$(echo "$FW_LINE" | grep -oE '[0-9]+_[0-9]+_[0-9]+' | head -1)
    FW_MINOR=$(echo "$FW_VER" | cut -d_ -f3)
    if [ -n "$FW_MINOR" ] && [ "$FW_MINOR" -gt 189 ]; then
        ok "Firmware version $FW_VER is higher than 7_45_189 — requirement met."
    else
        warn "Firmware version $FW_VER may not be higher than 7_45_189. Proceed with caution."
    fi
fi

# ── Step 2 — Kill wpa_supplicant ─────────────────────────────────────────────
echo -e "\n${BOLD}=== Step 2: Kill wpa_supplicant ===${NC}"
info "Stopping wpa_supplicant..."
sudo pkill wpa_supplicant 2>&1 || true
ok "wpa_supplicant stopped (or was not running)."

# ── Step 3 — Update system and install dependencies ──────────────────────────
echo -e "\n${BOLD}=== Step 3: System update and dependencies ===${NC}"
run_checked "apt update"       sudo apt update -y
run_checked "apt full-upgrade" sudo apt full-upgrade -y
run_checked "install build dependencies" sudo apt install -y \
    git libgmp3-dev gawk qpdf bison flex make autoconf libtool texinfo xxd \
    libnl-3-dev libnl-genl-3-dev bc libssl-dev tcpdump

# ── Step 4 — armhf architecture (64-bit OS only) ─────────────────────────────
echo -e "\n${BOLD}=== Step 4: 64-bit OS — add armhf architecture ===${NC}"
ARCH=$(uname -m)
if [ "$ARCH" = "aarch64" ]; then
    info "Detected 64-bit OS ($ARCH) — adding armhf architecture."
    run_checked "dpkg --add-architecture armhf" sudo dpkg --add-architecture armhf
    run_checked "apt update (armhf)"            sudo apt update -y
    run_checked "install armhf libs" sudo apt-get install -y \
        libc6:armhf libisl23:armhf libmpfr6:armhf libmpc3:armhf libstdc++6:armhf

    LIB_BASE=/usr/lib/arm-linux-gnueabihf

    if [ ! -e "$LIB_BASE/libisl.so.10" ]; then
        run_checked "symlink libisl.so.10" \
            sudo ln -s "$LIB_BASE/libisl.so.23" "$LIB_BASE/libisl.so.10"
    else
        ok "libisl.so.10 symlink already exists."
    fi

    if [ ! -e "$LIB_BASE/libmpfr.so.4" ]; then
        run_checked "symlink libmpfr.so.4" \
            sudo ln -s "$LIB_BASE/libmpfr.so.6" "$LIB_BASE/libmpfr.so.4"
    else
        ok "libmpfr.so.4 symlink already exists."
    fi
else
    info "Detected 32-bit OS ($ARCH) — skipping armhf step."
fi

# ── Step 5 — Install Python 2.7 ──────────────────────────────────────────────
echo -e "\n${BOLD}=== Step 5: Install Python 2.7 ===${NC}"
if command -v python2.7 &>/dev/null; then
    ok "python2.7 already installed at $(command -v python2.7)."
else
    info "python2.7 not found — adding Debian Stretch archive temporarily."
    sudo cp /etc/apt/sources.list /tmp/sources.list.nexmon_bak

    echo 'deb http://archive.debian.org/debian/ stretch contrib main non-free' \
        | sudo tee -a /etc/apt/sources.list

    run_checked "apt update (stretch)" sudo apt update -y
    run_checked "install python2.7"    sudo apt install -y python2.7

    sudo mv /tmp/sources.list.nexmon_bak /etc/apt/sources.list
    run_checked "apt update (restore)"  sudo apt update -y
fi

# ── Step 6 — Fetch and init the nexmon repository ────────────────────────────
echo -e "\n${BOLD}=== Step 6: Fetch and init the nexmon repository ===${NC}"
NEXMON_DIR="$HOME/nexmon"

if [ -d "$NEXMON_DIR" ]; then
    warn "Nexmon directory already exists at $NEXMON_DIR — skipping clone."
else
    run_checked "clone nexmon" git clone --depth=1 https://github.com/seemoo-lab/nexmon.git "$NEXMON_DIR"
fi

cd "$NEXMON_DIR" || die "Could not cd into $NEXMON_DIR"
# shellcheck source=/dev/null
source setup_env.sh

info "Patching b43-beautifier to use python2.7..."
sed -i '1 s/$/2.7/' "$NEXMON_ROOT/buildtools/b43-v3/debug/b43-beautifier"

info "Running make (warnings are expected — only actual errors will abort)..."
MAKE_OUT=$(make 2>&1)
MAKE_RC=$?
echo "$MAKE_OUT"

if echo "$MAKE_OUT" | grep -q "arm-none-eabi-gcc: not found"; then
    die "Step 6: arm-none-eabi-gcc not found. Ensure Step 4 (armhf architecture and libs) was completed."
fi

if [ $MAKE_RC -ne 0 ]; then
    die "Step 6 make failed (exit $MAKE_RC). Review output above."
fi

ok "Nexmon buildtools built."

# ── Step 7 — Build and install nexutil ───────────────────────────────────────
echo -e "\n${BOLD}=== Step 7: Build and install nexutil ===${NC}"
cd "$NEXMON_ROOT/utilities/nexutil" || die "nexutil directory not found."
run_checked "make install nexutil" sudo -E make install USE_VENDOR_CMD=1
run_checked "setcap nexutil"       sudo setcap cap_net_admin+ep /usr/bin/nexutil

# ── Step 8 — Fetch the nexmon_csi repository ─────────────────────────────────
echo -e "\n${BOLD}=== Step 8: Fetch the nexmon_csi repository ===${NC}"
PATCH_DIR="$NEXMON_ROOT/patches/bcm43455c0/7_45_189"
CSI_DIR="$PATCH_DIR/nexmon_csi"

if [ ! -d "$PATCH_DIR" ]; then
    die "Patch directory $PATCH_DIR not found. Verify the Nexmon build completed and that your firmware version is compatible."
fi

info "Cloning into $PATCH_DIR — it must be executed from this directory, as the scripts in the next step are built for this version."

if [ -d "$CSI_DIR" ]; then
    warn "nexmon_csi already cloned at $CSI_DIR — skipping."
else
    run_checked "clone nexmon_csi" \
        git clone --depth=1 https://github.com/seemoo-lab/nexmon_csi.git "$CSI_DIR"
fi

cd "$CSI_DIR" || die "Could not cd into $CSI_DIR"

# ── Step 9 — Install nexmon_csi firmware ─────────────────────────────────────
echo -e "\n${BOLD}=== Step 9: Install the nexmon_csi firmware ===${NC}"

install_firmware() {
    info "Running: make -f Makefile.rpi install-firmware"
    FIRMWARE_OUTPUT=$(make -f Makefile.rpi install-firmware 2>&1)
    local exit_code=$?
    echo "$FIRMWARE_OUTPUT"
    return $exit_code
}

install_firmware
FIRMWARE_RC=$?

# 9.1 — "recipe commences before first target"
if echo "$FIRMWARE_OUTPUT" | grep -q "recipe commences before first target"; then
    warn "Step 9.1: Detected 'recipe commences before first target' error."
    warn "Re-sourcing setup_env.sh and retrying..."
    # shellcheck source=/dev/null
    source "$NEXMON_ROOT/setup_env.sh"
    install_firmware
    FIRMWARE_RC=$?

    if [ $FIRMWARE_RC -ne 0 ]; then
        die "install-firmware still failed after re-sourcing setup_env.sh. Review output above."
    fi
fi

# 9.2 — arm-none-eabi-gcc not found
if echo "$FIRMWARE_OUTPUT" | grep -q "arm-none-eabi-gcc: not found"; then
    die "Step 9.2: arm-none-eabi-gcc not found. This means Step 4 (armhf libs) was not completed. Re-run the script after installing armhf support."
fi

if [ $FIRMWARE_RC -ne 0 ]; then
    die "install-firmware failed (exit $FIRMWARE_RC). See output above for details."
fi

if echo "$FIRMWARE_OUTPUT" | grep -iE '\berror\b|\bfailed\b|\bfailure\b'; then
    warn "install-firmware exited 0 but output contains error keywords — review output above."
fi

ok "Firmware installed."

# ── Step 10 — Resume the remainder of the patch ──────────────────────────────
echo -e "\n${BOLD}=== Step 10: Resume the remainder of the patch ===${NC}"
warn "NOTE: Running unmanage will take the wifi interface down. Thus, if you are connected to your pi via wifi and there are no other SSIDs it can connect to, conect it via ethenet, otherwise you will lose access to your pi unless you connect peripherals to it."
warn "You have 10 seconds to abort (Ctrl-C)."
sleep 10

run_checked "unmanage wlan0"  make -f Makefile.rpi unmanage
run_checked "reload firmware" make -f Makefile.rpi reload-full

# ── Step 11 — Go to makecsiparams ────────────────────────────────────────────
echo -e "\n${BOLD}=== Step 11: Go to makecsiparams in nexmon_csi utils to generate and copy the config string you'll need for the next step ===${NC}"
MCP_DIR="$CSI_DIR/utils/makecsiparams"
cd "$MCP_DIR" || die "makecsiparams directory not found at $MCP_DIR"

run_checked "make makecsiparams" make

info "Generating CSI config string for channel 36 / 80 MHz, 1 core, 1 stream..."
info "or whatever channel and bandwidth you want to use, it should output something like this and close, \"KuABEQAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA==\""
CONFIG_STRING=$(./makecsiparams -c 36/80 -C 1 -N 1 2>&1)
MCP_RC=$?
echo "$CONFIG_STRING"

if [ $MCP_RC -ne 0 ] || echo "$CONFIG_STRING" | grep -iE '\berror\b|\bfailed\b'; then
    warn "makecsiparams may have failed. Review output above."
    warn "Regenerate manually: ./makecsiparams -c <channel>/<bw> -C <cores> -N <streams>"
else
    ok "CSI config string: $CONFIG_STRING"

    mkdir -p "$(dirname "$CONFIG_SAVE_FILE")"
    echo "$CONFIG_STRING" > "$CONFIG_SAVE_FILE"
    ok "Config string saved to $CONFIG_SAVE_FILE for use by nexmon_startup.sh on future startups."
fi

cd "$CSI_DIR" || die "Could not cd back to $CSI_DIR"

# ── Step 12 — Configure the CSI extractor and activate monitor mode ──────────
echo -e "\n${BOLD}=== Step 12: Configure the CSI extractor and activate monitor mode ===${NC}"

if [ -z "$CONFIG_STRING" ] || echo "$CONFIG_STRING" | grep -iE '\berror\b|\bfailed\b'; then
    warn "No valid config string available — skipping nexutil configuration."
    warn "Run manually once you have a valid config string:"
    warn "  nexutil -s500 -b -l34 -v<config-string>"
    warn "  nexutil -m1"
else
    info "Applying config string..."
    NEXUTIL_OUT=$(nexutil -s500 -b -l34 -v"$CONFIG_STRING" 2>&1)
    NEXUTIL_RC=$?
    echo "$NEXUTIL_OUT"
    if [ $NEXUTIL_RC -ne 0 ] || echo "$NEXUTIL_OUT" | grep -iE '\berror\b|\bfailed\b|\bnot found\b'; then
        warn "nexutil configuration may have failed — review output above."
    else
        ok "CSI configuration applied."
    fi

    info "Enabling monitor mode..."
    MON_OUT=$(nexutil -m1 2>&1)
    MON_RC=$?
    echo "$MON_OUT"
    if [ $MON_RC -ne 0 ] || echo "$MON_OUT" | grep -iE '\berror\b|\bfailed\b'; then
        warn "Monitor mode activation may have failed — review output above."
    else
        ok "Monitor mode enabled."
    fi
fi

# ── Step 13 — Demo capturing CSI UDPs using tcpdump ─────────────────────────
echo -e "\n${BOLD}=== Step 13: Demo capturing CSI UDPs using tcpdump ===${NC}"
info "NOTE: To reset the firmware to its default, and give control back to network manager, run:"
info "  make -f Makefile.rpi restore-wifi"
# This demo blocks until Ctrl-C, which would hang any non-interactive run (e.g.
# the RuView desktop app driving this over SSH). Only run it on a real TTY.
if [ -t 0 ]; then
    info "Starting tcpdump on wlan0 port 5500. Press Ctrl-C to stop and continue to reboot."
    sudo tcpdump -i wlan0 dst port 5500 2>&1 || true
else
    info "Non-interactive run: skipping the tcpdump demo (Step 13)."
fi

# ── Step 14 — Reboot the system ──────────────────────────────────────────────
echo -e "\n${BOLD}=== Step 14: Reboot the system ===${NC}"
echo ""
echo -e "${YELLOW}After the Pi reboots, run this script again with --post-reboot to complete step 15:${NC}"
echo -e "  ${CYAN}bash $(realpath "$0") --post-reboot${NC}"
echo ""
echo -e "${YELLOW}On every subsequent startup, run nexmon_startup.sh to re-enable monitor mode (steps 16–17):${NC}"
echo -e "  ${CYAN}bash nexmon_startup.sh${NC}"
echo ""
# Stable machine-readable marker so an automated driver (RuView desktop) can
# tell an intentional reboot apart from a mid-install SSH failure. Do not remove.
echo "RUVIEW_NEXMON_REBOOT"
warn "Rebooting in 5 seconds — Ctrl-C to abort."
sleep 5
sudo reboot
