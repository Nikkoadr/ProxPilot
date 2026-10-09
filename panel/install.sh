#!/usr/bin/env bash
# ProxPilot Panel — one-line installer (Ubuntu / Debian / WSL / Rocky / RHEL).
#
#   curl -fsSL https://raw.githubusercontent.com/Nikkoadr/ProxPilot/master/panel/install.sh | bash
#
# Env overrides:
#   PANEL_REPO=owner/repo   GitHub repo hosting releases (default Nikkoadr/ProxPilot)
#   PANEL_VERSION=v2.1.0    release tag, or "latest"
#   ADMIN_USER / ADMIN_PASS  seed login (default admin / admin123, change in Settings!)
#   DATA_DIR                sqlite + state dir (default /var/lib/proxpilot)
#   NO_SERVICE=1            skip systemd service, just install binary + deps
set -euo pipefail

REPO="${PANEL_REPO:-Nikkoadr/ProxPilot}"
VERSION="${PANEL_VERSION:-latest}"
DATA_DIR="${DATA_DIR:-/var/lib/proxpilot}"
BIN="/usr/local/bin/proxpilot"
SERVICE="proxpilot"

# Repo PRIVATE: export GITHUB_TOKEN=<personal-access-token> sebelum install.
# Token butuh akses baca repo (classic PAT: scope `repo`).
# Tanpa token, repo harus PUBLIC (raw + release assets butuh akses anonim).
AUTH=()
if [[ -n "${GITHUB_TOKEN:-}" ]]; then
  AUTH=(-H "Authorization: Bearer $GITHUB_TOKEN")
  echo "(private mode: using GITHUB_TOKEN)"
fi

SUDO=""
if [[ "$(id -u)" -ne 0 ]]; then SUDO="sudo"; fi

echo "==> [0/6] platform detection (os family + arch, fail fast)..."
if [[ ! -f "${OS_RELEASE_FILE:-/etc/os-release}" ]]; then
  echo "ERROR: ${OS_RELEASE_FILE:-/etc/os-release} tidak ditemukan — OS tidak dikenali." >&2
  exit 1
fi
# shellcheck disable=SC1091
source "${OS_RELEASE_FILE:-/etc/os-release}"
OS_ID="${ID:-unknown}"
OS_VER="${VERSION_ID:-}"
ARCH="$(uname -m)"
echo "      os: $OS_ID $OS_VER / arch: $ARCH"
case "$OS_ID" in
  ubuntu|debian)
    FAMILY="debian" ;;
  rocky|almalinux|rhel|ol)
    FAMILY="rhel" ;;
  fedora)
    FAMILY="rhel" ;;
  *)
    # ID_LIKE fallback untuk turunan (mis. Linux Mint -> ubuntu -> debian).
    if [[ "${ID_LIKE:-}" == *debian* ]]; then FAMILY="debian";
    elif [[ "${ID_LIKE:-}" == *rhel* ]] || [[ "${ID_LIKE:-}" == *fedora* ]]; then FAMILY="rhel";
    else
      echo "ERROR: OS '$OS_ID' belum didukung. Didukung: Ubuntu/Debian (apt) dan Rocky/RHEL/AlmaLinux (dnf)." >&2
      exit 1
    fi ;;
esac
FORCE_SOURCE=0
if [[ "$ARCH" != "x86_64" && "$ARCH" != "amd64" ]]; then
  echo "      NOTE: arch $ARCH tidak punya release asset (hanya x86_64) — build dari source."
  FORCE_SOURCE=1
fi
if [[ "$FAMILY" == "debian" ]]; then PKG="apt"; else PKG="dnf"; fi
echo "      family: $FAMILY (pkg: $PKG)"

# Migrasi sekali saja dari nama lama (proxmox-panel): hentikan service lama,
# pindahkan data (DB + cluster tidak hilang), hapus binary lama.
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
    echo "      data dipindah: $OLD_DATA -> $DATA_DIR"
  fi
  $SUDO rm -f "$OLD_BIN"
  $SUDO rm -rf /usr/share/proxmox-panel
fi

echo "==> [1/6] system deps (openssh, ansible, sqlite3, build tools)..."
if [[ "$FAMILY" == "debian" ]]; then
  $SUDO apt-get update -qq
  $SUDO apt-get install -y -qq openssh-client sshpass curl ca-certificates gpg sqlite3 ansible lsb-release build-essential pkg-config git > /dev/null
else
  # RHEL family: ansible + sshpass ada di EPEL.
  $SUDO dnf install -y -q epel-release > /dev/null
  $SUDO dnf install -y -q openssh-clients sshpass curl ca-certificates sqlite ansible git gcc make pkgconf > /dev/null
fi
echo "      ansible: $(ansible --version 2>/dev/null | head -1 || echo MISSING)"

echo "==> [1b/6] rust toolchain (cargo)..."
if ! command -v cargo >/dev/null 2>&1; then
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --default-toolchain stable --profile minimal
  export PATH="$HOME/.cargo/bin:$PATH"
fi
echo "      cargo: $(cargo --version 2>/dev/null || echo MISSING)"

echo "==> [2/6] terraform (HashiCorp repo)..."
if ! command -v terraform >/dev/null 2>&1; then
  if [[ "$FAMILY" == "debian" ]]; then
    wget -qO- https://apt.releases.hashicorp.com/gpg | $SUDO gpg --dearmor -o /usr/share/keyrings/hashicorp.gpg
    echo "deb [signed-by=/usr/share/keyrings/hashicorp.gpg] https://apt.releases.hashicorp.com $(lsb_release -cs) main" \
      | $SUDO tee /etc/apt/sources.list.d/hashicorp.list > /dev/null
    $SUDO apt-get update -qq
    $SUDO apt-get install -y -qq terraform > /dev/null
  else
    $SUDO tee /etc/yum.repos.d/hashicorp.repo > /dev/null <<'HASHIEOF'
