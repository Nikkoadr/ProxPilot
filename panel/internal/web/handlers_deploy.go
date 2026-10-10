package web

import (
	"fmt"
	"net/http"
	"os"
	"path/filepath"
	"strings"
	"time"

	"github.com/gin-gonic/gin"
	"proxpilot/internal/runs"
	"proxpilot/internal/sshutil"
	"proxpilot/internal/store"
	"proxpilot/internal/tfgen"
)

func (s *Server) pageDeploy(c *gin.Context) { s.render(c, "deploy.html", nil) }

func (s *Server) tfDir(id string) string {
	return filepath.Join(s.cfg.DataDir, "terraform", id)
}

// ---------- clusters ----------

func (s *Server) apiClusters(c *gin.Context) {
	list, err := s.store.ListClusters()
	if err != nil {
		c.JSON(http.StatusInternalServerError, gin.H{"error": err.Error()})
		return
	}
	if list == nil {
		list = []store.Cluster{}
	}
	c.JSON(http.StatusOK, list)
}

func (s *Server) apiClusterCreate(c *gin.Context) {
	var b struct {
		Name         string           `json:"name"`
		Node         string           `json:"node"`
		TemplateVMID int              `json:"template_vmid"`
		TemplateName string           `json:"template_name"`
		VMs          []store.VMSpec   `json:"vms"`
		VMNames      []string         `json:"vm_names"`
		CPU          int              `json:"cpu"`
		RAM          int              `json:"ram"`
		DiskGB       int              `json:"disk_gb"`
		Bridge       string           `json:"bridge"`
		IPMode       string           `json:"ip_mode"`
		BaseIP       string           `json:"base_ip"`
	}
	if err := c.ShouldBindJSON(&b); err != nil {
		c.JSON(http.StatusBadRequest, gin.H{"error": "bad request"})
		return
	}
	name := strings.TrimSpace(b.Name)
	if !validVMName(name) {
		c.JSON(http.StatusBadRequest, gin.H{"error": "nama cluster invalid (huruf/angka/strip, maks 63)"})
		return
	}
	if b.TemplateVMID < 100 {
		c.JSON(http.StatusBadRequest, gin.H{"error": "pilih template dulu"})
		return
	}
	var names []string
	var specs []store.VMSpec
	for _, vm := range b.VMs {
		n := strings.TrimSpace(vm.Name)
		if n == "" {
			continue
		}
		if !validVMName(n) {
			c.JSON(http.StatusBadRequest, gin.H{"error": "nama VM invalid: " + n})
			return
		}
		if vm.IPMode == "static" && !validIPv4(strings.TrimSpace(vm.IP)) {
			c.JSON(http.StatusBadRequest, gin.H{"error": "IP static invalid untuk " + n})
			return
		}
		specs = append(specs, store.VMSpec{
			Name: n, CPU: numOr(vm.CPU, 2), RAM: numOr(vm.RAM, 4096),
			DiskGB: numOr(vm.DiskGB, 32),
			Bridge: orStr(vm.Bridge, "vmbr0"),
			IPMode: orStatic(vm.IPMode), IP: strings.TrimSpace(vm.IP),
		})
		names = append(names, n)
	}
	// Fallback format lama (vm_names saja).
	if len(specs) == 0 {
		for _, n := range b.VMNames {
			if t := strings.TrimSpace(n); t != "" {
				names = append(names, t)
			}
		}
	}
	if len(names) == 0 {
		c.JSON(http.StatusBadRequest, gin.H{"error": "isi minimal 1 nama VM"})
		return
	}
	if len(names) > 20 {
		c.JSON(http.StatusBadRequest, gin.H{"error": "maks 20 VM per cluster"})
		return
	}
	seen := map[string]bool{}
	for _, n := range names {
		if !validVMName(n) {
			c.JSON(http.StatusBadRequest, gin.H{"error": "nama VM invalid: " + n})
			return
		}
		if seen[n] {
			c.JSON(http.StatusBadRequest, gin.H{"error": "nama VM dobel: " + n})
			return
		}
		seen[n] = true
	}
	if b.CPU < 1 {
		b.CPU = 2
	}
	if b.RAM < 512 {
		b.RAM = 4096
	}
	if b.DiskGB < 0 {
		b.DiskGB = 0
	}
	if b.DiskGB == 0 {
		b.DiskGB = 32
	}
	if b.Bridge == "" {
		b.Bridge = "vmbr0"
	}
	if b.IPMode != "static" {
		b.IPMode = "dhcp"
	}
	if b.IPMode == "static" && len(specs) == 0 {
		if _, err := tfgen.StaticIPs(b.BaseIP, len(names)); err != nil {
			c.JSON(http.StatusBadRequest, gin.H{"error": "static base IP invalid: " + err.Error()})
			return
		}
	}
	node := strings.TrimSpace(b.Node)
	if node == "" {
		node = s.setupNode()
	}
	cl := store.Cluster{
		ID: store.NewID(), Name: name, Node: node,
		TemplateVMID: b.TemplateVMID, TemplateName: b.TemplateName,
		VMNames: names, VMSpecs: specs, CPU: b.CPU, RAM: b.RAM,
		DiskGB: b.DiskGB, Bridge: b.Bridge, IPMode: b.IPMode, BaseIP: b.BaseIP,
		Status: "pending", CreatedAt: time.Now().Unix(),
	}
	if err := s.store.CreateCluster(cl); err != nil {
		c.JSON(http.StatusInternalServerError, gin.H{"error": err.Error()})
		return
	}
	if err := s.writeClusterFiles(cl); err != nil {
		c.JSON(http.StatusInternalServerError, gin.H{"error": "tulis file gagal: " + err.Error()})
		return
	}
	c.JSON(http.StatusCreated, cl)
}

