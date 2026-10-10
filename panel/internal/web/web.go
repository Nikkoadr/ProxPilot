package web

import (
	"fmt"
	"html/template"
	"net/http"
	"os"
	"path/filepath"
	"strconv"
	"strings"

	"github.com/gin-gonic/gin"
	"proxpilot/internal/auth"
	"proxpilot/internal/config"
	"proxpilot/internal/proxmox"
	"proxpilot/internal/runs"
	"proxpilot/internal/store"
)

// Server wires config, store, runs and templates.
type Server struct {
	cfg   config.Config
	store *store.Store
	runs  *runs.Registry
	tmpl  *template.Template
}

func New(cfg config.Config, st *store.Store) *Server {
	t := template.Must(template.ParseGlob(filepath.Join(cfg.TmplDir, "*.html")))
	return &Server{cfg: cfg, store: st, runs: runs.NewRegistry(), tmpl: t}
}

func (s *Server) Router() *gin.Engine {
	gin.SetMode(gin.ReleaseMode)
	r := gin.New()
	r.Use(gin.Recovery())
	r.SetHTMLTemplate(s.tmpl)
	r.Static("/static", s.cfg.StaticDir)

	pub := r.Group("/")
	{
		pub.GET("/login", s.pageLogin)
		pub.POST("/api/login", s.apiLogin)
		pub.POST("/api/logout", s.apiLogout)
		pub.GET("/api/health", s.apiHealth)
	}

	prot := r.Group("/")
	prot.Use(auth.Middleware(s.store))
	{
		prot.GET("/", s.pageIndex)
		prot.GET("/setup", s.pageSetup)
		prot.GET("/settings", s.pageSettings)
		prot.GET("/api/me", s.apiMe)
		prot.POST("/api/user/password", s.apiPassword)

		prot.GET("/api/setup", s.apiSetupGet)
		prot.PUT("/api/setup", s.apiSetupPut)
		prot.POST("/api/test/proxmox", s.apiTestProxmox)
		prot.POST("/api/test/ssh", s.apiTestSSH)
		prot.GET("/api/ssh/key", s.apiSSHKey)
		prot.POST("/api/ssh/keygen", s.apiSSHKeygen)
		prot.POST("/api/ssh/copy", s.apiSSHCopy)
		prot.GET("/api/nodes", s.apiNodes)

		gated := prot.Group("/")
		gated.Use(s.requireSetup)
		{
			gated.GET("/clone", s.pageClone)
			gated.GET("/configure", s.pageConfigure)
			gated.GET("/deploy", s.pageDeploy)
			gated.GET("/api/vms", s.apiVMs)
			gated.GET("/api/templates", s.apiTemplates)
			gated.POST("/api/clone", s.apiClone)
			gated.POST("/api/vms/:vmid/:action", s.apiVMPower)
			gated.DELETE("/api/vms/:vmid", s.apiVMDelete)
			gated.GET("/api/vms/:vmid/ip", s.apiVMIP)
			gated.GET("/api/configure/templates", s.apiConfigTemplates)
			gated.POST("/api/configure", s.apiConfigure)
			gated.GET("/api/clusters", s.apiClusters)
			gated.POST("/api/clusters", s.apiClusterCreate)
			gated.DELETE("/api/clusters/:id", s.apiClusterDelete)
			gated.GET("/api/clusters/:id/hosts", s.apiClusterHosts)
			gated.POST("/api/clusters/:id/deploy", s.apiClusterDeploy)
			gated.POST("/api/clusters/:id/destroy", s.apiClusterDestroy)
		}
		prot.GET("/api/runs/:id", s.apiRunStatus)
		prot.GET("/api/runs/:id/events", s.apiRunEvents)
	}
	return r
}

// ---------- pages ----------

func (s *Server) pageLogin(c *gin.Context) {
	if _, err := c.Cookie(auth.CookieName); err == nil {
		if tok, _ := c.Cookie(auth.CookieName); tok != "" {
			if _, ok := s.store.CheckSession(tok); ok {
				c.Redirect(http.StatusFound, "/")
				return
			}
		}
	}
	c.HTML(http.StatusOK, "login.html", nil)
}

func (s *Server) pageIndex(c *gin.Context) {
	if !s.setupComplete() {
		c.Redirect(http.StatusFound, "/setup")
		return
	}
	c.Redirect(http.StatusFound, "/clone")
}

