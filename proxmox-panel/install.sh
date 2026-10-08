#!/usr/bin/env bash
# Proxmox Panel — one-line installer (Ubuntu / Debian / WSL / any systemd Linux).
#
#   curl -fsSL https://raw.githubusercontent.com/<USER>/proxmox-panel/main/proxmox-panel/install.sh | bash
#
# Env overrides:
#   PANEL_REPO=owner/repo   GitHub repo hosting releases (REQUIRED — edit default below)
#   PANEL_VERSION=v2.1.0    release tag, or "latest"
#   ADMIN_USER / ADMIN_PASS  seed login (default admin / admin123, change in Settings!)
#   DATA_DIR                sqlite + state dir (default /var/lib/proxmox-panel)
#   NO_SERVICE=1            skip systemd service, just install binary + deps
set -euo pipefail

REPO="${PANEL_REPO:-<USER>/proxmox-panel}"
VERSION="${PANEL_VERSION:-latest}"
DATA_DIR="${DATA_DIR:-/var/lib/proxmox-panel}"
BIN="/usr/local/bin/proxmox-panel"
SERVICE="proxmox-panel"

if [[ "$REPO" == "<USER>/proxmox-panel" ]]; then
  echo "ERROR: edit PANEL_REPO or export PANEL_REPO=owner/repo (your GitHub releases repo)." >&2
  exit 1
fi

SUDO=""
if [[ "$(id -u)" -ne 0 ]]; then SUDO="sudo"; fi

echo "==> [1/6] system deps (openssh, ansible, sqlite3)..."
$SUDO apt-get update -qq
$SUDO apt-get install -y -qq openssh-client curl ca-certificates gpg sqlite3 ansible lsb-release > /dev/null
echo "      ansible: $(ansible --version 2>/dev/null | head -1 || echo MISSING)"

echo "==> [2/6] terraform (HashiCorp repo)..."
if ! command -v terraform >/dev/null 2>&1; then
  wget -qO- https://apt.releases.hashicorp.com/gpg | $SUDO gpg --dearmor -o /usr/share/keyrings/hashicorp.gpg
  echo "deb [signed-by=/usr/share/keyrings/hashicorp.gpg] https://apt.releases.hashicorp.com $(lsb_release -cs) main" \
    | $SUDO tee /etc/apt/sources.list.d/hashicorp.list > /dev/null
  $SUDO apt-get update -qq
  $SUDO apt-get install -y -qq terraform > /dev/null
fi
echo "      terraform: $(terraform version 2>/dev/null | head -1 || echo MISSING)"

echo "==> [3/6] ssh key (~/.ssh/id_ed25519)..."
if [[ ! -f "$HOME/.ssh/id_ed25519" ]]; then
  ssh-keygen -t ed25519 -N '' -f "$HOME/.ssh/id_ed25519" -q
  echo "      new key generated"
else
  echo "      key already exists, keeping it"
fi
chmod 700 "$HOME/.ssh" 2>/dev/null || true
chmod 600 "$HOME/.ssh/id_ed25519" 2>/dev/null || true

echo "==> [4/6] panel binary ($REPO @ $VERSION)..."
if [[ "$VERSION" == "latest" ]]; then
  TAG="$(curl -fsSL "https://api.github.com/repos/$REPO/releases/latest" | grep -m1 '"tag_name"' | cut -d'"' -f4)"
else
  TAG="$VERSION"
fi
[[ -z "${TAG:-}" ]] && { echo "ERROR: cannot resolve release tag" >&2; exit 1; }
URL="https://github.com/$REPO/releases/download/$TAG/proxmox-panel-linux-x86_64"
curl -fsSL -o /tmp/proxmox-panel "$URL"
$SUDO install -m 0755 /tmp/proxmox-panel "$BIN"
rm -f /tmp/proxmox-panel
echo "      installed: $BIN ($TAG)"

echo "==> [5/6] data dir ($DATA_DIR)..."
$SUDO mkdir -p "$DATA_DIR/infra/terraform"
if [[ "$(id -u)" -ne 0 ]]; then $SUDO chown -R "$(id -u):$(id -g)" "$DATA_DIR"; fi

if [[ "${NO_SERVICE:-0}" == "1" ]]; then
  echo "      NO_SERVICE=1, skipping systemd"
else
echo "==> [6/6] systemd service ($SERVICE)..."
if [[ "$(ps -p 1 -o comm= 2>/dev/null)" == "systemd" ]]; then
  ADMIN_USER="${ADMIN_USER:-admin}" ADMIN_PASS="${ADMIN_PASS:-admin123}"
  $SUDO tee "/etc/systemd/system/$SERVICE.service" > /dev/null <<EOF
[Unit]
Description=Proxmox Panel (Rust + SB Admin 2)
After=network-online.target
Wants=network-online.target

[Service]
Type=simple
User=$(id -un)
Environment=PORT=8080
Environment=PANEL_DATA=$DATA_DIR
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
  echo "      service active: $(systemctl is-active $SERVICE)"
else
  echo "      no systemd (WSL without systemd?) — run manually or enable WSL systemd:"
  echo "        PANEL_DATA=$DATA_DIR $BIN"
  echo "      WSL systemd: add '[boot] systemd=true' to /etc/wsl.conf, then 'wsl --shutdown'"
fi
fi

echo
echo "==================================================="
echo " Proxmox Panel ready!"
echo " URL   : http://localhost:8080"
echo " Login : ${ADMIN_USER:-admin} / (your ADMIN_PASS or admin123)"
echo " DB    : $DATA_DIR/panel.db"
echo "---------------------------------------------------"
echo " Authorize THIS machine on Proxmox (run once):"
echo "   ssh-copy-id -p <ssh-port> root@<proxmox-ip>"
echo " Your public key:"
cat "$HOME/.ssh/id_ed25519.pub"
echo "==================================================="
echo " Change default password: login -> Settings."
