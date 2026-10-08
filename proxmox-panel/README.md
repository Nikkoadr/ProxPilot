# Proxmox Panel — Rust + SB Admin 2

Panel untuk provisioning Kubernetes cluster di Proxmox VE. Backend **Rust (Axum)**,
frontend **SB Admin 2** (static, no build), realtime via **WebSocket + polling**,
storage **SQLite**, login **admin + password (dapat diubah)**.

**Auth Proxmox: API Token.** **Eksekusi: WSL lokal, atau WSL → `ssh` ke server Proxmox.**

---

## Install 1 perintah (customer)

**Linux / WSL (Ubuntu/Debian)** — install ansible + terraform + ssh key + binary + systemd service:

```bash
curl -fsSL https://raw.githubusercontent.com/<USER>/proxmox-panel/main/proxmox-panel/install.sh | bash
```

Env opsional: `PANEL_REPO=owner/repo PANEL_VERSION=v2.1.0 ADMIN_USER=admin ADMIN_PASS=rahasia DATA_DIR=/var/lib/proxmox-panel NO_SERVICE=1`.

**Windows** — download exe release + autostart (Scheduled Task):

```cmd
set REPO=owner/repo
install.bat
```

> Ganti `<USER>`/`owner/repo` dengan repo GitHub kamu (lihat "Release" di bawah).
> Deps Linux (ansible/terraform) tetap di-install via `install.sh` di dalam WSL.

Buka **http://localhost:8080** · login default **`admin / admin123`** → segera ganti di **Settings**.

## Release (maintainer)

```bash
cd proxmox-panel
cargo build --release                       # -> target/release/proxmox-panel
# Linux:  cross atau build di WSL/CI -> upload sebagai proxmox-panel-linux-x86_64
# Windows: target/release/proxmox-panel.exe -> upload sebagai proxmox-panel-windows-x86_64.exe
```

Buat GitHub Release dengan tag (mis. `v2.1.0`) berisi kedua asset di atas,
lalu arahkan `PANEL_REPO`/`REPO` ke repo tersebut. Contoh workflow CI: build
matrix `ubuntu-latest` + `windows-latest`, upload artifacts ke release.

## Service / restart tetap jalan

* **Linux/systemd** (dibuat otomatis oleh `install.sh`): `systemctl status proxmox-panel`
* **WSL tanpa systemd**: aktifkan systemd (`[boot] systemd=true` di `/etc/wsl.conf`, lalu `wsl --shutdown`), atau jalankan manual `PANEL_DATA=... proxmox-panel`
* **Windows**: Scheduled Task `ProxmoxPanel` (on logon) dari `install.bat`

## Data & login

* SQLite: `$PANEL_DATA/panel.db` (default `./data/panel.db`) — clusters, logs (max 1000/cluster), users, sessions. Restart-safe; deploy yang terpotong saat restart ditandai `error`.
* Seed login pertama: `ADMIN_USER`/`ADMIN_PASS` (default `admin/admin123`), flag `default_creds` memunculkan banner kuning sampai password diganti.
* Session cookie `panel_session` 7 hari; ganti password = logout semua sesi.
* API tanpa login → `401`; halaman HTML tanpa login → dialihkan ke `/login.html`.

---

## Requirements

- **Rust** 1.75+ (`cargo --version`)
- **WSL** (Ubuntu) — untuk `terraform` / `ansible` / `ssh`. Tanpa WSL panel tetap jalan (simulated mode).
- Opsional di dalam WSL: `terraform`, `ansible`
- **Proxmox VE** + API Token: Datacenter → Access → API Tokens (`root@pam!panel`), uncheck *Privilege Separation* kalau mau full.

## Quick Start (Windows)

```
proxmox-panel\start.bat        :: build release + run :8080
proxmox-panel\start-dev.bat    :: cargo run (dev)
```

Buka: **http://localhost:8080** · Health: **/health.html**

## SSH via WSL (remote mode)

