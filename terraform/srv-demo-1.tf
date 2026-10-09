# VM demo pertama — provider bpg/proxmox (support Proxmox 9).
# Duplikat file ini jadi srv-demo-2.tf dst untuk tambah VM.
resource "proxmox_virtual_environment_vm" "srv_demo_1" {
  name      = var.vm_name
  node_name = var.target_node

  clone {
    vm_id = var.template_vmid
    full  = true
  }

  # Import tidak membawa blok clone (create-time only) sehingga plan selalu
  # mau replace — abaikan agar VM adopsi tidak dibuat ulang.
  # Konsekuensi: ganti template_vmid tidak recreate otomatis (hapus state manual).
  lifecycle {
    ignore_changes = [clone]
  }

  # Template belum ada qemu-guest-agent: jangan enabled=true,
  # kalau tidak terraform menunggu agent selamanya saat create.
  # Setelah install qemu-guest-agent di VM/template, boleh ubah ke true.
  agent {
    enabled = false
  }

  cpu {
    cores = var.vm_cores
    type  = "host"
  }

  # Samakan dengan template (UEFI): jangan paksa seabios / hapus efi_disk,
  # VM bisa gagal boot.
  bios          = "ovmf"
  scsi_hardware = "virtio-scsi-single"

  efi_disk {
    datastore_id      = var.disk_storage
    file_format       = "raw"
    type              = "4m"
    pre_enrolled_keys = true
  }

  memory {
    dedicated = var.vm_memory
  }

  disk {
    datastore_id = var.disk_storage
    interface    = "scsi0"
    size         = var.disk_size_gb
  }

  network_device {
    bridge = var.network_bridge
  }

  operating_system {
    type = "l26"
  }

  initialization {
    datastore_id = var.disk_storage

    ip_config {
      ipv4 {
        address = "dhcp"
      }
    }

    dns {
      servers = [var.nameserver]
    }

    user_account {
      username = var.ciuser
      keys     = [trimspace(file(pathexpand(var.ssh_public_key)))]
    }
  }
}

output "srv_demo_1_id" {
  value = proxmox_virtual_environment_vm.srv_demo_1.vm_id
}
