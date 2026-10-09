package web

import (
	"net/http"
	"strconv"
	"strings"
	"time"

	"github.com/gin-gonic/gin"
	"proxpilot/internal/auth"
	"proxpilot/internal/sshutil"
)

// ---------- public ----------

func (s *Server) apiHealth(c *gin.Context) {
	c.JSON(http.StatusOK, gin.H{"ok": true, "time": time.Now().UTC()})
}

func (s *Server) apiLogin(c *gin.Context) {
	var b struct {
		Username string `json:"username"`
		Password string `json:"password"`
	}
	if err := c.ShouldBindJSON(&b); err != nil {
		c.JSON(http.StatusBadRequest, gin.H{"error": "bad request"})
		return
	}
	h, ok := s.store.GetPassHash(b.Username)
	if !ok || !auth.Check(h, b.Password) {
		c.JSON(http.StatusUnauthorized, gin.H{"error": "username/password salah"})
		return
	}
	tok, err := s.store.CreateSession(b.Username, auth.SessionTTL)
	if err != nil {
		c.JSON(http.StatusInternalServerError, gin.H{"error": err.Error()})
		return
	}
	c.SetCookie(auth.CookieName, tok, int(auth.SessionTTL.Seconds()), "/", "", false, true)
	c.JSON(http.StatusOK, gin.H{"ok": true})
}

func (s *Server) apiLogout(c *gin.Context) {
	if tok, err := c.Cookie(auth.CookieName); err == nil {
		s.store.DeleteSession(tok)
	}
	c.SetCookie(auth.CookieName, "", -1, "/", "", false, true)
	c.JSON(http.StatusOK, gin.H{"ok": true})
}

// ---------- account ----------

func (s *Server) apiMe(c *gin.Context) {
	user, _ := s.currentUser(c)
	c.JSON(http.StatusOK, gin.H{"username": user, "default_creds": s.store.Get("default_creds") == "1"})
}

func (s *Server) apiPassword(c *gin.Context) {
	user, _ := s.currentUser(c)
	var b struct {
		Old string `json:"old_password"`
		New string `json:"new_password"`
	}
	if err := c.ShouldBindJSON(&b); err != nil {
		c.JSON(http.StatusBadRequest, gin.H{"error": "bad request"})
		return
	}
	h, _ := s.store.GetPassHash(user)
	if !auth.Check(h, b.Old) {
		c.JSON(http.StatusBadRequest, gin.H{"error": "password lama salah"})
		return
	}
	if len(b.New) < 6 {
		c.JSON(http.StatusBadRequest, gin.H{"error": "password baru min 6 karakter"})
		return
	}
	nh, err := auth.Hash(b.New)
	if err != nil {
		c.JSON(http.StatusInternalServerError, gin.H{"error": err.Error()})
		return
	}
	_ = s.store.SetPassHash(user, nh)
	_ = s.store.Set("default_creds", "0")
	s.store.DeleteUserSessions(user)
	c.JSON(http.StatusOK, gin.H{"ok": true, "message": "password diganti, silakan login ulang"})
}

// ---------- setup ----------

func (s *Server) apiSetupGet(c *gin.Context) {
	hasSecret := s.store.Get("setup_token_secret") != ""
	secret := ""
	if hasSecret {
		secret = "******"
	}
	port := s.store.Get("setup_ssh_port")
	if port == "" {
		port = "22"
	}
	c.JSON(http.StatusOK, gin.H{
		"saved":         s.store.Get("setup_saved") == "1",
		"complete":      s.setupComplete(),
		"proxmox_url":   s.store.Get("setup_proxmox_url"),
		"proxmox_user":  s.store.Get("setup_proxmox_user"),
		"token_id":      s.store.Get("setup_token_id"),
		"token_secret":  secret,
		"has_token":     hasSecret,
		"verify_tls":    s.store.Get("setup_verify_tls") == "1",
		"target_node":   s.setupNode(),
		"ssh_host":      s.store.Get("setup_ssh_host"),
		"ssh_user":      s.store.Get("setup_ssh_user"),
		"ssh_port":      port,
		"prox_ok":       s.store.Get("prox_ok") == "1",
		"ssh_ok":        s.store.Get("ssh_ok") == "1",
	})
}

