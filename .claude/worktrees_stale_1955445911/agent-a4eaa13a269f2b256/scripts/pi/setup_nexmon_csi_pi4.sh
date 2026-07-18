#!/usr/bin/env bash
set -euo pipefail

log() { printf '[setup] %s\n' "$*"; }
die() { printf '[setup] ERROR: %s\n' "$*" >&2; exit 1; }
need_cmd() { command -v "$1" >/dev/null 2>&1 || die "missing command: $1"; }

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
RUVIEW_ROOT="$(cd "${SCRIPT_DIR}/../.." && pwd)"
NEXMON_ROOT="${NEXMON_ROOT:-$HOME/nexmon}"
PATCH_ROOT="${NEXMON_ROOT}/patches/bcm43455c0/7_45_189"
NEXMON_CSI_DST="${PATCH_ROOT}/nexmon_csi"
NEXMON_CSI_SRC="${NEXMON_CSI_SRC:-${RUVIEW_ROOT}/nexmon_csi}"

install_base_packages() {
  log "Installing build dependencies..."
  sudo apt-get update
  sudo apt-get install -y \
    git gawk qpdf bison flex make xxd automake autoconf libtool texinfo \
    libgmp3-dev python3 python3-pip iw tcpdump

  if ! sudo apt-get install -y raspberrypi-kernel-headers; then
    log "raspberrypi-kernel-headers unavailable, trying linux-headers for running kernel..."
    sudo apt-get install -y "linux-headers-$(uname -r)" || true
  fi
}

install_armhf_compat_libs() {
  if [[ "$(dpkg --print-architecture)" != "arm64" ]]; then
    return
  fi

  log "Installing armhf compatibility libs for Nexmon toolchain..."
  sudo dpkg --add-architecture armhf || true
  sudo apt-get update

  local isl_pkg=""
  local mpfr_pkg=""
  local mpc_pkg=""

  for p in libisl23 libisl22 libisl19 libisl15; do
    if apt-cache show "${p}:armhf" >/dev/null 2>&1; then
      isl_pkg="${p}:armhf"
      break
    fi
  done
  for p in libmpfr6 libmpfr4; do
    if apt-cache show "${p}:armhf" >/dev/null 2>&1; then
      mpfr_pkg="${p}:armhf"
      break
    fi
  done
  for p in libmpc3; do
    if apt-cache show "${p}:armhf" >/dev/null 2>&1; then
      mpc_pkg="${p}:armhf"
      break
    fi
  done

  local pkgs=("libc6:armhf")
  [[ -n "${isl_pkg}" ]] && pkgs+=("${isl_pkg}")
  [[ -n "${mpfr_pkg}" ]] && pkgs+=("${mpfr_pkg}")
  [[ -n "${mpc_pkg}" ]] && pkgs+=("${mpc_pkg}")

  sudo apt-get install -y "${pkgs[@]}"
}

ensure_nexmon_repo() {
  if [[ -f "${NEXMON_ROOT}/setup_env.sh" ]]; then
    log "Using existing Nexmon repo: ${NEXMON_ROOT}"
    return
  fi

  log "Cloning Nexmon into ${NEXMON_ROOT}..."
  git clone --depth=1 https://github.com/seemoo-lab/nexmon.git "${NEXMON_ROOT}"
}

ensure_nexmon_csi() {
  mkdir -p "${PATCH_ROOT}"

  if [[ -d "${NEXMON_CSI_SRC}" ]]; then
    log "Syncing local nexmon_csi from ${NEXMON_CSI_SRC} -> ${NEXMON_CSI_DST}"
    if command -v rsync >/dev/null 2>&1; then
      rsync -a --delete "${NEXMON_CSI_SRC}/" "${NEXMON_CSI_DST}/"
    else
      rm -rf "${NEXMON_CSI_DST}"
      cp -a "${NEXMON_CSI_SRC}" "${NEXMON_CSI_DST}"
    fi
    return
  fi

  if [[ -d "${NEXMON_CSI_DST}" ]]; then
    log "Using existing nexmon_csi in patch path: ${NEXMON_CSI_DST}"
    return
  fi

  log "Local nexmon_csi not found, cloning from GitHub..."
  git clone --depth=1 https://github.com/seemoo-lab/nexmon_csi.git "${NEXMON_CSI_DST}"
}

configure_git_safe_directories() {
  log "Configuring git safe.directory for root and user..."
  git config --global --add safe.directory "${NEXMON_ROOT}" || true
  git config --global --add safe.directory "${NEXMON_CSI_DST}" || true
  sudo git config --global --add safe.directory "${NEXMON_ROOT}" || true
  sudo git config --global --add safe.directory "${NEXMON_CSI_DST}" || true
}

build_and_install() {
  # shellcheck disable=SC1090
  source "${NEXMON_ROOT}/setup_env.sh"
  sudo ldconfig

  log "Building firmware assets for bcm43455c0/7_45_189..."
  make -C "${PATCH_ROOT}" -j"$(nproc)"

  log "Building and installing nexutil..."
  make -C "${NEXMON_ROOT}/utilities/nexutil" -j"$(nproc)"
  sudo make -C "${NEXMON_ROOT}/utilities/nexutil" install

  log "Building makecsiparams utility..."
  make -C "${NEXMON_CSI_DST}/utils/makecsiparams" -j"$(nproc)"

  log "Installing Nexmon CSI firmware patch..."
  if [[ -f "${NEXMON_CSI_DST}/Makefile.rpi" ]]; then
    (cd "${NEXMON_CSI_DST}" && sudo -E make -f Makefile.rpi install-firmware)
  else
    (cd "${NEXMON_CSI_DST}" && sudo -E make install-firmware)
  fi
}

main() {
  need_cmd git
  need_cmd sudo
  need_cmd apt-get

  log "RuView root: ${RUVIEW_ROOT}"
  log "Nexmon root: ${NEXMON_ROOT}"

  install_base_packages
  install_armhf_compat_libs
  ensure_nexmon_repo
  ensure_nexmon_csi
  configure_git_safe_directories
  build_and_install

  log "Done. Reboot the Pi before starting CSI capture."
}

main "$@"
