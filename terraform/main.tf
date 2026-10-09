# Terraform provider untuk Proxmox
terraform {
  required_providers {
    proxmox = {
      source  = "telmate/proxmox"
      version = ">= 2.9.0"
    }
  }
}

variable "proxmox_api_url" {
  description = "Proxmox API URL"
  type        = string
  default     = "https://192.168.1.100:8006/api2/json"
}

variable "proxmox_username" {
  description = "Proxmox username"
  type        = string
  default     = "root@pam"
}

variable "proxmox_password" {
  description = "Proxmox password"
  type        = string
  sensitive   = true
}

variable "vm_start_on_create" {
  description = "Start VMs after creation"
  type        = bool
  default     = true
}

variable "ssh_user" {
  description = "SSH user for ansible connection"
  type        = string
  default     = "ubuntu"
}

variable "ssh_public_key" {
  description = "SSH public key file injected into VMs via cloud-init"
  type        = string
  default     = "~/.ssh/id_ed25519.pub"
}

variable "clone_template" {
  description = "Cloud-init template name on Proxmox (must match panel default)"
  type        = string
  default     = "ubuntu-22-04-cloudinit"
}

# ============================================================
# MASTER NODE
# ============================================================
resource "proxmox_vm_qemu" "master" {
  name        = "k8s-master"
  target_node = "pve"
  desc        = "Kubernetes Master Node"

  # Resource
  cores      = 4
  sockets    = 1
  cpu        = "host"
  memory     = 8192
  agent      = 1

  # Cloud-init (Ubuntu)
  os_type    = "cloud-init"
  os_network_config = ""
  scsihw     = "virtio-scsi-single"

  # Cloud-init disk
  clone = var.clone_template

  # Network
  network {
    bridge = "vmbr0"
    firewall = false
    model = "virtio"
  }

  # IP address via cloud-init
  ipconfig0 = "ip=192.168.1.10/24,gw=192.168.1.1"

  # SSH key injection
  ssh_user     = var.ssh_user
  sshkeys      = file(pathexpand(var.ssh_public_key))

  # Start after create
  oncreate  = var.vm_start_on_create
  onboot    = true

  lifecycle {
    ignore_changes = [network]
  }
}

# ============================================================
# WORKER NODE 1
# ============================================================
resource "proxmox_vm_qemu" "worker1" {
  name        = "k8s-worker1"
  target_node = "pve"
  desc        = "Kubernetes Worker Node 1"

  cores      = 2
  sockets    = 1
  cpu        = "host"
  memory     = 4096
  agent      = 1

  os_type    = "cloud-init"
  os_network_config = ""
  scsihw     = "virtio-scsi-single"

  clone = var.clone_template

  network {
    bridge = "vmbr0"
    firewall = false
    model = "virtio"
  }

  ipconfig0 = "ip=192.168.1.11/24,gw=192.168.1.1"

  ssh_user     = var.ssh_user
  sshkeys      = file(pathexpand(var.ssh_public_key))

  oncreate  = var.vm_start_on_create
  onboot    = true

  lifecycle {
    ignore_changes = [network]
  }
}

# ============================================================
# WORKER NODE 2
# ============================================================
resource "proxmox_vm_qemu" "worker2" {
  name        = "k8s-worker2"
  target_node = "pve"
  desc        = "Kubernetes Worker Node 2"

  cores      = 2
  sockets    = 1
  cpu        = "host"
  memory     = 4096
  agent      = 1

  os_type    = "cloud-init"
  os_network_config = ""
  scsihw     = "virtio-scsi-single"

  clone = var.clone_template

  network {
    bridge = "vmbr0"
    firewall = false
    model = "virtio"
  }

  ipconfig0 = "ip=192.168.1.12/24,gw=192.168.1.1"

  ssh_user     = var.ssh_user
  sshkeys      = file(pathexpand(var.ssh_public_key))

  oncreate  = var.vm_start_on_create
  onboot    = true

  lifecycle {
    ignore_changes = [network]
  }
}

# ============================================================
# OUTPUTS
# ============================================================
output "master_ip" {
  value = proxmox_vm_qemu.master.default_ipv4_address
}

output "worker1_ip" {
  value = proxmox_vm_qemu.worker1.default_ipv4_address
}

output "worker2_ip" {
  value = proxmox_vm_qemu.worker2.default_ipv4_address
}
