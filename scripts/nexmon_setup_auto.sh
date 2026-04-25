#!/bin/bash

# Automated Nexmon CSI setup for Raspberry Pi 4B (kernel 5.15+).
# Mirrors every step in nexmon_setp.sh, inspects all console output for error
# keywords, handles the two documented errors automatically, and exits
# gracefully on any unexpected failure.
#
# Usage:
#   ./nexmon_setup_auto.sh               — full setup (steps 1–17, ends with reboot)
#   ./nexmon_setup_auto.sh --post-reboot — post-reboot steps (18–19) then prompts for capture

set -o pipefail

# ── colours ──────────────────────────────────────────────────────────────────
RED='\033[0;31m'; YELLOW='\033[1;33m'; GREEN='\033[0;32m'
CYAN='\033[0;36m'; BOLD='\033[1m'; NC='\033[0m'

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
    echo -e "\n${BOLD}=== Post-reboot mode: Steps 18–20 ===${NC}"

    # Step 18 — Verify unmanaged state and firmware version
    echo -e "\n${BOLD}=== Step 18: Post-reboot verification ===${NC}"
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
            ok "Firmware swapped to 7_45_189 — nexmon CSI patch is active."
        else
            warn "Unexpected firmware version. Verify the patch was applied correctly."
        fi
    fi

    # Step 19 — rfkill unblock and bring wlan0 up
    echo -e "\n${BOLD}=== Step 19: rfkill unblock and bring wlan0 up ===${NC}"
    info "Checking rfkill state..."
    rfkill list 2>&1 || warn "rfkill not available."

    info "Unblocking all wireless interfaces..."
    sudo rfkill unblock all 2>&1 || warn "rfkill unblock failed — may not have been blocked."

    run_checked "ip link set wlan0 up" sudo ip link set wlan0 up

    info "Verifying wlan0 is up..."
    IP_OUT=$(ip link show wlan0 2>&1)
    echo "$IP_OUT"
    if echo "$IP_OUT" | grep -q "UP"; then
        ok "wlan0 is up."
    else
        warn "wlan0 may still be down — check the output above."
    fi

    # Step 20 — Prompt user to repeat steps 14 and 16
    echo -e "\n${BOLD}=== Step 20: Resume CSI capture ===${NC}"
    warn "Step 20 requires the CSI config string you generated in Step 13."
    warn "If you no longer have it, regenerate it:"
    warn "  cd \$NEXMON_ROOT/patches/bcm43455c0/7_45_189/nexmon_csi/utils/makecsiparams"
    warn "  ./makecsiparams -c 36/80 -C 1 -N 1"
    echo ""
    read -rp "Enter your CSI config string (or press Enter to skip): " CONFIG_STRING

    if [ -n "$CONFIG_STRING" ]; then
        info "Applying CSI config..."
        NEXUTIL_OUT=$(nexutil -s500 -b -l34 -v"$CONFIG_STRING" 2>&1)
        NEXUTIL_RC=$?
        echo "$NEXUTIL_OUT"
        if [ $NEXUTIL_RC -ne 0 ] || echo "$NEXUTIL_OUT" | grep -iE '\berror\b|\bfailed\b|\bnot found\b'; then
            warn "nexutil config may have failed — review output above."
        else
            ok "CSI configuration applied."
        fi

        info "Enabling monitor mode..."
        MON_OUT=$(nexutil -m1 2>&1)
        MON_RC=$?
        echo "$MON_OUT"
        if [ $MON_RC -ne 0 ] || echo "$MON_OUT" | grep -iE '\berror\b|\bfailed\b'; then
            warn "Monitor mode activation may have failed."
        else
            ok "Monitor mode enabled."
        fi

        info "Starting CSI capture on wlan0 port 5500 (Ctrl-C to stop)..."
        sudo tcpdump -i wlan0 dst port 5500 2>&1 || true
    else
        warn "Skipped. Run manually when ready:"
        warn "  nexutil -s500 -b -l34 -v<config-string>"
        warn "  nexutil -m1"
        warn "  sudo tcpdump -i wlan0 dst port 5500"
    fi

    echo ""
    echo -e "${GREEN}${BOLD}=== Post-reboot setup complete ===${NC}"
    echo -e "To reset firmware and return to normal WiFi:"
    echo -e "  ${CYAN}make -f Makefile.rpi restore-wifi${NC}"
    exit 0
