#!/usr/bin/env bash
# ProxPilot panel (Go+Gin) — install satu perintah (Linux/WSL).
# Usage: curl -fsSL .../panel/install.sh | bash
set -euo pipefail

REPO="${PANEL_REPO:-Nikkoadr/ProxPilot}"
VERSION="${PANEL_VERSION:-latest}"
DATA_DIR="${DATA_DIR:-/var/lib/proxpilot}"
ADMIN_USER="${ADMIN_USER:-admin}"
ADMIN_PASS="${ADMIN_PASS:-admin123}"

echo "[1/4] deps (go, ansible)..."
if command -v apt-get >/dev/null; then
  sudo apt-get update -y
  sudo apt-get install -y ansible openssh-client
elif command -v dnf >/dev/null; then
  sudo dnf install -y epel-release
  sudo dnf install -y ansible openssh-clients
fi
if ! command -v go >/dev/null; then
  echo "Install Go 1.24+ dulu: https://go.dev/dl (atau: sudo snap install go --classic)"
  exit 1
fi

echo "[2/4] build panel..."
TMP="$(mktemp -d)"
git clone --depth 1 "https://github.com/$REPO" "$TMP/pp" 2>/dev/null || {
  echo "clone gagal (repo private? set GITHUB_TOKEN)"; exit 1; }
cd "$TMP/pp/panel"
go build -o proxpilot .
sudo install -m 0755 proxpilot /usr/local/bin/proxpilot

echo "[3/4] data dir $DATA_DIR..."
sudo mkdir -p "$DATA_DIR"
export ADMIN_USER ADMIN_PASS
PANEL_DATA="$DATA_DIR" ADMIN_USER="$ADMIN_USER" ADMIN_PASS="$ADMIN_PASS" /usr/local/bin/proxpilot &
sleep 2

echo "[4/4] OK — buka http://localhost:8080 (login $ADMIN_USER)"
echo "Ganti password di Settings setelah login pertama."
