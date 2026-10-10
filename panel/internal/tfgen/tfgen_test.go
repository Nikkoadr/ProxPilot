package tfgen

import (
	"os"
	"path/filepath"
	"strings"
	"testing"

	"proxpilot/internal/store"
)

func TestStaticIPs(t *testing.T) {
	ips, err := StaticIPs("192.168.1.50", 3)
	if err != nil {
		t.Fatal(err)
	}
	want := []string{"192.168.1.50", "192.168.1.51", "192.168.1.52"}
	for i := range want {
		if ips[i] != want[i] {
			t.Fatalf("got %v want %v", ips, want)
		}
	}
	if _, err := StaticIPs("192.168.1.254", 2); err == nil {
		t.Fatal("expected overflow error")
	}
	if _, err := StaticIPs("bogus", 1); err == nil {
		t.Fatal("expected invalid error")
	}
}

func TestWriteCluster(t *testing.T) {
	dir := t.TempDir()
	cl := store.Cluster{ID: "c1", Name: "web-1", Node: "pve001", TemplateVMID: 9001,
		VMSpecs: []store.VMSpec{
			{Name: "web-a", CPU: 2, RAM: 4096, DiskGB: 32, Bridge: "vmbr0", IPMode: "static", IP: "192.168.1.50"},
			{Name: "web-b", CPU: 4, RAM: 8192, DiskGB: 64, Bridge: "vmbr1", IPMode: "dhcp"},
		},
		IPMode: "static", BaseIP: "192.168.1.50"}
	err := Write(dir, Inputs{Cluster: cl, Endpoint: "https://x:8006", APIToken: "tok", SSHKey: "ssh-ed25519 AAAA", DNS: ""})
	if err != nil {
		t.Fatal(err)
	}
	b, _ := os.ReadFile(filepath.Join(dir, "main.tf"))
	s := string(b)
	for _, want := range []string{
		`source  = "bpg/proxmox"`,
		`resource "proxmox_virtual_environment_vm" "vm_0"`,
		`resource "proxmox_virtual_environment_vm" "vm_1"`,
		`"web-a"`, `"web-b"`,
		`address = "192.168.1.50/24"`, `address = "dhcp"`,
		`cores = 4`, `dedicated = 8192`,
		`boot_order`, `output "vms"`,
	} {
		if !strings.Contains(s, want) {
			t.Fatalf("main.tf missing %q", want)
		}
	}
	for _, nope := range []string{"firewall", "master_0", "worker_0"} {
		if strings.Contains(s, nope) {
			t.Fatalf("main.tf should not contain %q", nope)
		}
	}
	tfvars, _ := os.ReadFile(filepath.Join(dir, "terraform.tfvars"))
	if !strings.Contains(string(tfvars), "api_token") {
		t.Fatal("tfvars missing api_token")
	}
}