func (s *Server) apiSetupPut(c *gin.Context) {
	var b struct {
		ProxmoxURL   string `json:"proxmox_url"`
		ProxmoxUser  string `json:"proxmox_user"`
		TokenID      string `json:"token_id"`
		TokenSecret  string `json:"token_secret"`
		VerifyTLS    bool   `json:"verify_tls"`
		TargetNode   string `json:"target_node"`
		SSHHost      string `json:"ssh_host"`
		SSHUser      string `json:"ssh_user"`
		SSHPort      int    `json:"ssh_port"`
	}
	if err := c.ShouldBindJSON(&b); err != nil {
		c.JSON(http.StatusBadRequest, gin.H{"error": "bad request"})
		return
	}
	if strings.TrimSpace(b.ProxmoxURL) == "" {
		c.JSON(http.StatusBadRequest, gin.H{"error": "proxmox_url wajib diisi"})
		return
	}
	secret := strings.TrimSpace(b.TokenSecret)
	if secret == "" || secret == "******" {
		secret = s.store.Get("setup_token_secret")
	}
	if secret == "" {
		c.JSON(http.StatusBadRequest, gin.H{"error": "token_secret wajib diisi"})
		return
	}
	user := strings.TrimSpace(b.ProxmoxUser)
	if user == "" {
		user = "root@pam"
	}
	node := strings.TrimSpace(b.TargetNode)
	if node == "" {
		node = "pve"
	}
	sshUser := strings.TrimSpace(b.SSHUser)
	if sshUser == "" {
		sshUser = "root"
	}
	port := b.SSHPort
	if port <= 0 {
		port = 22
	}
	_ = s.store.Set("setup_proxmox_url", strings.TrimSpace(b.ProxmoxURL))
	_ = s.store.Set("setup_proxmox_user", user)
	_ = s.store.Set("setup_token_id", strings.TrimSpace(strings.TrimPrefix(b.TokenID, "!")))
	_ = s.store.Set("setup_token_secret", secret)
	_ = s.store.Set("setup_verify_tls", boolStr(b.VerifyTLS))
	_ = s.store.Set("setup_target_node", node)
	_ = s.store.Set("setup_ssh_host", strings.TrimSpace(b.SSHHost))
	_ = s.store.Set("setup_ssh_user", sshUser)
	_ = s.store.Set("setup_ssh_port", strconv.Itoa(port))
	_ = s.store.Set("setup_saved", "1")
	c.JSON(http.StatusOK, gin.H{"ok": true, "message": "setup tersimpan", "complete": s.setupComplete()})
}

func boolStr(b bool) string {
	if b {
		return "1"
	}
	return "0"
}

func (s *Server) apiTestProxmox(c *gin.Context) {
	var b struct {
		ProxmoxURL   string `json:"proxmox_url"`
		ProxmoxUser  string `json:"proxmox_user"`
		TokenID      string `json:"token_id"`
		TokenSecret  string `json:"token_secret"`
		VerifyTLS    bool   `json:"verify_tls"`
	}
	_ = c.ShouldBindJSON(&b)
	secret := b.TokenSecret
	if secret == "" || secret == "******" {
		secret = s.store.Get("setup_token_secret")
	}
	creds := credsFrom(b.ProxmoxURL, b.ProxmoxUser, b.TokenID, secret, b.VerifyTLS)
	res := creds.TestConnection()
	if ok, _ := res["ok"].(bool); ok {
		_ = s.store.Set("prox_ok", "1")
	} else {
		_ = s.store.Set("prox_ok", "0")
	}
	c.JSON(http.StatusOK, res)
}

func (s *Server) apiTestSSH(c *gin.Context) {
	var b struct {
		SSHHost string `json:"ssh_host"`
		SSHUser string `json:"ssh_user"`
		SSHPort int    `json:"ssh_port"`
	}
	_ = c.ShouldBindJSON(&b)
	if b.SSHUser == "" {
		b.SSHUser = "root"
	}
	if b.SSHPort <= 0 {
		b.SSHPort = 22
	}
	ok, out := sshutil.Test(b.SSHHost, b.SSHUser, b.SSHPort)
	_ = s.store.Set("ssh_ok", boolStr(ok))
	c.JSON(http.StatusOK, gin.H{"ok": ok, "output": out})
}

func (s *Server) apiSSHKey(c *gin.Context) {
	pub, exists := sshutil.PublicKey()
	c.JSON(http.StatusOK, gin.H{"exists": exists, "public_key": pub})
}

func (s *Server) apiSSHKeygen(c *gin.Context) {
	msg, err := sshutil.Keygen()
	if err != nil {
		c.JSON(http.StatusBadRequest, gin.H{"error": err.Error()})
		return
	}
	c.JSON(http.StatusOK, gin.H{"ok": true, "message": msg})
}

func (s *Server) apiSSHCopy(c *gin.Context) {
	var b struct {
		SSHHost     string `json:"ssh_host"`
		SSHUser     string `json:"ssh_user"`
		SSHPort     int    `json:"ssh_port"`
		SSHPassword string `json:"ssh_password"`
	}
	_ = c.ShouldBindJSON(&b)
	if b.SSHUser == "" {
		b.SSHUser = "root"
	}
	if b.SSHPort <= 0 {
		b.SSHPort = 22
	}
	ok, out := sshutil.CopyID(b.SSHHost, b.SSHUser, b.SSHPort, b.SSHPassword)
	if ok {
		if ok2, _ := sshutil.Test(b.SSHHost, b.SSHUser, b.SSHPort); ok2 {
			_ = s.store.Set("ssh_ok", "1")
		}
	}
	c.JSON(http.StatusOK, gin.H{"ok": ok, "output": out})
}

func (s *Server) apiNodes(c *gin.Context) {
	creds, ok := s.creds()
	if !ok {
		c.JSON(http.StatusOK, []any{})
		return
	}
	nodes, err := creds.ListNodes()
	if err != nil || len(nodes) == 0 {
		c.JSON(http.StatusOK, []any{})
		return
	}
	c.JSON(http.StatusOK, nodes)
}
