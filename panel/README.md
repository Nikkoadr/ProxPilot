# Proxmox Panel — Rust + SB Admin 2

Panel untuk provisioning Kubernetes cluster di Proxmox VE. Backend **Rust (Axum)**,
frontend **SB Admin 2** (static, no build), realtime via **WebSocket + polling**,
storage **SQLite**, login **admin + password (dapat diubah)**.

**Auth Proxmox: API Token.** **Eksekusi lokal di host, atau `ssh` ke server Proxmox.**

---

## Install 1 perintah (WSL / Ubuntu / Debian / Rocky / RHEL)

```bash
curl -fsSL https://raw.githubusercontent.com/Nikkoadr/ProxPilot/master/panel/install.sh | bash
```

Itu saja — script mendeteksi OS dulu (`[0/6]`, fail fast kalau tak didukung),
lalu menginstall **rust +** ansible + terraform + openssh/sqlite3, generate SSH key,
lalu pasang binary (dari GitHub Release, atau **build dari source otomatis** kalau Release belum ada),
dan pasang systemd service (restart otomatis).

Env opsional: `PANEL_REPO=Nikkoadr/ProxPilot PANEL_VERSION=v2.1.0 ADMIN_USER=admin ADMIN_PASS=rahasia DATA_DIR=/var/lib/proxpilot NO_SERVICE=1`.

### Repo private (butuh token)

`raw.githubusercontent.com` + release assets repo private tidak bisa diakses anonim —
customer harus menyertakan Personal Access Token (classic, scope **`repo`**):

```bash
curl -fsSL -H "Authorization: Bearer ghp_XXXX" \
  https://raw.githubusercontent.com/Nikkoadr/ProxPilot/master/panel/install.sh \
  | GITHUB_TOKEN=ghp_XXXX bash
```

Token diteruskan ke semua request GitHub (API + download asset). Catatan:

* Token = kredensial: bagikan hanya ke customer yang berhak, dan pakai token
  khusus installer (jangan token pribadi utama). Revoke/rotasi dari Settings GitHub bila bocor.
* Alternatif tanpa token per customer: repo **public** (cukup release + `install.sh` yang public; kode lain boleh tetap private di repo terpisah).

Buka **http://localhost:8080** · login default **`admin / admin123`** → segera ganti di **Settings**.

## Update (di WSL, setelah install)

```bash
sudo panel-update            # ke Release latest
sudo panel-update panel-v2.2.0   # ke versi tertentu
```

Update hanya mengganti binary + restart service — database SQLite (`$DATA_DIR/panel.db`),
cluster, dan user tidak disentuh. Butuh Release di GitHub (dibuat otomatis oleh CI
tiap push tag `panel-v*`, atau manual). Tanpa Release yang valid, update gagal
dengan pesan jelas (bukan setengah jalan).

## Dev (edit Windows, run WSL)

Edit file di Windows seperti biasa. Build + run selalu di WSL agar binary-nya Linux
(`start.sh` memakai `target-linux/` terpisah supaya tidak bentrok dengan `target/` Windows):

```bash
# di WSL, sekali saja: Rust + C compiler
sudo apt install -y build-essential pkg-config curl
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
source ~/.cargo/env

# tiap kali mau jalan:
cd /mnt/d/laragon/www/ProxPilot/panel   # sesuaikan path repo
./start.sh        # PORT=8080 PANEL_DATA=./data cargo run
```

## Release (2 cara, asset wajib `proxpilot-linux-x86_64`)

**A. Otomatis via CI (disarankan)** — push tag, binary di-build + ditempel ke Release:

```bash
git tag panel-v2.1.0
git push origin panel-v2.1.0
# -> .github/workflows/panel-release.yml membuat Release + asset otomatis
```

**B. Manual** — build di WSL lalu upload:

```bash
cd /mnt/d/laragon/www/ProxPilot/panel
cargo build --release
cp target/release/proxpilot ./proxpilot-linux-x86_64
gh release upload panel-v2.1.0 ./proxpilot-linux-x86_64 --repo Nikkoadr/ProxPilot
```

`install.sh` mengambil Release `latest` secara default (`PANEL_VERSION` untuk pin versi,
mis. `PANEL_VERSION=panel-v2.1.0`).

## Service / restart tetap jalan

systemd unit dibuat otomatis oleh `install.sh`: `systemctl status proxpilot`.
WSL tanpa systemd: aktifkan systemd (`[boot] systemd=true` di `/etc/wsl.conf`, lalu `wsl --shutdown`), atau jalankan manual `PANEL_DATA=... proxpilot`.

## Data & login

* SQLite: `$PANEL_DATA/panel.db` (default `./data/panel.db`) — clusters, logs (max 1000/cluster), users, sessions. Restart-safe; deploy yang terpotong saat restart ditandai `error`.
* Seed login pertama: `ADMIN_USER`/`ADMIN_PASS` (default `admin/admin123`), flag `default_creds` memunculkan banner kuning sampai password diganti.
* Session cookie `panel_session` 7 hari; ganti password = logout semua sesi.
* API tanpa login → `401`; halaman HTML tanpa login → dialihkan ke `/login.html`.

---

## Requirements (dipasang otomatis oleh `install.sh`)

- Ubuntu / Debian / WSL (apt) atau Rocky / RHEL / AlmaLinux (dnf + EPEL otomatis)
- `ansible`, `terraform`, `openssh-client` (+ `sshpass` untuk tombol Salin key)
- Rocky: port `8080/tcp` dibuka otomatis bila `firewalld` aktif
- **Proxmox VE** + API Token: Datacenter → Access → API Tokens (`root@pam!panel`), uncheck *Privilege Separation* kalau mau full.

## VM template (Ubuntu / Rocky / custom)

