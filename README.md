# ProxPilot — Proxmox Terraform + Ansible Automation

Provisioning Kubernetes cluster di Proxmox VE: Terraform untuk VM, Ansible untuk
K8s + Nginx, dan **panel web (Rust/Axum)** sebagai orkestrator.

```
ProxPilot/
├── panel/       # Web panel (Rust + SB Admin 2, SQLite, WebSocket logs)
├── terraform/   # Provisioning VM manual (tanpa panel)
├── ansible/     # Konfigurasi VM: common, master, worker, nginx
```

## Cara 1 — via Panel (disarankan)

```bash
curl -fsSL https://raw.githubusercontent.com/Nikkoadr/ProxPilot/master/panel/install.sh | bash
# buka http://localhost:8080  (login default admin / admin123 → ganti di Settings)
```

Alur: isi form cluster (API Token Proxmox) → Test Proxmox → Create → Deploy.
Panel menjalankan `terraform init + apply` (lokal atau via `ssh` ke server
Proxmox), lalu menampilkan perintah ansible lanjutan di log. Detail:
[`panel/README.md`](panel/README.md).

## Cara 2 — manual (tanpa panel)

```bash
cp terraform/terraform.tfvars.example terraform/terraform.tfvars
# isi kredensial Proxmox, lalu:
cd terraform && terraform init && terraform apply

ansible-playbook -i ../ansible/inventory.ini ../ansible/playbook-common.yml
ansible-playbook -i ../ansible/inventory.ini ../ansible/playbook-master.yml
ansible-playbook -i ../ansible/inventory.ini ../ansible/playbook-workers.yml
ansible-playbook -i ../ansible/inventory.ini ../ansible/playbook-nginx.yml
```

## VM default

- `k8s-master` — 4 vCPU, 8 GB RAM
- `k8s-worker1`, `k8s-worker2` — 2 vCPU, 4 GB RAM
- Template cloud-init: `ubuntu-22-04-cloudinit` (lihat `terraform/terraform.tfvars.example`)
- Jaringan DHCP via `vmbr0` (IP statis legacy `192.168.1.10/11/12` hanya di `ansible/inventory.ini` contoh)

## Dev panel (edit Windows, run WSL)

```bash
cd /mnt/d/laragon/www/ProxPilot/panel   # sesuaikan path
./start.sh                              # PORT=8080 PANEL_DATA=./data cargo run
```