func (s *Server) render(c *gin.Context, name string, data gin.H) {
	if data == nil {
		data = gin.H{}
	}
	data["SetupComplete"] = s.setupComplete()
	data["User"], _ = s.currentUser(c)
	c.HTML(http.StatusOK, name, data)
}

func (s *Server) pageSetup(c *gin.Context)     { s.render(c, "setup.html", nil) }
func (s *Server) pageClone(c *gin.Context)     { s.render(c, "clone.html", nil) }
func (s *Server) pageConfigure(c *gin.Context) { s.render(c, "configure.html", nil) }
func (s *Server) pageSettings(c *gin.Context)  { s.render(c, "settings.html", nil) }

// ---------- helpers ----------

func (s *Server) currentUser(c *gin.Context) (string, bool) {
	tok, err := c.Cookie(auth.CookieName)
	if err != nil {
		return "", false
	}
	return s.store.CheckSession(tok)
}

func (s *Server) setupComplete() bool {
	return s.store.Get("setup_saved") == "1" &&
		s.store.Get("prox_ok") == "1" &&
		s.store.Get("ssh_ok") == "1"
}

// requireSetup hides Clone/Configure until SSH+API verified and saved.
func (s *Server) requireSetup(c *gin.Context) {
	if s.setupComplete() {
		c.Next()
		return
	}
	if strings.HasSuffix(c.Request.URL.Path, ".html") || c.Request.URL.Path == "/clone" || c.Request.URL.Path == "/configure" {
		c.Redirect(http.StatusFound, "/setup")
		c.Abort()
		return
	}
	c.AbortWithStatusJSON(http.StatusBadRequest, gin.H{"error": "setup belum lengkap — verifikasi SSH + API di Setup dulu"})
}

func (s *Server) creds() (proxmox.Creds, bool) {
	url := s.store.Get("setup_proxmox_url")
	secret := s.store.Get("setup_token_secret")
	if strings.TrimSpace(url) == "" || strings.TrimSpace(secret) == "" {
		return proxmox.Creds{}, false
	}
	user := s.store.Get("setup_proxmox_user")
	if user == "" {
		user = "root@pam"
	}
	return proxmox.Creds{
		BaseURL:     url,
		User:        user,
		TokenID:     s.store.Get("setup_token_id"),
		TokenSecret: secret,
		VerifyTLS:   s.store.Get("setup_verify_tls") == "1",
	}, true
}

func (s *Server) setupNode() string {
	if n := s.store.Get("setup_target_node"); n != "" {
		return n
	}
	return "pve"
}

// fetchAll merges cluster-wide VMs, falling back to per-node.
func (s *Server) fetchAll() ([]proxmox.VM, []string, string, error) {
	creds, ok := s.creds()
	if !ok {
		return nil, nil, "", fmt.Errorf("setup belum disimpan")
	}
	if vms, nodes, err := creds.ListVMsCluster(); err == nil {
		primary := s.setupNode()
		if len(nodes) > 0 && !contains(nodes, primary) {
			primary = nodes[0]
		}
		return vms, nodes, primary, nil
	}
	// Fallback per-node.
	tryNodes := []string{s.setupNode()}
	if live, err := creds.ListNodes(); err == nil {
		for _, n := range live {
			if !contains(tryNodes, n.Name) {
				tryNodes = append(tryNodes, n.Name)
			}
		}
	}
	var merged []proxmox.VM
	var okNodes []string
	for _, n := range tryNodes {
		if vms, err := creds.ListVMsNode(n); err == nil {
			okNodes = append(okNodes, n)
			for _, v := range vms {
				dup := false
				for _, m := range merged {
					if m.VMID == v.VMID {
						dup = true
						break
					}
				}
				if !dup {
					merged = append(merged, v)
				}
			}
		}
	}
	if len(okNodes) == 0 {
		return nil, nil, "", fmt.Errorf("gagal list VM di semua node")
	}
	return merged, okNodes, okNodes[0], nil
}

func contains(ss []string, v string) bool {
	for _, x := range ss {
		if x == v {
			return true
		}
	}
	return false
}

func validVMName(s string) bool {
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

func validIPv4(s string) bool {
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

func (s *Server) runsDir() string {
	d := filepath.Join(s.cfg.DataDir, "runs")
	_ = os.MkdirAll(d, 0o755)
	return d
}