func (s *Server) writeClusterFiles(cl store.Cluster) error {
	creds, ok := s.creds()
	if !ok {
		return fmt.Errorf("setup belum disimpan")
	}
	endpoint := strings.TrimSuffix(strings.TrimSuffix(creds.BaseURL, "/"), "/api2/json")
	token := fmt.Sprintf("%s!%s=%s", creds.User, creds.TokenID, creds.TokenSecret)
	pub, _ := sshutil.PublicKey()
	return tfgen.Write(s.tfDir(cl.ID), tfgen.Inputs{
		Cluster: cl, Endpoint: endpoint, APIToken: token,
		SSHKey: pub, DNS: "8.8.8.8",
	})
}

func (s *Server) apiClusterDelete(c *gin.Context) {
	id := c.Param("id")
	if _, ok := s.store.GetCluster(id); !ok {
		c.JSON(http.StatusNotFound, gin.H{"error": "cluster tidak ada"})
		return
	}
	os.RemoveAll(s.tfDir(id))
	s.store.DeleteCluster(id)
	c.JSON(http.StatusOK, gin.H{"ok": true})
}

func (s *Server) apiClusterHosts(c *gin.Context) {
	hosts, err := s.store.ClusterHosts(c.Param("id"))
	if err != nil {
		c.JSON(http.StatusInternalServerError, gin.H{"error": err.Error()})
		return
	}
	if hosts == nil {
		hosts = []store.Host{}
	}
	c.JSON(http.StatusOK, hosts)
}

// ---------- deploy / destroy (async SSE) ----------

func (s *Server) apiClusterDeploy(c *gin.Context) {
	id := c.Param("id")
	cl, ok := s.store.GetCluster(id)
	if !ok {
		c.JSON(http.StatusNotFound, gin.H{"error": "cluster tidak ada"})
		return
	}
	if err := s.writeClusterFiles(cl); err != nil {
		c.JSON(http.StatusBadRequest, gin.H{"error": err.Error()})
		return
	}
	r := s.runs.Start("deploy "+cl.Name, func(r *runs.Run) { s.flowDeploy(r, cl) })
	c.JSON(http.StatusCreated, gin.H{"ok": true, "run_id": r.ID})
}

