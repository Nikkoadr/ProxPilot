package web

import (
	"fmt"
	"net/http"
	"os"
	"path/filepath"
	"strings"

	"github.com/gin-gonic/gin"
	"proxpilot/internal/ansible"
	"proxpilot/internal/runs"
)

// ---------- configure ----------

func (s *Server) apiConfigTemplates(c *gin.Context) {
	c.JSON(http.StatusOK, ansible.Templates)
}

func (s *Server) apiConfigure(c *gin.Context) {
	var b struct {
		VMIDs    []uint64 `json:"vmids"`
		Template string   `json:"template"`
		SSHUser  string   `json:"ssh_user"`
	}
	if err := c.ShouldBindJSON(&b); err != nil || len(b.VMIDs) == 0 || b.Template == "" {
		c.JSON(http.StatusBadRequest, gin.H{"error": "pilih VM + template dulu"})
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
	r := s.runs.Start("configure "+b.Template, func(r *runs.Run) {
		s.flowConfigure(r, b.VMIDs, b.Template, playbook, sshUser)
	})
	c.JSON(http.StatusCreated, gin.H{"ok": true, "run_id": r.ID})
}

func (s *Server) flowConfigure(r *runs.Run, vmids []uint64, template, playbook, sshUser string) {
	vms, _, _, err := s.fetchAll()
	if err != nil {
		r.Fail("gagal list VM: " + err.Error())
		return
	}
	byID := map[uint64]string{}
	var hosts []ansible.Host
	for _, v := range vms {
		if v.Template {
			continue
		}
		byID[v.VMID] = v.Name
	}
	for _, id := range vmids {
		name, ok := byID[id]
		if !ok {
			r.Fail(fmt.Sprintf("vmid %d tidak ada (atau itu template)", id))
			return
		}
		ip, err := s.resolveHostIP(id, name)
		if err != nil {
			r.Fail(fmt.Sprintf("vm %s (%d): %v", name, id, err))
			return
		}
		hosts = append(hosts, ansible.Host{Name: name, IP: ip, User: sshUser})
	}
	dir := filepath.Join(s.runsDir(), r.ID)
	_ = os.MkdirAll(dir, 0o755)
	inv, err := ansible.WriteInventory(dir, hosts)
	if err != nil {
		r.Fail(err.Error())
		return
	}
	r.Log("%d VM -> %s (%s)", len(hosts), template, playbook)
	if err := ansible.StreamPlaybook(s.cfg.AnsibleDir, playbook, inv, r); err != nil {
		r.Fail(err.Error())
		return
	}
	r.Done("selesai: " + template)
}

// resolveHostIP gets VM IP via guest agent.
func (s *Server) resolveHostIP(vmid uint64, name string) (string, error) {
	creds, ok := s.creds()
	if !ok {
		return "", fmt.Errorf("setup belum disimpan")
	}
	vms, _, _, err := s.fetchAll()
	if err != nil {
		return "", err
	}
	for _, v := range vms {
		if v.VMID == vmid && !v.Template {
			node := v.Node
			if node == "" {
				node = s.setupNode()
			}
			if ip := creds.AgentIP(node, vmid); ip != "" {
				return ip, nil
			}
			return "", fmt.Errorf("IP belum terbaca (agent mati / VM baru boot) — start VM dan tunggu")
		}
	}
	return "", fmt.Errorf("tidak ketemu")
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
