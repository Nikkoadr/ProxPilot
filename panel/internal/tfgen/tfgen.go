package tfgen

import (
	"bufio"
	"encoding/json"
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"strconv"
	"strings"
	"text/template"

	"proxpilot/internal/runs"
	"proxpilot/internal/store"
)

// Inputs for cluster file generation.
type Inputs struct {
	Cluster  store.Cluster
	Endpoint string // https://host:8006 (no /api2/json)
	APIToken string // user@realm!tokenid=secret
	SSHKey   string // panel public key (may be empty)
	DNS      string
}

// StaticIPs assigns consecutive IPs from base for n VMs.
func StaticIPs(base string, n int) ([]string, error) {
	parts := strings.Split(base, ".")
	if len(parts) != 4 {
		return nil, fmt.Errorf("base IP invalid")
	}
	octs := make([]int, 4)
	for i, p := range parts {
		n, err := strconv.Atoi(p)
		if err != nil || n < 0 || n > 255 {
			return nil, fmt.Errorf("base IP invalid")
		}
		octs[i] = n
	}
	var out []string
	for i := 0; i < n; i++ {
		last := octs[3] + i
		if last > 254 {
			return nil, fmt.Errorf("blok IP lewat .254")
		}
		out = append(out, fmt.Sprintf("%d.%d.%d.%d", octs[0], octs[1], octs[2], last))
	}
	return out, nil
}

func gatewayOf(ip string) string {
	if i := strings.LastIndex(ip, "."); i > 0 {
		return ip[:i] + ".1"
	}
	return ""
}

// validIP checks dotted-quad IPv4.
func validIP(s string) bool {
	parts := strings.Split(s, ".")
	if len(parts) != 4 {
		return false
	}
	for _, p := range parts {
		n, err := strconv.Atoi(p)
		if err != nil || n < 0 || n > 255 || p == "" {
			return false
		}
	}
	return true
}

// validTFName mirrors Proxmox VM naming (also used for terraform resource safety).
func validTFName(s string) bool {
	if len(s) == 0 || len(s) > 63 || s[0] == '-' || s[len(s)-1] == '-' {
		return false
	}
	for i := 0; i < len(s); i++ {
		ch := s[i]
		if !(ch >= 'a' && ch <= 'z' || ch >= 'A' && ch <= 'Z' || ch >= '0' && ch <= '9' || ch == '-') {
			return false
		}
	}
	return true
}

const mainTmpl = `terraform {
  required_providers {
    proxmox = {
      source  = "bpg/proxmox"
      version = ">= 0.66.0"
    }
  }
}

variable "endpoint" {
  type = string
}
variable "api_token" {
  type      = string
  sensitive = true
}

provider "proxmox" {
  endpoint  = var.endpoint
  api_token = var.api_token
  insecure  = true
}

{{range .VMs}}
resource "proxmox_virtual_environment_vm" "{{.Res}}" {
  name      = "{{.Name}}"
  node_name = "{{$.Node}}"
  tags      = ["proxpilot", "{{$.Name}}"]

  clone {
    vm_id = {{$.TemplateVMID}}
    full  = true
  }
  lifecycle { ignore_changes = [clone] }

  agent {
    enabled = false
  }

  # Template boot order (ide2;net0) tidak ada disk → tanpa ini VM nyangkut PXE.
  boot_order = ["scsi0"]

  cpu {
    cores = {{.CPU}}
    type  = "host"
  }
  memory {
    dedicated = {{.RAM}}
  }

  disk {
    datastore_id = "local-lvm"
    interface    = "scsi0"
    size         = {{.DiskGB}}
  }

  network_device {
    bridge = "{{.Bridge}}"
  }
  operating_system {
    type = "l26"
  }

  initialization {
    datastore_id = "local-lvm"
    ip_config {
      ipv4 {
        address = "{{.Addr}}"
        gateway = "{{.GW}}"
      }
    }
    dns {
      servers = ["{{$.DNS}}"]
    }
    user_account {
      username = "ubuntu"
      keys     = [{{$.SSHKeyHCL}}]
    }
  }
}
{{end}}

output "vms" {
  value = [
{{range .VMs}}    { name = proxmox_virtual_environment_vm.{{.Res}}.name, vmid = proxmox_virtual_environment_vm.{{.Res}}.vm_id },
{{end}}  ]
}
`

type vmTmpl struct {
	Res    string // unique resource name (vm-0)
	Name   string // vm name
	CPU    int
	RAM    int
	DiskGB int
	Bridge string
	Addr   string // "dhcp" or "192.168.1.50/24"
	GW     string
}

