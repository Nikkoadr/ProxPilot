terraform {
  required_providers {
    proxmox = {
      source  = "bpg/proxmox"
      version = ">= 0.66.0"
    }
  }
}

variable "proxmox_endpoint" {
  description = "Base URL Proxmox (tanpa /api2/json)"
  type        = string
}

variable "proxmox_api_token" {
  description = "Format: user@realm!tokenid=secret"
  type        = string
  sensitive   = true
}

provider "proxmox" {
  endpoint  = var.proxmox_endpoint
  api_token = var.proxmox_api_token
  insecure  = true
}