fi

# ══════════════════════════════════════════════════════════════════════════════
# Full setup: Steps 1–17
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
    # Extract version string e.g. "7_45_189" and compare numerically
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

# ── Step 6 — Clone and init Nexmon ───────────────────────────────────────────
echo -e "\n${BOLD}=== Step 6: Clone and initialise Nexmon ===${NC}"
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

# arm-none-eabi-gcc missing is a known fatal error at this stage too
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

# ── Step 8 — Clone nexmon_csi ────────────────────────────────────────────────
echo -e "\n${BOLD}=== Step 8: Clone nexmon_csi ===${NC}"
PATCH_DIR="$NEXMON_ROOT/patches/bcm43455c0/7_45_189"
CSI_DIR="$PATCH_DIR/nexmon_csi"

if [ ! -d "$PATCH_DIR" ]; then
    die "Patch directory $PATCH_DIR not found. Verify the Nexmon build completed and that your firmware version is compatible."
fi

info "Cloning into $PATCH_DIR — scripts in the next step are built for this firmware version."

if [ -d "$CSI_DIR" ]; then
    warn "nexmon_csi already cloned at $CSI_DIR — skipping."
else
    run_checked "clone nexmon_csi" \
        git clone --depth=1 https://github.com/seemoo-lab/nexmon_csi.git "$CSI_DIR"
fi

cd "$CSI_DIR" || die "Could not cd into $CSI_DIR"

# ── Step 9 — Install nexmon_csi firmware ─────────────────────────────────────
echo -e "\n${BOLD}=== Step 9: Install nexmon_csi firmware ===${NC}"

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

# ── Step 10 — Unmanage interface and reload firmware ─────────────────────────
echo -e "\n${BOLD}=== Step 10: Unmanage interface and reload firmware ===${NC}"
warn "This step will take wlan0 DOWN."
warn "If you are connected via WiFi only and no other SSIDs are available, you WILL lose SSH access."
warn "Connect via Ethernet before proceeding. You have 10 seconds to abort (Ctrl-C)."
sleep 10

run_checked "unmanage wlan0"  make -f Makefile.rpi unmanage
run_checked "reload firmware" make -f Makefile.rpi reload-full

# ── Step 13 — Generate CSI parameters ────────────────────────────────────────
echo -e "\n${BOLD}=== Step 13: Generate CSI parameters ===${NC}"
MCP_DIR="$CSI_DIR/utils/makecsiparams"
cd "$MCP_DIR" || die "makecsiparams directory not found at $MCP_DIR"

run_checked "make makecsiparams" make

info "Generating CSI config string for channel 36 / 80 MHz, 1 core, 1 stream..."
CONFIG_STRING=$(./makecsiparams -c 36/80 -C 1 -N 1 2>&1)
MCP_RC=$?
echo "$CONFIG_STRING"

if [ $MCP_RC -ne 0 ] || echo "$CONFIG_STRING" | grep -iE '\berror\b|\bfailed\b'; then
    warn "makecsiparams may have failed. Review output above."
    warn "Regenerate manually: ./makecsiparams -c <channel>/<bw> -C <cores> -N <streams>"
else
    ok "CSI config string: $CONFIG_STRING"
fi

cd "$CSI_DIR" || die "Could not cd back to $CSI_DIR"

# ── Step 14 — Configure CSI extractor and enable monitor mode ────────────────
echo -e "\n${BOLD}=== Step 14: Configure CSI extractor and enable monitor mode ===${NC}"

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

# ── Step 16 — Demo capture ───────────────────────────────────────────────────
echo -e "\n${BOLD}=== Step 16: Demo CSI capture ===${NC}"
info "Starting tcpdump on wlan0 port 5500. Press Ctrl-C to stop and continue to reboot."
sudo tcpdump -i wlan0 dst port 5500 2>&1 || true

# ── Step 17 — Reboot ─────────────────────────────────────────────────────────
echo -e "\n${BOLD}=== Step 17: Reboot ===${NC}"
echo ""
echo -e "${YELLOW}After the Pi reboots, run this script again with --post-reboot to complete steps 18–20:${NC}"
echo -e "  ${CYAN}bash $(realpath "$0") --post-reboot${NC}"
echo ""
warn "Rebooting in 5 seconds — Ctrl-C to abort."
sleep 5
sudo reboot