func (s *Server) flowDeploy(r *runs.Run, cl store.Cluster) {
	dir := s.tfDir(cl.ID)
	s.store.SetClusterStatus(cl.ID, "deploying")
	r.Log("terraform init...")
	if err := tfgen.Run(dir, r, "init", "-input=false"); err != nil {
		s.store.SetClusterStatus(cl.ID, "error")
		r.Fail("init gagal: " + err.Error())
		return
	}
	r.Log("terraform apply...")
	if err := tfgen.Run(dir, r, "apply", "-auto-approve", "-input=false"); err != nil {
		// Clone full paralel kadang kena 596 timeout sesaat di Proxmox;
		// apply ulang hanya melengkapi yang kurang (idempoten).
		r.Log("apply gagal sekali, coba lagi (resume)...")
		if err2 := tfgen.Run(dir, r, "apply", "-auto-approve", "-input=false"); err2 != nil {
			s.store.SetClusterStatus(cl.ID, "error")
			r.Fail("apply gagal: " + err2.Error())
			return
		}
	}
	if err := s.refreshOutputs(cl, r); err != nil {
		s.store.SetClusterStatus(cl.ID, "error")
		r.Fail("baca output gagal: " + err.Error())
		return
	}
	s.store.SetClusterStatus(cl.ID, "running")
	r.Done(fmt.Sprintf("cluster %s running", cl.Name))
}

// refreshOutputs reads `terraform output -json vms` and stores hosts.
func (s *Server) refreshOutputs(cl store.Cluster, r *runs.Run) error {
	raw, err := tfgen.OutputJSON(s.tfDir(cl.ID), "vms")
	if err != nil {
		return err
	}
	outs, err := tfgen.ParseOutputs(raw)
	if err != nil {
		return err
	}
	var staticIPs []string
	if len(cl.VMSpecs) > 0 {
		for _, sp := range cl.VMSpecs {
			if sp.IPMode == "static" {
				staticIPs = append(staticIPs, sp.IP)
			} else {
				staticIPs = append(staticIPs, "")
			}
		}
	} else if cl.IPMode == "static" {
		staticIPs, _ = tfgen.StaticIPs(cl.BaseIP, len(cl.VMList()))
	}
	var hosts []store.Host
	for i, o := range outs {
		ip := ""
		if i < len(staticIPs) {
			ip = staticIPs[i]
		}
		hosts = append(hosts, store.Host{ClusterID: cl.ID, VMName: o.Name, VMID: o.VMID, IP: ip})
		if r != nil {
			r.Log("  %s (vmid %d) ip=%s", o.Name, o.VMID, ipOrDash(ip))
		}
	}
	return s.store.SaveHosts(cl.ID, hosts)
}

func ipOrDash(ip string) string {
	if ip == "" {
		return "-"
	}
	return ip
}

func (s *Server) apiClusterDestroy(c *gin.Context) {
	id := c.Param("id")
	cl, ok := s.store.GetCluster(id)
	if !ok {
		c.JSON(http.StatusNotFound, gin.H{"error": "cluster tidak ada"})
		return
	}
	r := s.runs.Start("destroy "+cl.Name, func(r *runs.Run) {
		s.store.SetClusterStatus(cl.ID, "deploying")
		r.Log("terraform destroy...")
		if err := tfgen.Run(s.tfDir(cl.ID), r, "destroy", "-auto-approve", "-input=false"); err != nil {
			s.store.SetClusterStatus(cl.ID, "error")
			r.Fail("destroy gagal: " + err.Error())
			return
		}
		_ = s.store.SaveHosts(cl.ID, nil)
		s.store.SetClusterStatus(cl.ID, "pending")
		r.Done(fmt.Sprintf("cluster %s dihancurkan (definisi tetap)", cl.Name))
	})
	c.JSON(http.StatusCreated, gin.H{"ok": true, "run_id": r.ID})
}
