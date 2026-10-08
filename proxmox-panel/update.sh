#!/usr/bin/env bash
# panel-update — update binary Proxmox Panel, restart service, data tetap.
# Usage: sudo panel-update [latest|panel-vX.Y.Z]   (default: latest)
set -euo pipefail

REPO="${PANEL_REPO:-Nikkoadr/ansible}"
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

if [[ "$(ps -p 1 -o comm= 2>/dev/null)" == "systemd" ]]; then
  $SUDO systemctl restart "$SERVICE"
  echo "service: $(systemctl is-active $SERVICE) ($TAG)"
else
  echo "no systemd — restart manual panelnya (binary sudah diganti: $TAG)"
fi
echo "Data (SQLite) tidak disentuh. Selesai."
