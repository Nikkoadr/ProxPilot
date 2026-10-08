#!/usr/bin/env bash
# Proxmox Panel — dev runner (Linux / WSL).
# Usage: ./start.sh   (env: PORT=8080 PANEL_DATA=./data ADMIN_USER=admin ADMIN_PASS=admin123)
set -euo pipefail
cd "$(dirname "$0")"
command -v cargo >/dev/null || { echo "ERROR: install Rust dulu di WSL (lihat README)"; exit 1; }
export PORT="${PORT:-8080}"
export PANEL_DATA="${PANEL_DATA:-./data}"
# Target dir terpisah dari build Windows (./target) agar tidak saling timpa.
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$PWD/target-linux}"
echo "Panel dev: http://localhost:$PORT  (data: $PANEL_DATA/panel.db)"
exec cargo run
