package web

import (
	"fmt"
	"net/http"
	"strconv"
	"strings"
	"time"

	"github.com/gin-gonic/gin"
	"proxpilot/internal/proxmox"
	"proxpilot/internal/runs"
	"proxpilot/internal/sshutil"
)

func credsFrom(url, user, tokenID, secret string, verifyTLS bool) proxmox.Creds {
	if strings.TrimSpace(user) == "" {
		user = "root@pam"
	}
	return proxmox.Creds{
		BaseURL:     strings.TrimSpace(url),
		User:        user,
		TokenID:     strings.TrimSpace(strings.TrimPrefix(tokenID, "!")),
		TokenSecret: secret,
		VerifyTLS:   verifyTLS,
	}
}

// ---------- inventory ----------

func (s *Server) apiVMs(c *gin.Context) {
	vms, nodes, primary, err := s.fetchAll()
	if err != nil {
		c.JSON(http.StatusBadGateway, gin.H{"error": err.Error()})
		return
	}
	var templates []proxmox.VM
	for _, v := range vms {
		if v.Template {
			templates = append(templates, v)
		}
	}
	if templates == nil {
		templates = []proxmox.VM{}
	}
	c.JSON(http.StatusOK, gin.H{"ok": true, "node": primary, "nodes": nodes, "vms": vms, "templates": templates})
}

func (s *Server) apiTemplates(c *gin.Context) {
	vms, nodes, primary, err := s.fetchAll()
	if err != nil {
		c.JSON(http.StatusOK, gin.H{
			"templates": []any{}, "live": false, "node": s.setupNode(), "source": "fallback",
			"warning": "Proxmox belum terhubung: " + err.Error(),
		})
		return
	}
	var live []gin.H
	for _, v := range vms {
		if v.Template {
			node := v.Node
			if node == "" {
				node = primary
			}
			live = append(live, gin.H{
				"name": v.Name, "vmid": v.VMID,
				"description": fmt.Sprintf("live di node %s (vmid %d)", node, v.VMID),
			})
		}
	}
	if len(live) > 0 {
		c.JSON(http.StatusOK, gin.H{"templates": live, "live": true, "node": primary, "nodes": nodes, "source": "live"})
		return
	}
	var sample []string
	for i, v := range vms {
		if i >= 10 {
			break
		}
		sample = append(sample, fmt.Sprintf("%s (vmid %d)", v.Name, v.VMID))
	}
	warn := fmt.Sprintf("Terhubung ke %v tapi tidak ada template (cek %d VM). Buat template dulu di Proxmox (VM > More > Convert to template).", nodes, len(vms))
	c.JSON(http.StatusOK, gin.H{
		"templates": []any{}, "live": true, "node": primary, "nodes": nodes,
		"source": "live-empty", "total_vms": len(vms), "sample": sample, "warning": warn,
	})
}

// findVM locates a VM and its node.
func (s *Server) findVM(vmid uint64) (proxmox.VM, proxmox.Creds, bool) {
	creds, ok := s.creds()
	if !ok {
		return proxmox.VM{}, creds, false
	}
	vms, _, _, err := s.fetchAll()
	if err != nil {
		return proxmox.VM{}, creds, false
	}
	for _, v := range vms {
		if v.VMID == vmid {
			return v, creds, true
		}
	}
	return proxmox.VM{}, creds, false
}

// ---------- clone (async + SSE) ----------

type cloneReq struct {
	Template   string  `json:"template"`
	Name       string  `json:"name"`
	VMID       *uint64 `json:"vmid"`
	Full       bool    `json:"full"`
	Storage    string  `json:"storage"`
	StaticIP   string  `json:"static_ip"`
	Gateway    string  `json:"gateway"`
	CIUser     string  `json:"ciuser"`
	Nameserver string  `json:"nameserver"`
	Start      *bool   `json:"start"`
}

