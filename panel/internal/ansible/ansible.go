package ansible

import (
	"bufio"
	"fmt"
	"os"
	"os/exec"
	"path/filepath"

	"proxpilot/internal/runs"
)

// Host is one ansible target.
type Host struct {
	Name string
	IP   string
	User string
}

// Playbooks maps template id -> playbook file.
var Playbooks = map[string]string{
	"k8s-master": "playbook-master.yml",
	"k8s-worker": "playbook-workers.yml",
	"nginx":      "playbook-nginx.yml",
	"redis":      "playbook-redis.yml",
	"mariadb":    "playbook-mariadb.yml",
}

// Templates metadata for UI.
type Template struct {
	ID       string `json:"id"`
	Name     string `json:"name"`
	Playbook string `json:"playbook"`
	Note     string `json:"note"`
}

var Templates = []Template{
	{"k8s-master", "Kubernetes Master", "playbook-master.yml", "Init control-plane + Calico"},
	{"k8s-worker", "Kubernetes Worker", "playbook-workers.yml", "Join via /tmp/k8s-join.sh dari master"},
	{"nginx", "Nginx Test Page", "playbook-nginx.yml", "Landing page di :80"},
	{"redis", "Redis", "playbook-redis.yml", "Cache/session store"},
	{"mariadb", "MariaDB Tuned", "playbook-mariadb.yml", "Tuning otomatis ikut CPU/RAM"},
}

// WriteInventory creates inventory.ini mapping hosts to all groups.
func WriteInventory(dir string, hosts []Host) (string, error) {
	mkHost := func(h Host) string {
		return fmt.Sprintf("%s ansible_host=%s ansible_user=%s\n", h.Name, h.IP, h.User)
	}
	var b string
	b += "[targets]\n"
	for _, h := range hosts {
		b += mkHost(h)
	}
	for _, g := range []string{"k8s_master", "k8s_workers", "nginx_group", "configured"} {
		b += "\n[" + g + "]\n"
		for _, h := range hosts {
			b += mkHost(h)
		}
	}
	p := filepath.Join(dir, "inventory.ini")
	if err := os.WriteFile(p, []byte(b), 0o644); err != nil {
		return "", err
	}
	return p, nil
}

// StreamPlaybook runs ansible-playbook, piping each line to run log.
func StreamPlaybook(ansibleDir, playbook, inventory string, r *runs.Run) error {
	cmd := exec.Command("ansible-playbook", "-i", inventory, filepath.Join(ansibleDir, playbook))
	cmd.Env = append(os.Environ(), "ANSIBLE_HOST_KEY_CHECKING=False")
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
		r.Log("%s", sc.Text())
	}
	if err := cmd.Wait(); err != nil {
		return fmt.Errorf("ansible gagal: %v", err)
	}
	return nil
}
