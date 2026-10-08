#!/usr/bin/env bash
# panel-update — update binary Proxmox Panel, restart service, data tetap.
# Usage: sudo panel-update [latest|panel-vX.Y.Z]   (default: latest)
set -euo pipefail

REPO="${PANEL_REPO:-Nikkoadr/ProxPilot}"
VERSION="${1:-${PANEL_VERSION:-latest}}"
BIN="/usr/local/bin/proxpilot"
SERVICE="proxpilot"
DATA_DIR="${DATA_DIR:-/var/lib/proxpilot}"

SUDO=""
if [[ "$(id -u)" -ne 0 ]]; then SUDO="sudo"; fi

# Migrasi sekali saja dari nama lama (lihat install.sh).
OLD_BIN="/usr/local/bin/proxmox-panel"
OLD_SERVICE="proxmox-panel"
OLD_DATA="/var/lib/proxmox-panel"
if [[ -f "$OLD_BIN" || -d "$OLD_DATA" ]]; then
  echo "(migrasi instalasi lama proxmox-panel -> proxpilot...)"
  if [[ "$(ps -p 1 -o comm= 2>/dev/null)" == "systemd" ]]; then
    $SUDO systemctl stop "$OLD_SERVICE" 2>/dev/null || true
    $SUDO systemctl disable "$OLD_SERVICE" 2>/dev/null || true
  fi
  if [[ -d "$OLD_DATA" && ! -d "$DATA_DIR" ]]; then
    $SUDO mkdir -p "$(dirname "$DATA_DIR")"
    $SUDO mv "$OLD_DATA" "$DATA_DIR"
    echo "data dipindah: $OLD_DATA -> $DATA_DIR"
  fi
  $SUDO rm -f "$OLD_BIN"
  $SUDO rm -rf /usr/share/proxmox-panel
fi

AUTH=()
if [[ -n "${GITHUB_TOKEN:-}" ]]; then
  AUTH=(-H "Authorization: Bearer $GITHUB_TOKEN")
fi

if [[ "$VERSION" == "latest" ]]; then
  TAG="$(curl -fsSL "${AUTH[@]}" "https://api.github.com/repos/$REPO/releases/latest" 2>/dev/null | grep -m1 '"tag_name"' | cut -d'"' -f4 || true)"
else
  TAG="$VERSION"
fi
if [[ -z "${TAG:-}" ]]; then
  echo "ERROR: tidak ada Release di $REPO (atau butuh GITHUB_TOKEN untuk repo private)." >&2
  exit 1
fi

echo "Updating to $TAG..."
URL="https://github.com/$REPO/releases/download/$TAG/proxpilot-linux-x86_64"
if ! curl -fsSL "${AUTH[@]}" -o /tmp/proxpilot "$URL"; then
  echo "ERROR: asset 'proxpilot-linux-x86_64' tidak ada di Release $TAG." >&2
  echo "Cek: https://github.com/$REPO/releases/tag/$TAG" >&2
  echo "Rilis binary baru: push tag 'panel-vX.Y.Z' (CI build otomatis) lalu update lagi." >&2
  exit 1
fi
$SUDO install -m 0755 /tmp/proxpilot "$BIN"
rm -f /tmp/proxpilot

echo "Refreshing static files..."
STATIC_DST="/usr/share/proxpilot/static"
CLONE_URL="https://github.com/$REPO.git"
if [[ -n "${GITHUB_TOKEN:-}" ]]; then
  CLONE_URL="https://oauth2:${GITHUB_TOKEN}@github.com/$REPO.git"
fi
rm -rf /tmp/panel-static
if git clone --depth 1 --branch master --filter=blob:none --sparse "$CLONE_URL" /tmp/panel-static 2>/dev/null \
  && (cd /tmp/panel-static && git sparse-checkout set panel/static 2>/dev/null); then
  $SUDO mkdir -p "$STATIC_DST"
  $SUDO cp -r /tmp/panel-static/panel/static/. "$STATIC_DST/"
  rm -rf /tmp/panel-static
  echo "static: refreshed ($TAG)"
else
  rm -rf /tmp/panel-static
  echo "WARNING: static files NOT refreshed (clone failed) — UI may be stale." >&2
fi

if [[ "$(ps -p 1 -o comm= 2>/dev/null)" == "systemd" ]]; then
  # Tulis ulang unit (path binary/static baru setelah rename), lalu restart.
  ADMIN_USER="${ADMIN_USER:-admin}" ADMIN_PASS="${ADMIN_PASS:-admin123}"
  $SUDO tee "/etc/systemd/system/$SERVICE.service" > /dev/null <<EOF
[Unit]
Description=ProxPilot Panel (Rust + SB Admin 2)
After=network-online.target
Wants=network-online.target

[Service]
Type=simple
User=$(id -un)
WorkingDirectory=/usr/share/proxpilot
Environment=PORT=8080
Environment=PANEL_DATA=$DATA_DIR
Environment=PANEL_STATIC=/usr/share/proxpilot/static
Environment=ADMIN_USER=$ADMIN_USER
Environment=ADMIN_PASS=$ADMIN_PASS
ExecStart=$BIN
Restart=always
RestartSec=3

[Install]
WantedBy=multi-user.target
EOF
  $SUDO systemctl daemon-reload
  $SUDO systemctl enable --now "$SERVICE"
  echo "service: $(systemctl is-active $SERVICE) ($TAG)"
else
  echo "no systemd — restart manual panelnya (binary sudah diganti: $TAG)"
fi
echo "Data (SQLite) tidak disentuh. Selesai."