func (s *Server) apiClone(c *gin.Context) {
	var b cloneReq
	if err := c.ShouldBindJSON(&b); err != nil {
		c.JSON(http.StatusBadRequest, gin.H{"error": "bad request"})
		return
	}
	tplName := strings.TrimSpace(b.Template)
	vmName := strings.TrimSpace(b.Name)
	if tplName == "" {
		c.JSON(http.StatusBadRequest, gin.H{"error": "pilih template dulu"})
		return
	}
	if !validVMName(vmName) {
		c.JSON(http.StatusBadRequest, gin.H{"error": "nama VM invalid (huruf/angka/strip, maks 63)"})
		return
	}
	start := true
	if b.Start != nil {
		start = *b.Start
	}
	r := s.runs.Start("clone "+vmName, func(r *runs.Run) { s.flowClone(r, b, tplName, vmName, start) })
	c.JSON(http.StatusCreated, gin.H{"ok": true, "run_id": r.ID})
}

// flowClone executes clone -> cloud-init -> start -> IP, logging to run.
func (s *Server) flowClone(r *runs.Run, b cloneReq, tplName, vmName string, start bool) {
	creds, ok := s.creds()
	if !ok {
		r.Fail("setup belum disimpan")
		return
	}
	vms, nodes, primary, err := s.fetchAll()
	if err != nil {
		r.Fail("gagal list VM: " + err.Error())
		return
	}
	r.Log("terhubung, nodes: %s", strings.Join(nodes, ", "))
	// Resolve template in any node.
	var tpl *proxmox.VM
	for i := range vms {
		if vms[i].Template && vms[i].Name == tplName {
			tpl = &vms[i]
			break
		}
	}
	if tpl == nil {
		r.Fail(fmt.Sprintf("template '%s' tidak ditemukan (harus template, bukan VM biasa)", tplName))
		return
	}
	node := tpl.Node
	if node == "" {
		node = primary
	}
	r.Log("template %s (vmid %d) di node %s", tpl.Name, tpl.VMID, node)
	// VMID.
	var vmid uint64
	if b.VMID != nil {
		if *b.VMID < 100 || *b.VMID > 999999999 {
			r.Fail("vmid harus 100..999999999")
			return
		}
		for _, v := range vms {
			if v.VMID == *b.VMID {
				r.Fail(fmt.Sprintf("vmid %d sudah dipakai (%s)", *b.VMID, v.Name))
				return
			}
		}
		vmid = *b.VMID
	} else {
		n, err := creds.NextVMID()
		if err != nil {
			r.Fail("nextid gagal: " + err.Error())
			return
		}
		vmid = n
	}
	storage := strings.TrimSpace(b.Storage)
	if storage == "" {
		storage = "local-lvm"
	}
	r.Log("clone -> %s (%d), full=%v ...", vmName, vmid, b.Full)
	upid, err := creds.CloneVM(node, tpl.VMID, vmid, vmName, b.Full, storage)
	if err != nil {
		r.Fail("clone gagal: " + err.Error())
		return
	}
	r.Log("task %s, menunggu...", upid)
	exit, err := creds.WaitTask(node, upid)
	if err != nil {
		r.Fail("clone gagal: " + err.Error())
		return
	}
	if exit != "OK" {
		r.Fail("clone task exit: " + exit)
		return
	}
	r.Log("clone OK, config cloud-init...")
	ciuser := strings.TrimSpace(b.CIUser)
	if ciuser == "" {
		ciuser = "ubuntu"
	}
	pubkey, _ := sshutil.PublicKey()
	ipconfig := "ip=dhcp"
	if strings.TrimSpace(b.StaticIP) != "" {
		if !validIPv4(strings.TrimSpace(b.StaticIP)) {
			r.Fail(fmt.Sprintf("static_ip invalid — VM %s (%d) sudah ter-clone, perbaiki manual", vmName, vmid))
			return
		}
		gw := strings.TrimSpace(b.Gateway)
		if gw == "" {
			ip := strings.TrimSpace(b.StaticIP)
			if i := strings.LastIndex(ip, "."); i > 0 {
				gw = ip[:i] + ".1"
			}
		}
		ipconfig = fmt.Sprintf("ip=%s/24,gw=%s", strings.TrimSpace(b.StaticIP), gw)
	}
	ns := strings.TrimSpace(b.Nameserver)
	if ns == "" {
		ns = "8.8.8.8"
	}
	if err := creds.SetVMConfig(node, vmid, ciuser, pubkey, ipconfig, ns); err != nil {
		r.Fail(fmt.Sprintf("VM ter-clone TAPI config gagal: %v", err))
		return
	}
	if start {
		r.Log("start VM...")
		if _, err := creds.Power(node, vmid, "start"); err != nil {
			r.Fail(fmt.Sprintf("VM ter-clone TAPI start gagal: %v", err))
			return
		}
	}
	ip := ""
	for i := 0; i < 8; i++ {
		if found := creds.AgentIP(node, vmid); found != "" {
			ip = found
			break
		}
		time.Sleep(5 * time.Second)
	}
	if ip != "" {
		r.Log("IP: %s", ip)
		r.Done(fmt.Sprintf("VM %s (%d) jadi, IP %s", vmName, vmid, ip))
	} else {
		r.Log("IP belum terbaca (agent/DHCP lambat)")
		r.Done(fmt.Sprintf("VM %s (%d) jadi, IP belum terbaca", vmName, vmid))
	}
}