// Write generates main.tf + terraform.tfvars in dir (one block per VM).
// Deploy is intentionally standard: cloud-init + SSH only, no firewall
// (firewall & apps belong to Ansible).
func Write(dir string, in Inputs) error {
	if err := os.MkdirAll(dir, 0o755); err != nil {
		return err
	}
	c := in.Cluster
	names := c.VMList()
	if len(names) == 0 {
		return fmt.Errorf("minimal 1 nama VM")
	}
	byName := map[string]store.VMSpec{}
	for _, sp := range c.VMSpecs {
		byName[sp.Name] = sp
	}
	sshHCL := `"PLACEHOLDER_KEY"`
	if k := strings.TrimSpace(in.SSHKey); k != "" {
		sshHCL = strconv.Quote(k)
	}
	dns := in.DNS
	if dns == "" {
		dns = "8.8.8.8"
	}
	var ips []string
	if c.IPMode == "static" && len(c.VMSpecs) == 0 {
		var err error
		ips, err = StaticIPs(c.BaseIP, len(names))
		if err != nil {
			return err
		}
	}
	var vms []vmTmpl
	for i, name := range names {
		if !validTFName(name) {
			return fmt.Errorf("nama VM invalid: %s", name)
		}
		sp, hasSpec := byName[name]
		cpu, ram, disk, bridge := c.CPU, c.RAM, c.DiskGB, c.Bridge
		addr, gw := "dhcp", ""
		if hasSpec {
			if sp.CPU >= 1 {
				cpu = sp.CPU
			}
			if sp.RAM >= 512 {
				ram = sp.RAM
			}
			if sp.DiskGB >= 4 {
				disk = sp.DiskGB
			}
			if sp.Bridge != "" {
				bridge = sp.Bridge
			}
			if sp.IPMode == "static" {
				if !validIP(sp.IP) {
					return fmt.Errorf("IP static invalid untuk %s: %s", name, sp.IP)
				}
				addr, gw = sp.IP+"/24", gatewayOf(sp.IP)
			}
		} else if ips != nil {
			addr, gw = ips[i]+"/24", gatewayOf(ips[i])
		}
		if bridge == "" {
			bridge = "vmbr0"
		}
		vms = append(vms, vmTmpl{Res: fmt.Sprintf("vm_%d", i), Name: name,
			CPU: cpu, RAM: ram, DiskGB: disk, Bridge: bridge, Addr: addr, GW: gw})
	}
	data := map[string]any{
		"Name":         c.Name,
		"Node":         c.Node,
		"TemplateVMID": c.TemplateVMID,
		"DNS":          dns,
		"SSHKeyHCL":    sshHCL,
		"VMs":          vms,
	}
	t, err := template.New("main").Parse(mainTmpl)
	if err != nil {
		return err
	}
	var sb strings.Builder
	if err := t.Execute(&sb, data); err != nil {
		return err
	}
	if err := os.WriteFile(filepath.Join(dir, "main.tf"), []byte(sb.String()), 0o644); err != nil {
		return err
	}
	tfvars := fmt.Sprintf("endpoint  = %q\napi_token = %q\n", in.Endpoint, in.APIToken)
	return os.WriteFile(filepath.Join(dir, "terraform.tfvars"), []byte(tfvars), 0o600)
}

// OutputVM is one entry of `terraform output -json vms`.
type OutputVM struct {
	Name string `json:"name"`
	VMID int    `json:"vmid"`
}

// ParseOutputs decodes `terraform output -json vms`.
func ParseOutputs(raw []byte) ([]OutputVM, error) {
	var v struct {
		Value []OutputVM `json:"value"`
	}
	// output -json vms returns the value directly when naming one output.
	if err := json.Unmarshal(raw, &v.Value); err == nil {
		return v.Value, nil
	}
	if err := json.Unmarshal(raw, &v); err != nil {
		return nil, err
	}
	return v.Value, nil
}

// terraformBin resolves the terraform binary (PATH, then ~/.local/bin).
func terraformBin() (string, error) {
	if p, err := exec.LookPath("terraform"); err == nil {
		return p, nil
	}
	home, _ := os.UserHomeDir()
	p := filepath.Join(home, ".local", "bin", "terraform")
	if st, err := os.Stat(p); err == nil && !st.IsDir() {
		return p, nil
	}
	return "", fmt.Errorf(`terraform tidak ketemu — install via "setup.bat deps"`)
}

// Run streams a terraform command into run log.
func Run(dir string, run *runs.Run, args ...string) error {
	bin, err := terraformBin()
	if err != nil {
		return err
	}
	cmd := exec.Command(bin, args...)
	cmd.Dir = dir
	cmd.Env = append(os.Environ(), "TF_IN_AUTOMATION=1")
	stdout, err := cmd.StdoutPipe()
	if err != nil {
		return err
	}
	cmd.Stderr = cmd.Stdout
	if err := cmd.Start(); err != nil {
		return err
	}
	sc := bufio.NewScanner(stdout)
	sc.Buffer(make([]byte, 64*1024), 1024*1024)
	for sc.Scan() {
		run.Log("%s", sc.Text())
	}
	return cmd.Wait()
}

// OutputJSON runs `terraform output -json <name>`.
func OutputJSON(dir, name string) ([]byte, error) {
	bin, err := terraformBin()
	if err != nil {
		return nil, err
	}
	cmd := exec.Command(bin, "output", "-json", name)
	cmd.Dir = dir
	return cmd.Output()
}
