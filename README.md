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

Alur panel: **Health** (SSH key + test koneksi) → **New Cluster** (VM/Terraform)
→ **Deploy** → Refresh IPs → **Configure** (isi VM/Ansible) → `run-ansible.sh`.
Kelola: **Plan** (preview tanpa apply), **Destroy** (hapus VM, definisi tetap),
VM live + power (start/reboot/shutdown/stop), guard anti deploy-ganda.

## Cara 2 — manual (tanpa panel, pola Thomas-Krenn, provider bpg)

> Catatan: provider lama `telmate/proxmox` menuntut hak `VM.Monitor` yang
> tidak ada lagi di Proxmox 9, jadi repo ini pakai `bpg/proxmox`.

```bash
cd terraform
cp credentials.auto.tfvars.example credentials.auto.tfvars
# isi endpoint + api_token (format user@realm!tokenid=secret),
# sesuaikan variables.tf (target_node, template_vmid) bila perlu, lalu:
terraform init
terraform plan     # cek: 1 VM srv-demo-1, tanpa error template/node
terraform apply    # ketik yes

ansible-playbook -i ../ansible/inventory.ini ../ansible/playbook-common.yml
ansible-playbook -i ../ansible/inventory.ini ../ansible/playbook-master.yml
ansible-playbook -i ../ansible/inventory.ini ../ansible/playbook-workers.yml
ansible-playbook -i ../ansible/inventory.ini ../ansible/playbook-nginx.yml
```

Struktur `terraform/`: `provider.tf` (token auth) + `variables.tf`
(default `target_node = pve001`, `template_vmid = 9001`) + `srv-demo-1.tf`
(1 file per VM, duplikat untuk tambah VM). Secret di
`credentials.auto.tfvars` (gitignored).

## VM default (bisa diubah semua dari panel)

- 1 master (4 vCPU, 8 GB RAM) + 2 worker (2 vCPU, 4 GB RAM) — jumlah/CPU/RAM bebas, `0` worker = 1 VM
- Nama VM: prefix custom atau otomatis (`prefix-id-master-0`) — unik per projek
- Template cloud-init: `ubuntu-22-04-cloudinit` / `rocky-9-cloudinit` / custom
- IP: DHCP via `vmbr0`, atau statik dari base IP (master dulu, worker lanjut)
- Disk: ikut template, atau resize scsi0 (GB) di storage pilihan; VLAN tag opsional
- User + SSH key panel diinject via cloud-init (`ciuser`/`sshkeys`); DNS dari field DNS

## Dev panel (edit Windows, run WSL)

```bash
cd /mnt/d/laragon/www/ProxPilot/panel   # sesuaikan path
./start.sh                              # PORT=8080 PANEL_DATA=./data cargo run
```