Pilihan template di form New Cluster berasal dari `GET /api/templates`
(Ubuntu 22.04/24.04, Debian 12, **Rocky 8/9**) plus opsi **Custom** untuk
mengetik nama template apa pun yang ada di Proxmox. SSH user VM otomatis
menyesuaikan (rocky → `rocky`, debian → `admin`, ubuntu → `ubuntu`) tapi
tetap bisa diubah manual. Template yang sama bisa diganti belakangan via
kartu Connection di cluster-detail.

Catatan: role Ansible (`ansible/roles/*`) saat ini menarget keluarga Debian
(`apt`). Untuk Rocky Linux, role `common`/`master`/`worker` perlu adaptasi
`dnf` + repo Kubernetes EL — Terraform-nya sudah OS-agnostic (tinggal nama
template).

## SSH key (remote mode, sekali saja — dari panel, tanpa CLI)

Buka **Health & SSH** → kartu *SSH key*:

1. **Generate key** (sekali saja, idempoten — tidak menimpa key lama).
2. Isi host/user/port + **password sekali saja** → **Salin key ke server**
   (`ssh-copy-id` via `sshpass`, password tidak disimpan di mana pun).
3. **Test SSH** harus OK tanpa password.

Butuh `sshpass` di host panel (sudah dipasang otomatis oleh `install.sh`).
Kalau alamat Proxmox berubah: ubah SSH host/URL di kartu **Connection**
halaman cluster-detail → Save → Deploy ulang. Tidak perlu hapus cluster.

## API Endpoints

| Method | Endpoint | Deskripsi |
|--------|----------|-----------|
| GET | `/api/health` | Server time + status runtime |
| GET | `/api/tools` | Versi terraform/ansible/ssh (native + compat) |
| GET | `/api/realtime/summary` | Counter clusters (poll 5s, dashboard) |
| POST | `/api/health/proxmox-test` | Test Proxmox API Token `{proxmox_url, proxmox_user, token_id, token_secret, verify_tls}` |
| POST | `/api/ssh/test` | Test SSH `{ssh_host, ssh_user, ssh_port}` (+ diagnosa key saat gagal) |
| GET | `/api/ssh/key` | Public key host panel `{exists, public_key, via}` — setup sekali |
| POST | `/api/ssh/keygen` | Generate key ed25519 kalau belum ada (idempoten) |
| POST | `/api/ssh/copy-id` | `ssh-copy-id` dengan password sekali saja `{ssh_host, ssh_user, ssh_port, ssh_password}` — password tidak disimpan |
| GET/POST | `/api/clusters` | List / create (wajib `name` + `token_secret`) |
| PUT | `/api/clusters/:id` | Ubah koneksi/template (URL, SSH, template) tanpa hapus; secret kosong = tetap |
| GET | `/api/clusters/:id/status` | Status + progress + logs |
| POST | `/api/clusters/:id/deploy` | Deploy async (background task, guard concurrent → 409) |
| POST | `/api/clusters/:id/plan` | Preview `init + plan` tanpa apply (log fase plan, guard concurrent → 409) |
| POST | `/api/clusters/:id/preflight` | Cek kesiapan read-only: api, template, terraform/ssh, ip (tanpa ubah apa pun) |
| POST | `/api/clusters/:id/destroy` | Hapus semua VM (`terraform destroy`), definisi cluster tetap |
| POST | `/api/clusters/:id/refresh-ips` | Baca ulang `terraform output` tanpa deploy ulang |
| GET | `/api/clusters/:id/vms` | VM milik cluster (live Proxmox, filter prefix nama) |
| POST | `/api/clusters/:id/vms/:vmid/:action` | Power: `start`/`shutdown`/`reboot`/`stop` (cek prefix, tercatat di log) |
| DELETE | `/api/clusters/:id` | Hapus |
| GET | `/api/nodes` | Live dari Proxmox bila token valid, else mock |
| GET | `/api/templates` | Daftar template |
| GET | `/api/config` | Defaults |
| GET | `/api/ws/logs/:id` | WebSocket log realtime + heartbeat 5s |

## Flow Deployment

1. Isi form (token + target node + template) → **Test Proxmox** harus OK.
2. Pilih mode: kosongkan SSH host = **eksekusi lokal**; isi SSH host = **remote via `ssh`** (transport otomatis: native di Linux, WSL-bridge di Windows).
3. Create → Deploy → backend:
   - test Proxmox `/version` (live) — gagal = deploy **dibatalkan** dengan status `error` (tidak ada sukses palsu),
   - tulis `$PANEL_DATA/infra/terraform/<id>/{main.tf, terraform.tfvars (0600, secret asli), inventory.ini, deploy-remote.sh}`,
   - lokal: `terraform init` + `terraform apply -auto-approve` di host ini,
   - remote: `scp -r` direktori cluster ke `/tmp/proxpilot/<id>/` lalu `terraform init + apply` di server,
   - sukses apply → status `running` 100% + perintah ansible lanjutan di log.
4. Log mengalir realtime ke `cluster-detail.html` via WS; kalau WS putus, polling 3s backup.
5. Isi VM **tidak** diinstal otomatis — jalankan playbook sesuai **fitur cluster**
   (`k8s`, `nginx`, `nodejs`; Docker selalu ikut via playbook common).
   Perintah persisnya ada di log deploy + file `run-ansible.sh` di folder cluster:
   `$PANEL_DATA/infra/terraform/<id>/run-ansible.sh` (dari repo root).
   IP (`ansible_host`) sudah terisi otomatis: statis langsung, DHCP via `terraform output`.

## Struktur

```
panel/
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
├── infra/terraform/ # generated per-cluster, dev fallback (prod: $PANEL_DATA/infra, gitignored)
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