func (s *Server) apiVMPower(c *gin.Context) {
	vmid, err := strconv.ParseUint(c.Param("vmid"), 10, 64)
	if err != nil {
		c.JSON(http.StatusBadRequest, gin.H{"error": "vmid invalid"})
		return
	}
	action := c.Param("action")
	allowed := map[string]bool{"start": true, "shutdown": true, "reboot": true, "stop": true}
	if !allowed[action] {
		c.JSON(http.StatusBadRequest, gin.H{"error": "action harus start/shutdown/reboot/stop"})
		return
	}
	vm, creds, ok := s.findVM(vmid)
	if !ok {
		c.JSON(http.StatusNotFound, gin.H{"error": fmt.Sprintf("vmid %d tidak ada", vmid)})
		return
	}
	if vm.Template {
		c.JSON(http.StatusBadRequest, gin.H{"error": "itu TEMPLATE"})
		return
	}
	node := vm.Node
	if node == "" {
		node = s.setupNode()
	}
	upid, err := creds.Power(node, vmid, action)
	if err != nil {
		c.JSON(http.StatusBadGateway, gin.H{"error": err.Error()})
		return
	}
	c.JSON(http.StatusOK, gin.H{"ok": true, "task": upid})
}

func (s *Server) apiVMDelete(c *gin.Context) {
	vmid, err := strconv.ParseUint(c.Param("vmid"), 10, 64)
	if err != nil {
		c.JSON(http.StatusBadRequest, gin.H{"error": "vmid invalid"})
		return
	}
	vm, creds, ok := s.findVM(vmid)
	if !ok {
		c.JSON(http.StatusNotFound, gin.H{"error": fmt.Sprintf("vmid %d tidak ada", vmid)})
		return
	}
	if vm.Template {
		c.JSON(http.StatusBadRequest, gin.H{"error": "itu TEMPLATE — hapus manual di Proxmox"})
		return
	}
	if strings.ToLower(vm.Status) == "running" {
		c.JSON(http.StatusBadRequest, gin.H{"error": "VM running — shutdown/stop dulu"})
		return
	}
	node := vm.Node
	if node == "" {
		node = s.setupNode()
	}
	if err := creds.DeleteVM(node, vmid); err != nil {
		c.JSON(http.StatusBadGateway, gin.H{"error": err.Error()})
		return
	}
	c.JSON(http.StatusOK, gin.H{"ok": true})
}

func (s *Server) apiVMIP(c *gin.Context) {
	vmid, err := strconv.ParseUint(c.Param("vmid"), 10, 64)
	if err != nil {
		c.JSON(http.StatusBadRequest, gin.H{"error": "vmid invalid"})
		return
	}
	vm, creds, ok := s.findVM(vmid)
	if !ok {
		c.JSON(http.StatusNotFound, gin.H{"error": "tidak ada"})
		return
	}
	node := vm.Node
	if node == "" {
		node = s.setupNode()
	}
	ip := creds.AgentIP(node, vmid)
	c.JSON(http.StatusOK, gin.H{"ok": true, "vmid": vmid, "ip": ip})
}
