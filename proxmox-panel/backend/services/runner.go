package services

import (
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
)

// RunTerraformApply executes terraform in the cluster directory
func (ts *TerraformService) RunTerraformApply(clusterID string, variables string) error {
	dir := filepath.Join(ts.WorkDir, clusterID)
	os.MkdirAll(dir, 0755)

	cmd := exec.Command("terraform", "init", "-upgrade=true")
	cmd.Dir = dir
	cmd.Env = os.Environ()
	out, err := cmd.CombinedOutput()
	if err != nil {
		return fmt.Errorf("terraform init failed: %v\n%s", err, out)
	}

	// Write variables file
	varFile := filepath.Join(dir, "terraform.tfvars")
	os.WriteFile(varFile, []byte(variables), 0644)

	// Generate main.tf from template
	mainTF := generateMainTF(clusterID)
	os.WriteFile(filepath.Join(dir, "main.tf"), []byte(mainTF), 0644)

	cmd = exec.Command("terraform", "apply", "-auto-approve", "-var-file=terraform.tfvars")
	cmd.Dir = dir
	cmd.Env = os.Environ()
	out, err = cmd.CombinedOutput()
	if err != nil {
		return fmt.Errorf("terraform apply failed: %v\n%s", err, out)
	}

	fmt.Printf("Terraform apply completed for cluster %s\n", clusterID)
	return nil
}

// RunAnsiblePlaybook executes ansible playbook
func (as *AnsibleService) RunAnsiblePlaybook(clusterID string, playbook string, extraVars map[string]string) error {
	dir := filepath.Join(as.WorkDir, clusterID)
	os.MkdirAll(dir, 0755)

	args := []string{
		"-i", filepath.Join("..", "ansible", "inventory.ini"),
		fmt.Sprintf("-e=@%s", filepath.Join(dir, "extra-vars.yml")),
		playbook,
	}

	cmd := exec.Command("ansible-playbook", args...)
	cmd.Dir = filepath.Join("..", "ansible")
	out, err := cmd.CombinedOutput()
	if err != nil {
		return fmt.Errorf("ansible failed: %v\n%s", err, out)
	}

	fmt.Printf("Ansible playbook %s completed for cluster %s\n", playbook, clusterID)
	return nil
}

func generateMainTF(clusterID string) string {
	return `
variable "proxmox_api_url" { type = string }
variable "proxmox_username" { type = string }
variable "proxmox_password" { type = string }
variable "target_node" { type = string }
variable "clone_template" { type = string }
variable "network_bridge" { type = string }
variable "gateway" { type = string }
variable "dns1" { type = string }
variable "ssh_user" { type = string }
variable "ssh_public_key" { type = string }
variable "master_count" { type = number }
variable "worker_count" { type = number }
variable "master_cpu" { type = number }
variable "master_ram" { type = number }
variable "worker_cpu" { type = number }
variable "worker_ram" { type = number }

provider "proxmox" {
  pm_api_url   = var.proxmox_api_url
  pm_api_token = ""
  pm_user      = var.proxmox_username
  pm_password  = var.proxmox_password
  pm_tls_insecure = true
}

resource "proxmox_vm_qemu" "master" {
  count       = var.master_count
  name        = "k8s-master-${count.index}"
  target_node = var.target_node
  desc        = "Kubernetes Master Node ${count.index}"

  cores     = var.master_cpu
  sockets   = 1
  cpu       = "host"
  memory    = var.master_ram
  agent     = 1
  os_type   = "cloud-init"
  scsihw    = "virtio-scsi-single"
  clone     = var.clone_template

  network {
    bridge = var.network_bridge
    firewall = false
  }

  ipconfig0 = "ip=dhcp"
  ssh_user  = var.ssh_user
  sshkey    = var.ssh_public_key

  start     = true
  onboot    = true

  lifecycle {
    ignore_changes = [network]
  }
}

resource "proxmox_vm_qemu" "worker" {
  count       = var.worker_count
  name        = "k8s-worker-${count.index}"
  target_node = var.target_node
  desc        = "Kubernetes Worker Node ${count.index}"

  cores     = var.worker_cpu
  sockets   = 1
  cpu       = "host"
  memory    = var.worker_ram
  agent     = 1
  os_type   = "cloud-init"
  scsihw    = "virtio-scsi-single"
  clone     = var.clone_template

  network {
    bridge = var.network_bridge
    firewall = false
  }

  ipconfig0 = "ip=dhcp"
  ssh_user  = var.ssh_user
  sshkey    = var.ssh_public_key

  start     = true
  onboot    = true

  lifecycle {
    ignore_changes = [network]
  }
}
`
}