[hashicorp]
name=HashiCorp Stable - $basearch
baseurl=https://rpm.releases.hashicorp.com/RHEL/$releasever/$basearch/stable
enabled=1
gpgcheck=1
gpgkey=https://rpm.releases.hashicorp.com/gpg
HASHIEOF
    $SUDO dnf install -y -q terraform > /dev/null
  fi
fi
echo "      terraform: $(terraform version 2>/dev/null | head -1 || echo MISSING)"

echo "==> [2b/6] firewall (hanya bila firewalld aktif, umum di Rocky)..."
if command -v firewall-cmd >/dev/null 2>&1 && $SUDO firewall-cmd --state 2>/dev/null | grep -qi running; then
  $SUDO firewall-cmd --permanent --add-port=8080/tcp > /dev/null
  $SUDO firewall-cmd --reload > /dev/null
  echo "      firewalld: port 8080/tcp dibuka"
else
  echo "      firewalld: tidak aktif / tidak ada — skip"
fi

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
CLONE_URL="https://github.com/$REPO.git"
if [[ -n "${GITHUB_TOKEN:-}" ]]; then
  CLONE_URL="https://oauth2:${GITHUB_TOKEN}@github.com/$REPO.git"
fi
if [[ "$VERSION" == "latest" ]]; then
  # '|| true' agar set -o pipefail tidak mematikan script saat API 404 (belum ada Release).
  TAG="$(curl -fsSL "${AUTH[@]}" "https://api.github.com/repos/$REPO/releases/latest" 2>/dev/null | grep -m1 '"tag_name"' | cut -d'"' -f4 || true)"
else
  TAG="$VERSION"
fi
URL="https://github.com/$REPO/releases/download/${TAG:-none}/proxpilot-linux-x86_64"
if [[ "$FORCE_SOURCE" != "1" ]] && [[ -n "${TAG:-}" ]] && curl -fsSL "${AUTH[@]}" -o /tmp/proxpilot "$URL" 2>/dev/null; then
  $SUDO install -m 0755 /tmp/proxpilot "$BIN"
  rm -f /tmp/proxpilot
  echo "      installed: $BIN ($TAG)"
else
  echo "      no release asset — building from source (this takes a few minutes)..."
  export PATH="$HOME/.cargo/bin:$PATH"
  command -v cargo >/dev/null || { echo "ERROR: cargo missing after toolchain install" >&2; exit 1; }
  rm -rf /tmp/panel-src
  git clone --depth 1 --branch master "$CLONE_URL" /tmp/panel-src
  (cd /tmp/panel-src/panel && cargo build --release)
  $SUDO install -m 0755 /tmp/panel-src/panel/target/release/proxpilot "$BIN"
  echo "      installed: $BIN (built from source)"
fi

echo "==> [4b/6] static files (/usr/share/proxpilot/static)..."
STATIC_DST="/usr/share/proxpilot/static"
if [[ -d /tmp/panel-src/panel/static ]]; then
  $SUDO mkdir -p "$STATIC_DST"
  $SUDO cp -r /tmp/panel-src/panel/static/. "$STATIC_DST/"
  rm -rf /tmp/panel-src
  echo "      static: from source checkout"
else
  rm -rf /tmp/panel-static
  if git clone --depth 1 --branch master --filter=blob:none --sparse "$CLONE_URL" /tmp/panel-static 2>/dev/null \
    && (cd /tmp/panel-static && git sparse-checkout set panel/static 2>/dev/null); then
    $SUDO mkdir -p "$STATIC_DST"
    $SUDO cp -r /tmp/panel-static/panel/static/. "$STATIC_DST/"
    rm -rf /tmp/panel-static
    echo "      static: from repo ($REPO)"
  else
    rm -rf /tmp/panel-static
    echo "      WARNING: static files not installed — panel will serve fallback pages." >&2
    echo "      Fix: git clone $REPO and copy panel/static to $STATIC_DST" >&2
  fi
fi

echo "==> [5/6] data dir ($DATA_DIR)..."
$SUDO mkdir -p "$DATA_DIR/infra/terraform"
if [[ "$(id -u)" -ne 0 ]]; then $SUDO chown -R "$(id -u):$(id -g)" "$DATA_DIR"; fi

echo "==> [5b/6] update command (panel-update)..."
RAW_BASE="https://raw.githubusercontent.com/$REPO/master/panel"
if curl -fsSL "${AUTH[@]}" -o /tmp/panel-update "$RAW_BASE/update.sh" 2>/dev/null; then
  $SUDO install -m 0755 /tmp/panel-update /usr/local/bin/panel-update
  rm -f /tmp/panel-update
  echo "      installed: panel-update"
else
  echo "      skip (update.sh not in repo yet — push dulu file-nya)"
fi

if [[ "${NO_SERVICE:-0}" == "1" ]]; then
  echo "      NO_SERVICE=1, skipping systemd"
else
echo "==> [6/6] systemd service ($SERVICE)..."
if [[ "$(ps -p 1 -o comm= 2>/dev/null)" == "systemd" ]]; then
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
  echo "      service active: $(systemctl is-active $SERVICE)"
else
  echo "      no systemd (WSL without systemd?) — run manually or enable WSL systemd:"
  echo "        PANEL_DATA=$DATA_DIR $BIN"
  echo "      WSL systemd: add '[boot] systemd=true' to /etc/wsl.conf, then 'wsl --shutdown'"
fi
fi

echo
echo "==================================================="
echo " Proxmox Panel ready! (rust + deps + binary + service)"
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
