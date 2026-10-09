#!/usr/bin/env bash
# ProxPilot panel — dev runner (Linux / WSL).
# Usage: ./start.sh   (env: PORT=8080 PANEL_DATA=./data ADMIN_USER=admin ADMIN_PASS=admin123)
set -euo pipefail
cd "$(dirname "$0")"
command -v go >/dev/null || { echo "ERROR: install Go dulu (lihat README)"; exit 1; }
export PORT="${PORT:-8080}"
export PANEL_DATA="${PANEL_DATA:-./data}"
echo "Panel dev: http://localhost:$PORT  (data: $PANEL_DATA/panel_go.db)"
exec go run .
