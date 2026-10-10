#!/usr/bin/env bash
# ProxPilot panel (Go) — install di Linux server.
# Usage: curl -fsSL https://raw.githubusercontent.com/Nikkoadr/ProxPilot/master/panel/install.sh | sudo bash
# Update nanti: sudo panel-update   (git pull + rebuild + restart)
set -euo pipefail

REPO="${PANEL_REPO:-Nikkoadr/ProxPilot}"
BRANCH="${PANEL_BRANCH:-master}"
APP_DIR="${APP_DIR:-/opt/proxpilot}"
DATA_DIR="${DATA_DIR:-/var/lib/proxpilot}"
PORT="${PORT:-8080}"
ADMIN_USER="${ADMIN_USER:-admin}"
ADMIN_PASS="${ADMIN_PASS:-admin123}"
GO_VER="${GO_VER:-1.24.6}"
TF_VER="${TF_VER:-1.9.8}"

if [ "$(id -u)" != "0" ]; then echo "Jalankan sebagai root (sudo)."; exit 1; fi

echo "[1/5] deps sistem..."
if command -v apt-get >/dev/null; then
  apt-get update -y
  apt-get install -y git curl unzip ansible openssh-client python3
elif command -v dnf >/dev/null; then
  dnf install -y epel-release
  dnf install -y git curl unzip ansible openssh-clients python3
fi

if ! command -v go >/dev/null || ! go version 2>/dev/null | grep -qE 'go1\.(2[4-9]|[3-9][0-9])'; then
  echo "Install Go $GO_VER..."
  curl -sSL -o /tmp/go.tgz "https://go.dev/dl/go${GO_VER}.linux-amd64.tar.gz"
  rm -rf /usr/local/go && tar -C /usr/local -xzf /tmp/go.tgz
  ln -sf /usr/local/go/bin/go /usr/local/bin/go
fi
export PATH=/usr/local/go/bin:$PATH

if ! command -v terraform >/dev/null; then
  echo "Install Terraform $TF_VER..."
  curl -sSL -o /tmp/tf.zip "https://releases.hashicorp.com/terraform/${TF_VER}/terraform_${TF_VER}_linux_amd64.zip"
  unzip -o -q /tmp/tf.zip -d /usr/local/bin
fi

echo "[2/5] kode ke $APP_DIR..."
AUTH=""
if [ -n "${GITHUB_TOKEN:-}" ]; then AUTH="oauth2:${GITHUB_TOKEN}@"; fi
if [ -d "$APP_DIR/.git" ]; then
  git -C "$APP_DIR" pull --ff-only
else
  git clone --depth 1 -b "$BRANCH" "https://${AUTH}github.com/${REPO}" "$APP_DIR"
fi

echo "[3/5] build panel..."
cd "$APP_DIR/panel"
export GOFLAGS='-mod=mod -buildvcs=false' CGO_ENABLED=0
go build -buildvcs=false -o proxpilot .
cp "$APP_DIR/panel/update.sh" /usr/local/bin/panel-update
chmod +x /usr/local/bin/panel-update

echo "[4/5] service systemd..."
mkdir -p "$DATA_DIR"
cp "$APP_DIR/panel/proxpilot.service" /etc/systemd/system/proxpilot.service
mkdir -p /etc/default
touch /etc/default/proxpilot
systemctl daemon-reload
systemctl enable --now proxpilot
sleep 3

echo "[5/5] cek health..."
curl -s -m 10 "http://localhost:${PORT}/api/health" || {
  echo "Service tidak respon, lihat: journalctl -u proxpilot -n 50"; exit 1; }
echo
echo "OK — buka http://SERVER:${PORT} (login ${ADMIN_USER} / ganti di Settings)"
echo "Seed login pertama via env: ADMIN_USER=... ADMIN_PASS=... (hanya bila user belum ada)"
