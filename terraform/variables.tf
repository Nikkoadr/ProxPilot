variable "target_node" {
  description = "Nama node Proxmox asli (lihat Datacenter > Nodes, mis. pve001)"
  type        = string
  default     = "pve001"
}

variable "template_vmid" {
  description = "VMID template cloud-init QEMU (mis. 9001 template-ubuntu-22)"
  type        = number
  default     = 9001
}

variable "vm_name" {
  type    = string
  default = "srv-demo-1"
}

variable "vm_cores" {
  type    = number
  default = 2
}

variable "vm_memory" {
  type    = number
  default = 4096
}

variable "disk_storage" {
  type    = string
  default = "local-lvm"
}

variable "disk_size_gb" {
  description = "Ukuran disk GB, harus >= disk template (template-ubuntu-22 = 2.2G)"
  type        = number
  default     = 32
}

variable "network_bridge" {
  type    = string
  default = "vmbr0"
}

variable "ciuser" {
  type    = string
  default = "ubuntu"
}

variable "nameserver" {
  type    = string
  default = "8.8.8.8"
}

variable "ssh_public_key" {
  description = "Path public key yang diinject via cloud-init"
  type        = string
  default     = "~/.ssh/id_ed25519.pub"
}
