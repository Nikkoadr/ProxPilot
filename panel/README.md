# ProxPilot Panel — Go + Gin + Tabler

Panel cloning VM Proxmox + configure Ansible. Backend **Go (Gin)**,
frontend **Tabler** (CDN), realtime **SSE**, storage **SQLite**, login
**admin + password (dapat diubah)**.

Alur terkunci: **Setup** (SSH + API wajib OK & tersimpan) → **Clone VM**
→ **Deploy** (Terraform: VM + firewall) → **Configure** (Ansible).
Clone/Deploy/Configure disembunyikan + diblokir sampai setup lengkap.

## Install (Linux/WSL)

```bash
curl -fsSL https://raw.githubusercontent.com/Nikkoadr/ProxPilot/master/panel/install.sh | bash
# buka http://localhost:8080 (admin/admin123 → ganti di Settings)
```

Butuh: Go 1.24+, ansible, sshpass.

## Dev

```bash
cd panel
./start.sh   # PORT=8080 PANEL_DATA=./data go run .
```

DB: `$PANEL_DATA/panel_go.db` (setup lama dari `panel.db` diimpor otomatis
sekali saat boot pertama).

## API

| Method | Endpoint | Deskripsi |
|--------|----------|-----------|
| POST | `/api/login` `/api/logout` | sesi cookie 7 hari |
| GET | `/api/me` | user + flag default password |
| POST | `/api/user/password` | ganti password (logout semua sesi) |
| GET/PUT | `/api/setup` | baca/simpan koneksi |
| POST | `/api/test/proxmox` `/api/test/ssh` | test (sekali pakai) |
| GET | `/api/ssh/key` | public key panel |
| POST | `/api/ssh/keygen` `/api/ssh/copy` | generate / ssh-copy-id |
| GET | `/api/nodes` `/api/vms` `/api/templates` | live Proxmox (gated) |
| POST | `/api/clone` | clone async → `{run_id}` (SSE) |
| POST | `/api/vms/:vmid/:action` | start/shutdown/reboot/stop |
| DELETE | `/api/vms/:vmid` | hapus (harus stopped) |
| GET | `/api/configure/templates` | k8s-master, k8s-worker, nginx, redis, mariadb |
| POST | `/api/configure` | ansible async → `{run_id}` (SSE). Host dari `vmids` (live, via agent) atau `hosts:[{name,ip}]` (cluster/static, tanpa agent) |
| GET/POST | `/api/clusters` | list / buat cluster Terraform |
| DELETE | `/api/clusters/:id` | hapus definisi (+ file terraform) |
| GET | `/api/clusters/:id/hosts` | hosts + IP hasil deploy (untuk Configure) |
| POST | `/api/clusters/:id/deploy` | `terraform init+apply` async → `{run_id}` (SSE) |
| POST | `/api/clusters/:id/destroy` | `terraform destroy` async (definisi tetap) |
| GET | `/api/runs/:id` | status + log |
| GET | `/api/runs/:id/events` | **SSE**: `log`/`done`/`error` |

## Configure templates (Ansible)

`ansible/playbook-{master,workers,nginx,redis,mariadb}.yml`.
MariaDB bawa tuning otomatis dari fakta host (buffer pool 50% RAM,
log 25% pool, max_connections ikut RAM, thread cache ikut vCPU).

## CD otomatis (opsional, ala Jenkins)

Push ke master + CI hijau → server update sendiri. Butuh sekali saja:

```bash
# di server, sebagai root:
useradd -m gh-runner
echo 'gh-runner ALL=(ALL) NOPASSWD: /usr/local/bin/panel-update, /usr/bin/systemctl restart proxpilot' > /etc/sudoers.d/proxpilot-cd
git config --global --add safe.directory /opt/proxpilot
chown -R gh-runner:gh-runner /opt/proxpilot
# ambil token runner di GitHub: repo → Settings → Actions → Runners → New self-hosted runner
su - gh-runner
mkdir actions-runner && cd actions-runner
curl -sSL -o runner.tar.gz <URL_DARI_GITHUB>
tar xzf runner.tar.gz && ./config.sh --url https://github.com/Nikkoadr/ProxPilot --token TOKEN
sudo ./svc.sh install && sudo ./svc.sh start
```

Tanpa runner, job `deploy` cuma nunggu (kuning) — 3 job tes tetap jalan normal.
