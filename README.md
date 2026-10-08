# Proxmox Terraform + Ansible Automation
# =====================================
#
# Struktur:
#   terraform/        - Proxmox VM provisioning
#   ansible/          - Konfigurasi VM (K8s master, worker, nginx)
#   landing-page/     - Halaman web landing
#
# Cara Pakai:
#   1. cp terraform/terraform.tfvars.example terraform/terraform.tfvars
#      lalu isi dengan kredensial Proxmox Anda
#
#   2. Provision VM:
#      cd terraform
#      terraform init
#      terraform apply
#
#   3. Konfigurasi K8s Master:
#      ansible-playbook -i ../ansible/inventory.ini playbook-master.yml
#
#   4. Konfigurasi Worker:
#      ansible-playbook -i ../ansible/inventory.ini playbook-workers.yml
#
#   5. Konfigurasi Nginx Landing Page:
#      ansible-playbook -i ../ansible/inventory.ini playbook-nginx.yml
#
# VM yang dibuat:
#   - k8s-master (192.168.1.10) : 4 vCPU, 8GB RAM
#   - k8s-worker1 (192.168.1.11): 2 vCPU, 4GB RAM
#   - k8s-worker2 (192.168.1.12): 2 vCPU, 4GB RAM
