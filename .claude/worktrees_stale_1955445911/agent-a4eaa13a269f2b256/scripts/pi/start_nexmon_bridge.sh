#!/usr/bin/env bash
set -euo pipefail

log() { printf '[start] %s\n' "$*"; }
die() { printf '[start] ERROR: %s\n' "$*" >&2; exit 1; }
need_cmd() { command -v "$1" >/dev/null 2>&1 || die "missing command: $1"; }

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
RUVIEW_ROOT="$(cd "${SCRIPT_DIR}/../.." && pwd)"

NEXMON_ROOT="${NEXMON_ROOT:-$HOME/nexmon}"
NEXMON_CSI_DIR="${NEXMON_CSI_DIR:-${NEXMON_ROOT}/patches/bcm43455c0/7_45_189/nexmon_csi}"
BRIDGE_SCRIPT="${BRIDGE_SCRIPT:-${RUVIEW_ROOT}/scripts/nexmon_to_ruview_bridge.py}"

WIFI_IFACE="wlan0"
CHANNEL="1/20"
COREMASK="1"
NSSMASK="1"
LISTEN_HOST="0.0.0.0"
LISTEN_PORT="5500"
OUT_HOST=""
OUT_PORT="5005"
NODE_BASE="10"
VERBOSE_EVERY="50"
MAX_SUBCARRIERS="256"
COMPAT_LAYOUT=0
KILL_WPA_SUPPLICANT=0
TEST_ONLY=0

usage() {
  cat <<'EOF'
Usage:
  start_nexmon_bridge.sh --out-host <laptop-ip> [options]

Required:
  --out-host <ip>          Laptop IP running sensing-server.

Optional:
  --out-port <port>        Laptop UDP port (default: 5005).
  --listen-port <port>     Pi listen port for nexmon UDP (default: 5500).
  --channel <ch/bw>        Chanspec for capture, e.g. 1/20, 6/20, 36/80 (default: 1/20).
  --coremask <mask>        CSI core mask (default: 1).
  --nssmask <mask>         CSI spatial-stream mask (default: 1).
  --node-base <id>         Node base ID used by bridge (default: 10).
  --max-subcarriers <n>    Clamp subcarrier count (default: 256).
  --compat-layout          Enable legacy frame layout for older server builds.
  --kill-wpa-supplicant    Stop wpa_supplicant before enabling capture.
  --test-only              Only sniff UDP/5500 on Pi, do not start bridge.
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --out-host) OUT_HOST="$2"; shift 2 ;;
    --out-port) OUT_PORT="$2"; shift 2 ;;
    --listen-port) LISTEN_PORT="$2"; shift 2 ;;
    --channel) CHANNEL="$2"; shift 2 ;;
    --coremask) COREMASK="$2"; shift 2 ;;
    --nssmask) NSSMASK="$2"; shift 2 ;;
    --node-base) NODE_BASE="$2"; shift 2 ;;
    --max-subcarriers) MAX_SUBCARRIERS="$2"; shift 2 ;;
    --compat-layout) COMPAT_LAYOUT=1; shift ;;
    --kill-wpa-supplicant) KILL_WPA_SUPPLICANT=1; shift ;;
    --test-only) TEST_ONLY=1; shift ;;
    -h|--help) usage; exit 0 ;;
    *) die "unknown argument: $1" ;;
  esac
done

[[ -n "${OUT_HOST}" ]] || die "--out-host is required"

need_cmd python3
need_cmd sudo
need_cmd iw
need_cmd awk
need_cmd timeout
need_cmd tcpdump

[[ -f "${BRIDGE_SCRIPT}" ]] || die "bridge script not found: ${BRIDGE_SCRIPT}"
[[ -d "${NEXMON_CSI_DIR}" ]] || die "nexmon_csi dir not found: ${NEXMON_CSI_DIR}"
[[ -f "${NEXMON_ROOT}/setup_env.sh" ]] || die "setup_env.sh not found in ${NEXMON_ROOT}"

# shellcheck disable=SC1090
source "${NEXMON_ROOT}/setup_env.sh"

if ! command -v nexutil >/dev/null 2>&1; then
  if [[ -x "${NEXMON_ROOT}/utilities/nexutil/nexutil" ]]; then
    export PATH="${NEXMON_ROOT}/utilities/nexutil:${PATH}"
  else
    die "nexutil not found; run setup_nexmon_csi_pi4.sh first"
  fi
fi

MAKECSIPARAMS_BIN="${NEXMON_CSI_DIR}/utils/makecsiparams/makecsiparams"
if [[ ! -x "${MAKECSIPARAMS_BIN}" ]]; then
  log "Building makecsiparams..."
  make -C "${NEXMON_CSI_DIR}/utils/makecsiparams" -j"$(nproc)"
fi

log "Preparing Wi-Fi interface: ${WIFI_IFACE}"
sudo ip link set "${WIFI_IFACE}" up

if [[ "${KILL_WPA_SUPPLICANT}" -eq 1 ]]; then
  log "Stopping wpa_supplicant..."
  sudo pkill wpa_supplicant || true
fi

CSIPARAMS="$("${MAKECSIPARAMS_BIN}" -c "${CHANNEL}" -C "${COREMASK}" -N "${NSSMASK}")"
log "Applying CSI params on ${WIFI_IFACE} (channel ${CHANNEL})"
sudo nexutil -I"${WIFI_IFACE}" -k"${CHANNEL}" || true
sudo nexutil -I"${WIFI_IFACE}" -s500 -b -l34 -v"${CSIPARAMS}"

PHY_NAME="$(iw dev "${WIFI_IFACE}" info | awk '/wiphy/ {print "phy"$2; exit}')"
if [[ -n "${PHY_NAME}" ]]; then
  if ! iw dev | awk '$1=="Interface"{print $2}' | grep -qx mon0; then
    sudo iw phy "${PHY_NAME}" interface add mon0 type monitor || true
  fi
  sudo ip link set mon0 up || true
fi

if [[ "${TEST_ONLY}" -eq 1 ]]; then
  log "Testing for incoming CSI UDP packets on ${WIFI_IFACE}:${LISTEN_PORT} (12s)..."
  sudo timeout 12 tcpdump -ni "${WIFI_IFACE}" "udp dst port ${LISTEN_PORT}" -c 12 || true
  exit 0
fi

cmd=(
  python3 "${BRIDGE_SCRIPT}"
  --listen-host "${LISTEN_HOST}"
  --listen-port "${LISTEN_PORT}"
  --out-host "${OUT_HOST}"
  --out-port "${OUT_PORT}"
  --node-base "${NODE_BASE}"
  --max-subcarriers "${MAX_SUBCARRIERS}"
  --verbose-every "${VERBOSE_EVERY}"
)
if [[ "${COMPAT_LAYOUT}" -eq 1 ]]; then
  cmd+=(--compat-layout)
fi

log "Starting bridge: ${LISTEN_HOST}:${LISTEN_PORT} -> ${OUT_HOST}:${OUT_PORT}"
exec "${cmd[@]}"
