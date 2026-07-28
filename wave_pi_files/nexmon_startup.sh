#!/bin/bash

# Automates steps 16-17 from nexmon_setup.md for Raspberry Pi 4B.
# Run this on every Pi startup after the initial nexmon setup is complete.
# It unblocks WiFi, brings the interface up, applies the CSI config, and
# enables monitor mode so CSI capture can begin immediately.
#
# Usage:
#   ./nexmon_startup.sh                    — use saved CSI config string
#   ./nexmon_startup.sh --config <string>  — override with a specific config string
#   ./nexmon_startup.sh --no-tcpdump       — skip the demo tcpdump at the end
#
# The CSI config string is saved automatically by nexmon_setup_auto.sh.
# To regenerate it manually:
#   cd $NEXMON_ROOT/patches/bcm43455c0/7_45_189/nexmon_csi/utils/makecsiparams
#   ./makecsiparams -c 36/80 -C 1 -N 1

set -o pipefail

# ── colours ──────────────────────────────────────────────────────────────────
RED='\033[0;31m'; YELLOW='\033[1;33m'; GREEN='\033[0;32m'
CYAN='\033[0;36m'; BOLD='\033[1m'; NC='\033[0m'

CONFIG_SAVE_FILE="$HOME/.config/nexmon/csi_config"

info()  { echo -e "${CYAN}[INFO]${NC} $*"; }
ok()    { echo -e "${GREEN}[OK]${NC}   $*"; }
warn()  { echo -e "${YELLOW}[WARN]${NC} $*"; }
die()   { echo -e "${RED}[FATAL]${NC} $*" >&2; exit 1; }

# ── Parse arguments ───────────────────────────────────────────────────────────
CONFIG_STRING=""
NO_TCPDUMP=false

while [[ $# -gt 0 ]]; do
    case "$1" in
        --config)
            [ -z "${2:-}" ] && die "--config requires an argument."
            CONFIG_STRING="$2"
            shift 2
            ;;
        --no-tcpdump)
            NO_TCPDUMP=true
            shift
            ;;
        *)
            die "Unknown argument: $1. Usage: $0 [--config <string>] [--no-tcpdump]"
            ;;
    esac
done

# ── Load saved config if not provided ────────────────────────────────────────
if [ -z "$CONFIG_STRING" ]; then
    if [ -f "$CONFIG_SAVE_FILE" ]; then
        CONFIG_STRING=$(cat "$CONFIG_SAVE_FILE")
        info "Loaded CSI config string from $CONFIG_SAVE_FILE"
    else
        warn "No saved CSI config found at $CONFIG_SAVE_FILE."
        warn "Run nexmon_setup_auto.sh first, or provide a config string with --config."
        warn "To regenerate manually:"
        warn "  cd \$NEXMON_ROOT/patches/bcm43455c0/7_45_189/nexmon_csi/utils/makecsiparams"
        warn "  ./makecsiparams -c 36/80 -C 1 -N 1"
        echo ""
        read -rp "Enter your CSI config string (or press Enter to exit): " CONFIG_STRING
        if [ -z "$CONFIG_STRING" ]; then
            die "No config string provided. Exiting."
        fi
        mkdir -p "$(dirname "$CONFIG_SAVE_FILE")"
        echo "$CONFIG_STRING" > "$CONFIG_SAVE_FILE"
        ok "Config string saved to $CONFIG_SAVE_FILE for future startups."
    fi
fi

# ══════════════════════════════════════════════════════════════════════════════
# Step 16: In order to execute step 13 again, unblock from rfkill and get
# the interface up (it will still show that its down if you run
# sudo ip link show wlan0)
# ══════════════════════════════════════════════════════════════════════════════
echo -e "\n${BOLD}=== Step 16: rfkill unblock and bring wlan0 up ===${NC}"

info "Checking rfkill state — confirm that indeed wlan0 is blocked..."
rfkill list 2>&1 || warn "rfkill command not available."

info "Unblocking all wireless interfaces — this ensures that you can bring the wlan0 interface back up..."
sudo rfkill unblock all 2>&1 || warn "rfkill unblock failed — interface may not have been blocked."

info "Bringing wlan0 up..."
LINK_OUT=$(sudo ip link set wlan0 up 2>&1)
LINK_RC=$?
[ -n "$LINK_OUT" ] && echo "$LINK_OUT"

if [ $LINK_RC -ne 0 ]; then
    warn "ip link set wlan0 up exited $LINK_RC — interface may already be up or there may be a driver issue."
else
    ok "wlan0 brought up."
fi

info "Verifying wlan0 state (it may still show as down — this is expected per nexmon_setup.md)..."
ip link show wlan0 2>&1 || true

# ══════════════════════════════════════════════════════════════════════════════
# Step 17: Repeat steps 12 and 13 to start streaming CSI on the terminal again.
# ══════════════════════════════════════════════════════════════════════════════
echo -e "\n${BOLD}=== Step 17: Configure CSI extractor and activate monitor mode ===${NC}"

info "Applying CSI config string..."
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

echo ""
echo -e "${GREEN}${BOLD}=== Nexmon startup complete ===${NC}"

if [ "$NO_TCPDUMP" = false ]; then
    echo ""
    info "Starting demo CSI capture on wlan0 port 5500 (Ctrl-C to stop)..."
    info "NOTE: To reset the firmware to its default, and give control back to network manager, run:"
    info "  make -f Makefile.rpi restore-wifi"
    sudo tcpdump -i wlan0 dst port 5500 2>&1 || true
else
    echo -e "To start CSI capture, run:"
    echo -e "  ${CYAN}sudo tcpdump -i wlan0 dst port 5500${NC}"
    echo -e "For further instructions on streaming CSI to a remote device, refer to udp_streaming_setup.md"
fi
