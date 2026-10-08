#!/usr/bin/env bash
# panel-update — update binary Proxmox Panel, restart service, data tetap.
# Usage: sudo panel-update [latest|panel-vX.Y.Z]   (default: latest)
set -euo pipefail

REPO="${PANEL_REPO:-Nikkoadr/ProxPilot}"
VERSION="${1:-${PANEL_VERSION:-latest}}"
BIN="/usr/local/bin/proxmox-panel"
SERVICE="proxmox-panel"

SUDO=""
if [[ "$(id -u)" -ne 0 ]]; then SUDO="sudo"; fi

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
URL="https://github.com/$REPO/releases/download/$TAG/proxmox-panel-linux-x86_64"
if ! curl -fsSL "${AUTH[@]}" -o /tmp/proxmox-panel "$URL"; then
  echo "ERROR: asset 'proxmox-panel-linux-x86_64' tidak ada di Release $TAG." >&2
  echo "Cek: https://github.com/$REPO/releases/tag/$TAG" >&2
  echo "Rilis binary baru: push tag 'panel-vX.Y.Z' (CI build otomatis) lalu update lagi." >&2
  exit 1
fi
$SUDO install -m 0755 /tmp/proxmox-panel "$BIN"
rm -f /tmp/proxmox-panel

echo "Refreshing static files..."
STATIC_DST="/usr/share/proxmox-panel/static"
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
  $SUDO systemctl restart "$SERVICE"
  echo "service: $(systemctl is-active $SERVICE) ($TAG)"
else
  echo "no systemd — restart manual panelnya (binary sudah diganti: $TAG)"
fi
echo "Data (SQLite) tidak disentuh. Selesai."