Di PowerShell sekali saja:

```powershell
wsl bash -lc "ssh-keygen -t ed25519 -N '' -f ~/.ssh/id_ed25519; ssh-copy-id root@192.168.1.100"
wsl bash -lc "ssh -o BatchMode=yes root@192.168.1.100 'echo ok; pveversion | head -1'"
```

Lalu di form New Cluster: centang *Remote mode*, isi SSH host/user/port, klik **Test SSH via WSL**.
Backend menjalankan: `wsl ssh -o BatchMode=yes -o ConnectTimeout=8 -p PORT user@host "..."`.

## API Endpoints

| Method | Endpoint | Deskripsi |
|--------|----------|-----------|
| GET | `/api/health` | Server time + status WSL |
| GET | `/api/tools` | Versi terraform/ansible/ssh (lokal & WSL) |
| GET | `/api/realtime/summary` | Counter clusters + WSL (poll 5s, dashboard) |
| POST | `/api/health/proxmox-test` | Test Proxmox API Token `{proxmox_url, proxmox_user, token_id, token_secret, verify_tls}` |
| POST | `/api/ssh/test` | Test SSH via WSL `{ssh_host, ssh_user, ssh_port}` |
| GET/POST | `/api/clusters` | List / create (wajib `name` + `token_secret`) |
| GET | `/api/clusters/:id/status` | Status + progress + logs |
| POST | `/api/clusters/:id/deploy` | Deploy async (background task) |
| DELETE | `/api/clusters/:id` | Hapus |
| GET | `/api/nodes` | Live dari Proxmox bila token valid, else mock |
| GET | `/api/templates` | Daftar template |
| GET | `/api/config` | Defaults |
| GET | `/api/ws/logs/:id` | WebSocket log realtime + heartbeat 5s |

## Flow Deployment

1. Isi form (token + target node + template) → **Test Proxmox** harus OK.
2. Pilih mode: kosongkan SSH host = **WSL lokal**; isi SSH host = **remote via `wsl ssh`**.
3. Create → Deploy → backend:
   - test Proxmox `/version` (live),
   - tulis `infra/terraform/<id>/{main.tf, terraform.tfvars (redacted), inventory.ini}`,
   - lokal: cek tools di WSL + `terraform init -backend=false` (proof, non-destruktif),
   - remote: `ssh_test_via_wsl` + cek `terraform/ansible --version` di server,
   - fase K8s (kubeadm, Calico, Nginx) sebagai log progres,
   - status `running` 100%.
4. Log mengalir realtime ke `cluster-detail.html` via WS; kalau WS putus, polling 3s backup.

## Struktur

```
proxmox-panel/
├── Cargo.toml
├── src/
│   ├── main.rs      # Axum + ServeDir(static/) + fallback index.html
│   ├── routes.rs    # REST + WS + deployment engine + terraform filegen
│   ├── models.rs    # Cluster (token_id/token_secret, ssh_host/port), LogEntry
│   ├── store.rs     # DashMap + broadcast channel realtime
│   ├── proxmox.rs   # PVEAPIToken client, /version, /nodes
│   └── exec.rs      # wsl bash -lc, ssh_test_via_wsl, tools_summary
├── static/          # SB Admin 2 (CDN): index, new-cluster, cluster-detail, health + js/
├── infra/terraform/ # generated per-cluster (gitignored)
├── start.bat / start-dev.bat / Makefile
```

## Troubleshooting

- **WSL unavailable** → install WSL2 + Ubuntu; panel tetap jalan simulated.
- **Proxmox 401** → token_id salah (isi hanya bagian setelah `!`), atau privilege separation.
- **SSL error** → biarkan *Verify TLS* off (self-signed default Proxmox).
- **SSH FAILED / BatchMode** → key belum di-copy: `ssh-copy-id` dari dalam WSL.
- **terraform not found in WSL** → install di WSL (HashiCorp apt repo); deploy tetap simulasi sampai ada.
