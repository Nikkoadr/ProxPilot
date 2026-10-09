package main

import (
	"database/sql"
	"fmt"
	"os"
	"path/filepath"

	"proxpilot/internal/auth"
	"proxpilot/internal/config"
	"proxpilot/internal/store"
	"proxpilot/internal/web"
)

func main() {
	cfg := config.Load()
	st, err := store.Open(cfg.DataDir)
	if err != nil {
		fmt.Fprintln(os.Stderr, "open db:", err)
		os.Exit(1)
	}
	importLegacySetup(st, cfg.DataDir)
	if err := auth.EnsureSeed(st, os.Getenv("ADMIN_USER"), os.Getenv("ADMIN_PASS")); err != nil {
		fmt.Fprintln(os.Stderr, "seed:", err)
		os.Exit(1)
	}
	srv := web.New(cfg, st)
	fmt.Printf("proxpilot (Go+Gin) on http://localhost:%d\n", cfg.Port)
	fmt.Println("login default: admin / admin123 (ganti di Settings)")
	if err := srv.Router().Run(fmt.Sprintf("0.0.0.0:%d", cfg.Port)); err != nil {
		fmt.Fprintln(os.Stderr, err)
		os.Exit(1)
	}
}

// importLegacySetup copies setup_* keys from the old Rust panel.db once.
func importLegacySetup(st *store.Store, dataDir string) {
	if st.Get("setup_saved") == "1" {
		return
	}
	oldPath := filepath.Join(dataDir, "panel.db")
	if _, err := os.Stat(oldPath); err != nil {
		return
	}
	db, err := sql.Open("sqlite", oldPath)
	if err != nil {
		return
	}
	defer db.Close()
	rows, err := db.Query(`SELECT key, value FROM settings WHERE key LIKE 'setup_%' OR key IN ('prox_ok','ssh_ok')`)
	if err != nil {
		return
	}
	defer rows.Close()
	n := 0
	for rows.Next() {
		var k, v string
		if err := rows.Scan(&k, &v); err != nil {
			continue
		}
		if k == "setup_token_secret" && (v == "" || v == "******") {
			continue
		}
		_ = st.Set(k, v)
		n++
	}
	if n > 0 {
		fmt.Printf("imported %d setup keys from legacy panel.db\n", n)
	}
	// DB lama tak punya flag setup_saved — anggap tersimpan bila url+secret ada.
	// Test SSH/API tetap harus diulang sekali di UI (flag prox_ok/ssh_ok).
	if st.Get("setup_proxmox_url") != "" && st.Get("setup_token_secret") != "" &&
		st.Get("setup_saved") != "1" {
		_ = st.Set("setup_saved", "1")
		fmt.Println("legacy setup marked saved (re-test SSH/API once in UI)")
	}
}
