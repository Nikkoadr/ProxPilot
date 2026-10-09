package sshutil

import (
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"time"

	"golang.org/x/crypto/ssh"
)

// KeyPaths returns private/public key paths (ed25519 preferred).
func KeyPaths() (priv, pub string) {
	home, _ := os.UserHomeDir()
	priv = filepath.Join(home, ".ssh", "id_ed25519")
	pub = priv + ".pub"
	if _, err := os.Stat(pub); err != nil {
		priv = filepath.Join(home, ".ssh", "id_rsa")
		pub = priv + ".pub"
	}
	return priv, pub
}

// PublicKey reads the panel host public key.
func PublicKey() (string, bool) {
	_, pub := KeyPaths()
	b, err := os.ReadFile(pub)
	if err != nil {
		return "", false
	}
	return strings.TrimSpace(string(b)), true
}

// Keygen creates ed25519 key if missing (idempotent).
func Keygen() (string, error) {
	priv, pub := KeyPaths()
	if _, err := os.Stat(pub); err == nil {
		return "key sudah ada", nil
	}
	if err := os.MkdirAll(filepath.Dir(priv), 0o700); err != nil {
		return "", err
	}
	// Always ed25519 for new keys.
	priv = filepath.Join(filepath.Dir(priv), "id_ed25519")
	cmd := exec.Command("ssh-keygen", "-t", "ed25519", "-N", "", "-f", priv)
	out, err := cmd.CombinedOutput()
	if err != nil {
		return "", fmt.Errorf("%v: %s", err, out)
	}
	return "key ed25519 dibuat", nil
}

// baseSSH builds common args.
func baseArgs(host, user string, port int, batch bool) []string {
	target := fmt.Sprintf("%s@%s", user, host)
	args := []string{
		"-p", fmt.Sprint(port),
		"-o", "StrictHostKeyChecking=no",
		"-o", "ConnectTimeout=8",
	}
	if batch {
		args = append(args, "-o", "BatchMode=yes")
	}
	return append(args, target)
}

// Test tries BatchMode ssh (no password).
func Test(host, user string, port int) (bool, string) {
	args := append(baseArgs(host, user, port, true), "echo OK")
	out, err := exec.Command("ssh", args...).CombinedOutput()
	if err != nil {
		return false, diagnose(string(out))
	}
	return true, strings.TrimSpace(string(out))
}

// CopyID appends the panel public key to the server's authorized_keys
// via password login. Pure Go — no sshpass/ssh-copy-id needed (Windows-safe).
// Password is used once and never stored.
func CopyID(host, user string, port int, password string) (bool, string) {
	if password == "" {
		return false, "password kosong"
	}
	pub, ok := PublicKey()
	if !ok {
		return false, "public key panel belum ada — klik Generate key dulu"
	}
	ok2, out := CopyIDNative(host, user, port, password, pub)
	if ok2 {
		if ok3, _ := Test(host, user, port); ok3 {
			return true, "key tersalin + test SSH OK"
		}
		return true, "key tersalin (test SSH masih gagal — cek manual)"
	}
	return false, out
}

// CopyIDNative logs in with password and installs the key.
func CopyIDNative(host, user string, port int, password, pubkey string) (bool, string) {
	cfg := &ssh.ClientConfig{
		User:            user,
		Auth:            []ssh.AuthMethod{ssh.Password(password)},
		HostKeyCallback: ssh.InsecureIgnoreHostKey(),
		Timeout:         12 * time.Second,
	}
	conn, err := ssh.Dial("tcp", fmt.Sprintf("%s:%d", host, port), cfg)
	if err != nil {
		return false, "login password gagal: " + shortErr(err)
	}
	defer conn.Close()
	sess, err := conn.NewSession()
	if err != nil {
		return false, shortErr(err)
	}
	defer sess.Close()
	sess.Stdin = strings.NewReader(pubkey + "\n")
	cmd := `mkdir -p ~/.ssh && chmod 700 ~/.ssh && cat >> ~/.ssh/authorized_keys` +
		` && chmod 600 ~/.ssh/authorized_keys && sort -u ~/.ssh/authorized_keys -o ~/.ssh/authorized_keys && echo COPIED`
	out, err := sess.CombinedOutput(cmd)
	if err != nil {
		return false, strings.TrimSpace(string(out)) + " " + shortErr(err)
	}
	if !strings.Contains(string(out), "COPIED") {
		return false, strings.TrimSpace(string(out))
	}
	return true, "key tersalin"
}

func shortErr(err error) string {
	s := err.Error()
	if i := strings.Index(s, ": "); i >= 0 {
		if tail := s[i+2:]; len(tail) < len(s) {
			return tail
		}
	}
	if len(s) > 200 {
		return s[:200]
	}
	return s
}

func diagnose(out string) string {
	lo := strings.ToLower(out)
	switch {
	case strings.Contains(lo, "permission denied"):
		return "Permission denied — key belum disalin. Klik Salin Key dulu. | " + out
	case strings.Contains(lo, "timed out") || strings.Contains(lo, "connection refused") || strings.Contains(lo, "no route"):
		return "Jaringan gagal — IP/port salah atau firewall. | " + out
	default:
		return out
	}
}
