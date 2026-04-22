#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
BIN_DIR="${ROOT_DIR}/rust-port/wifi-densepose-rs/target/release"

if [[ ! -x "${BIN_DIR}/wifi-densepose-pi-node-agent" ]]; then
  echo "missing binary: ${BIN_DIR}/wifi-densepose-pi-node-agent"
  echo "build first: cargo build -p wifi-densepose-pi-node-agent --release"
  exit 1
fi

if [[ ! -x "${BIN_DIR}/sensing-server" ]]; then
  echo "missing binary: ${BIN_DIR}/sensing-server"
  echo "build first: cargo build -p wifi-densepose-sensing-server --release"
  exit 1
fi

if ! id -u ruview >/dev/null 2>&1; then
  sudo useradd --system --home /var/lib/ruview --create-home --shell /usr/sbin/nologin ruview
fi

sudo install -m 0755 "${BIN_DIR}/wifi-densepose-pi-node-agent" /usr/local/bin/
sudo install -m 0755 "${BIN_DIR}/sensing-server" /usr/local/bin/

sudo install -m 0644 "${ROOT_DIR}/deploy/pi/systemd/ruview-pi-agent.service" /etc/systemd/system/
sudo install -m 0644 "${ROOT_DIR}/deploy/pi/systemd/ruview-sensing-server.service" /etc/systemd/system/

sudo systemctl daemon-reload
sudo systemctl enable --now ruview-pi-agent
sudo systemctl enable --now ruview-sensing-server

echo "RuView Pi services installed and started."
