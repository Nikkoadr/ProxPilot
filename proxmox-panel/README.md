# Proxmox Panel — Rust + SB Admin 2

Panel untuk provisioning Kubernetes cluster di Proxmox VE. Backend **Rust (Axum)**,
frontend **SB Admin 2** (static, no build), realtime via **WebSocket + polling**,
storage **SQLite**, login **admin + password (dapat diubah)**.

**Auth Proxmox: API Token.** **Eksekusi lokal di host, atau `ssh` ke server Proxmox.**

---

## Install 1 perintah (WSL / Ubuntu / Debian)

```bash
curl -fsSL https://raw.githubusercontent.com/Nikkoadr/ansible/main/proxmox-panel/install.sh | bash
```

Itu saja — script menginstall ansible + terraform + openssh/sqlite3, generate SSH key,
download binary dari GitHub Release, dan pasang systemd service (restart otomatis).

Env opsional: `PANEL_REPO=Nikkoadr/ansible PANEL_VERSION=v2.1.0 ADMIN_USER=admin ADMIN_PASS=rahasia DATA_DIR=/var/lib/proxmox-panel NO_SERVICE=1`.

Buka **http://localhost:8080** · login default **`admin / admin123`** → segera ganti di **Settings**.

## Dev (WSL)

```bash
cd proxmox-panel
./start.sh        # PORT=8080 PANEL_DATA=./data cargo run
# atau: make dev
```

## Release (maintainer)

Build binary Linux dan upload ke GitHub Release repo `Nikkoadr/ansible` dengan tag
(mis. `v2.1.0`) sebagai asset **`proxmox-panel-linux-x86_64`**:

```bash
cd proxmox-panel
cargo build --release
# upload target/release/proxmox-panel sebagai proxmox-panel-linux-x86_64
```

Contoh via GitHub CLI:

```bash
gh release create v2.1.0 target/release/proxmox-panel#proxmox-panel-linux-x86_64 --repo Nikkoadr/ansible
```

`install.sh` mengambil `latest` secara default (`PANEL_VERSION` untuk pin versi).

## Service / restart tetap jalan

systemd unit dibuat otomatis oleh `install.sh`: `systemctl status proxmox-panel`.
WSL tanpa systemd: aktifkan systemd (`[boot] systemd=true` di `/etc/wsl.conf`, lalu `wsl --shutdown`), atau jalankan manual `PANEL_DATA=... proxmox-panel`.

## Data & login

* SQLite: `$PANEL_DATA/panel.db` (default `./data/panel.db`) — clusters, logs (max 1000/cluster), users, sessions. Restart-safe; deploy yang terpotong saat restart ditandai `error`.
* Seed login pertama: `ADMIN_USER`/`ADMIN_PASS` (default `admin/admin123`), flag `default_creds` memunculkan banner kuning sampai password diganti.
* Session cookie `panel_session` 7 hari; ganti password = logout semua sesi.
* API tanpa login → `401`; halaman HTML tanpa login → dialihkan ke `/login.html`.

---

## Requirements (dipasang otomatis oleh `install.sh`)

- Ubuntu / Debian / WSL + `ansible`, `terraform`, `openssh-client`
- **Proxmox VE** + API Token: Datacenter → Access → API Tokens (`root@pam!panel`), uncheck *Privilege Separation* kalau mau full.

## SSH key (remote mode, sekali saja)

```bash
ssh-keygen -t ed25519 -N '' -f ~/.ssh/id_ed25519
ssh-copy-id -p 2902 root@103.156.16.153
ssh -o BatchMode=yes -p 2902 root@103.156.16.153 'echo ok; pveversion | head -1'
```

Lalu di form New Cluster: centang *Remote mode*, isi SSH host/user/port, klik **Test SSH**.
Backend menjalankan: `ssh -o BatchMode=yes -o ConnectTimeout=8 -p PORT user@host "..."`.

## API Endpoints

| Method | Endpoint | Deskripsi |
|--------|----------|-----------|
| GET | `/api/health` | Server time + status runtime |
| GET | `/api/tools` | Versi terraform/ansible/ssh (native + compat) |
| GET | `/api/realtime/summary` | Counter clusters (poll 5s, dashboard) |
| POST | `/api/health/proxmox-test` | Test Proxmox API Token `{proxmox_url, proxmox_user, token_id, token_secret, verify_tls}` |
| POST | `/api/ssh/test` | Test SSH `{ssh_host, ssh_user, ssh_port}` (+ diagnosa key saat gagal) |
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
2. Pilih mode: kosongkan SSH host = **eksekusi lokal**; isi SSH host = **remote via `ssh`**.
3. Create → Deploy → backend:
   - test Proxmox `/version` (live),
   - tulis `infra/terraform/<id>/{main.tf, terraform.tfvars (redacted), inventory.ini}`,
   - lokal: cek tools + `terraform init -backend=false` (proof, non-destruktif),
   - remote: test SSH + cek `terraform/ansible --version` di server,
   - fase K8s (kubeadm, Calico, Nginx) sebagai log progres,
   - status `running` 100%.
4. Log mengalir realtime ke `cluster-detail.html` via WS; kalau WS putus, polling 3s backup.

## Struktur

```
proxmox-panel/
├── Cargo.toml
├── src/
│   ├── main.rs      # Axum + static + login guard
│   ├── routes.rs    # REST + WS + deployment engine + terraform filegen
│   ├── models.rs    # Cluster (token_id/token_secret, ssh_host/port), LogEntry
│   ├── store.rs     # SQLite-backed state + broadcast realtime
│   ├── db.rs        # SQLite (clusters, logs, users, sessions, settings)
│   ├── auth.rs      # login cookie + argon2 + ganti password
│   ├── proxmox.rs   # PVEAPIToken client, /version, /nodes
│   └── exec.rs      # exec lokal, ssh, tools_summary + diagnosa auth
├── static/          # SB Admin 2 (CDN): login, index, new-cluster, cluster-detail, health, settings + js/
├── infra/terraform/ # generated per-cluster (gitignored)
├── data/            # panel.db SQLite (gitignored, via PANEL_DATA)
├── install.sh       # one-line installer (apt deps + key + binary + systemd)
├── start.sh / Makefile
```

## Troubleshooting

- **WSL unavailable** → install WSL2 + Ubuntu; panel tetap jalan simulated.
- **Proxmox 401** → token_id salah (isi hanya bagian setelah `!`), atau privilege separation.
- **SSL error** → biarkan *Verify TLS* off (self-signed default Proxmox).
- **SSH FAILED / BatchMode** → key belum di-copy: `ssh-copy-id` dari dalam WSL.
- **terraform not found in WSL** → install di WSL (HashiCorp apt repo); deploy tetap simulasi sampai ada.
