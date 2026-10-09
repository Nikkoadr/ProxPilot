package store

import (
	"crypto/rand"
	"database/sql"
	"encoding/hex"
	"os"
	"path/filepath"
	"time"

	_ "modernc.org/sqlite"
)

// Store wraps SQLite: users, sessions, settings.
type Store struct {
	db *sql.DB
}

// Open creates dirs, opens DB, migrates schema.
func Open(dataDir string) (*Store, error) {
	if err := os.MkdirAll(dataDir, 0o755); err != nil {
		return nil, err
	}
	db, err := sql.Open("sqlite", filepath.Join(dataDir, "panel_go.db"))
	if err != nil {
		return nil, err
	}
	s := &Store{db: db}
	if err := s.migrate(); err != nil {
		return nil, err
	}
	return s, nil
}

func (s *Store) migrate() error {
	stmts := []string{
		`CREATE TABLE IF NOT EXISTS users (username TEXT PRIMARY KEY, pass_hash TEXT NOT NULL)`,
		`CREATE TABLE IF NOT EXISTS sessions (token TEXT PRIMARY KEY, username TEXT NOT NULL, expires_at INTEGER NOT NULL)`,
		`CREATE TABLE IF NOT EXISTS settings (key TEXT PRIMARY KEY, value TEXT NOT NULL DEFAULT '')`,
		`CREATE INDEX IF NOT EXISTS idx_sessions_exp ON sessions(expires_at)`,
	}
	for _, q := range stmts {
		if _, err := s.db.Exec(q); err != nil {
			return err
		}
	}
	return nil
}

// --- settings ---

func (s *Store) Get(key string) string {
	var v string
	_ = s.db.QueryRow(`SELECT value FROM settings WHERE key=?`, key).Scan(&v)
	return v
}

func (s *Store) Set(key, value string) error {
	_, err := s.db.Exec(`INSERT INTO settings(key,value) VALUES(?,?)
		ON CONFLICT(key) DO UPDATE SET value=excluded.value`, key, value)
	return err
}

// --- users ---

func (s *Store) GetPassHash(username string) (string, bool) {
	var h string
	err := s.db.QueryRow(`SELECT pass_hash FROM users WHERE username=?`, username).Scan(&h)
	return h, err == nil
}

func (s *Store) SetPassHash(username, hash string) error {
	_, err := s.db.Exec(`INSERT INTO users(username,pass_hash) VALUES(?,?)
		ON CONFLICT(username) DO UPDATE SET pass_hash=excluded.pass_hash`, username, hash)
	return err
}

func (s *Store) UserCount() int {
	var n int
	_ = s.db.QueryRow(`SELECT COUNT(*) FROM users`).Scan(&n)
	return n
}

// --- sessions ---

func NewToken() string {
	b := make([]byte, 32)
	_, _ = rand.Read(b)
	return hex.EncodeToString(b)
}

func (s *Store) CreateSession(username string, ttl time.Duration) (string, error) {
	tok := NewToken()
	exp := time.Now().Add(ttl).Unix()
	_, err := s.db.Exec(`INSERT INTO sessions(token,username,expires_at) VALUES(?,?,?)`, tok, username, exp)
	return tok, err
}

func (s *Store) CheckSession(tok string) (string, bool) {
	var user string
	var exp int64
	err := s.db.QueryRow(`SELECT username,expires_at FROM sessions WHERE token=?`, tok).Scan(&user, &exp)
	if err != nil || time.Now().Unix() > exp {
		return "", false
	}
	return user, true
}

func (s *Store) DeleteSession(tok string) {
	_, _ = s.db.Exec(`DELETE FROM sessions WHERE token=?`, tok)
}

func (s *Store) DeleteUserSessions(username string) {
	_, _ = s.db.Exec(`DELETE FROM sessions WHERE username=?`, username)
}

func (s *Store) CleanupSessions() {
	_, _ = s.db.Exec(`DELETE FROM sessions WHERE expires_at < ?`, time.Now().Unix())
}
