package web

import (
	"fmt"
	"net/http"
	"os"
	"path/filepath"
	"strings"

	"github.com/gin-gonic/gin"
	"proxpilot/internal/ansible"
	"proxpilot/internal/proxmox"
	"proxpilot/internal/runs"
)

// ---------- configure ----------

func (s *Server) apiConfigTemplates(c *gin.Context) {
	c.JSON(http.StatusOK, ansible.Templates)
}

func (s *Server) apiConfigure(c *gin.Context) {
	var b struct {
		VMIDs    []uint64 `json:"vmids"`
		Hosts    []struct {
			Name string `json:"name"`
			IP   string `json:"ip"`
		} `json:"hosts"`
		Template string `json:"template"`
		SSHUser  string `json:"ssh_user"`
	}
	if err := c.ShouldBindJSON(&b); err != nil {
		c.JSON(http.StatusBadRequest, gin.H{"error": "bad request"})
		return
	}
	if b.Template == "" {
		c.JSON(http.StatusBadRequest, gin.H{"error": "pilih template dulu"})
		return
	}
	playbook, ok := ansible.Playbooks[b.Template]
	if !ok {
		c.JSON(http.StatusBadRequest, gin.H{"error": "template tidak dikenal"})
		return
	}
	sshUser := strings.TrimSpace(b.SSHUser)
	if sshUser == "" {
		sshUser = "ubuntu"
	}
	// Two host sources: explicit cluster hosts (IP known, no agent needed)
	// or live VMIDs (IP via guest agent).
	var hosts []ansible.Host
	for _, h := range b.Hosts {
		if strings.TrimSpace(h.Name) == "" || strings.TrimSpace(h.IP) == "" {
			c.JSON(http.StatusBadRequest, gin.H{"error": "host cluster tanpa IP — Deploy static dulu"})
			return
		}
		hosts = append(hosts, ansible.Host{Name: h.Name, IP: h.IP, User: sshUser})
	}
	if len(hosts) == 0 {
		if len(b.VMIDs) == 0 {
			c.JSON(http.StatusBadRequest, gin.H{"error": "pilih VM/host dulu"})
			return
		}
		var err error
		hosts, err = s.hostsFromVMIDs(b.VMIDs, sshUser)
		if err != nil {
			c.JSON(http.StatusBadRequest, gin.H{"error": err.Error()})
			return
		}
	}
	r := s.runs.Start("configure "+b.Template, func(r *runs.Run) {
		s.flowConfigureHosts(r, hosts, b.Template, playbook)
	})
	c.JSON(http.StatusCreated, gin.H{"ok": true, "run_id": r.ID})
}

// hostsFromVMIDs resolves live VMs to ansible hosts via guest-agent IP.
func (s *Server) hostsFromVMIDs(vmids []uint64, sshUser string) ([]ansible.Host, error) {
	vms, _, _, err := s.fetchAll()
	if err != nil {
		return nil, err
	}
	creds, ok := s.creds()
	if !ok {
		return nil, fmt.Errorf("setup belum disimpan")
	}
	byID := map[uint64]proxmox.VM{}
	for _, v := range vms {
		if !v.Template {
			byID[v.VMID] = v
		}
	}
	var hosts []ansible.Host
	for _, id := range vmids {
		v, ok := byID[id]
		if !ok {
			return nil, fmt.Errorf("vmid %d tidak ada (atau itu template)", id)
		}
		node := v.Node
		if node == "" {
			node = s.setupNode()
		}
		ip := creds.AgentIP(node, id)
		if ip == "" {
			return nil, fmt.Errorf("vm %s (%d): IP belum terbaca (agent mati / baru boot)", v.Name, id)
		}
		hosts = append(hosts, ansible.Host{Name: v.Name, IP: ip, User: sshUser})
	}
	return hosts, nil
}

func (s *Server) flowConfigureHosts(r *runs.Run, hosts []ansible.Host, template, playbook string) {
	dir := filepath.Join(s.runsDir(), r.ID)
	_ = os.MkdirAll(dir, 0o755)
	inv, err := ansible.WriteInventory(dir, hosts)
	if err != nil {
		r.Fail(err.Error())
		return
	}
	r.Log("%d host -> %s (%s)", len(hosts), template, playbook)
	if err := ansible.StreamPlaybook(s.cfg.AnsibleDir, playbook, inv, r); err != nil {
		r.Fail(err.Error())
		return
	}
	r.Done("selesai: " + template)
}

// ---------- runs: status + SSE ----------

func (s *Server) apiRunStatus(c *gin.Context) {
	r, ok := s.runs.Get(c.Param("id"))
	if !ok {
		c.JSON(http.StatusNotFound, gin.H{"error": "run tidak ada"})
		return
	}
	status, msg, log := r.Snapshot()
	c.JSON(http.StatusOK, gin.H{"id": r.ID, "title": r.Title, "status": status, "message": msg, "log": log})
}

func (s *Server) apiRunEvents(c *gin.Context) {
	id := c.Param("id")
	if _, ok := s.runs.Get(id); !ok {
		c.JSON(http.StatusNotFound, gin.H{"error": "run tidak ada"})
		return
	}
	c.Header("Content-Type", "text/event-stream")
	c.Header("Cache-Control", "no-cache")
	c.Header("Connection", "keep-alive")
	c.Header("X-Accel-Buffering", "no")
	w := c.Writer
	flusher, _ := w.(http.Flusher)
	send := func(ev runs.Event) bool {
		_, _ = fmt.Fprintf(w, "event: %s\ndata: %s\n\n", ev.Type, sseEscape(ev.Data))
		if flusher != nil {
			flusher.Flush()
		}
		return c.Request.Context().Err() == nil
	}
	_, _ = fmt.Fprintf(w, ": connected %s\n\n", id)
	if flusher != nil {
		flusher.Flush()
	}
	s.runs.Stream(id, send)
}

func sseEscape(s string) string {
	s = strings.ReplaceAll(s, "\r", "")
	return strings.ReplaceAll(s, "\n", "\ndata: ")
}
