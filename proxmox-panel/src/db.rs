//! SQLite persistence (clusters, logs, users, sessions, settings).
//!
//! DB file: `$PANEL_DATA/panel.db` (default `./data/panel.db`).
//! All methods are blocking — call from `spawn_blocking`.

use crate::models::{Cluster, LogEntry};
use rusqlite::{params, Connection};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

pub const MAX_LOGS_PER_CLUSTER: i64 = 1000;

#[derive(Clone)]
pub struct Db {
    inner: Arc<Mutex<Connection>>,
}

impl Db {
    pub fn open() -> Result<Self, String> {
        let dir = std::env::var("PANEL_DATA").unwrap_or_else(|_| "data".to_string());
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let path: PathBuf = PathBuf::from(&dir).join("panel.db");
        let conn = Connection::open(&path).map_err(|e| e.to_string())?;
        conn.execute_batch(
            "PRAGMA journal_mode=WAL;
             PRAGMA busy_timeout=5000;
             CREATE TABLE IF NOT EXISTS users (
               username TEXT PRIMARY KEY,
               pass_hash TEXT NOT NULL,
               created_at TEXT NOT NULL DEFAULT (datetime('now'))
             );
             CREATE TABLE IF NOT EXISTS sessions (
               token TEXT PRIMARY KEY,
               username TEXT NOT NULL,
               expires_at INTEGER NOT NULL
             );
             CREATE INDEX IF NOT EXISTS idx_sessions_user ON sessions(username);
             CREATE TABLE IF NOT EXISTS clusters (
               id TEXT PRIMARY KEY,
               name TEXT NOT NULL,
               status TEXT NOT NULL DEFAULT 'pending',
               progress INTEGER NOT NULL DEFAULT 0,
               data TEXT NOT NULL,
               created_at TEXT NOT NULL DEFAULT (datetime('now')),
               updated_at TEXT NOT NULL DEFAULT (datetime('now'))
             );
             CREATE TABLE IF NOT EXISTS logs (
               rowid INTEGER PRIMARY KEY AUTOINCREMENT,
               id TEXT NOT NULL,
               cluster_id TEXT NOT NULL,
               phase TEXT NOT NULL,
               line TEXT NOT NULL,
               level TEXT NOT NULL,
               timestamp TEXT NOT NULL
             );
             CREATE INDEX IF NOT EXISTS idx_logs_cluster ON logs(cluster_id, rowid);
             CREATE TABLE IF NOT EXISTS settings (
               key TEXT PRIMARY KEY,
               value TEXT NOT NULL
             );",
        )
        .map_err(|e| e.to_string())?;
        tracing::info!("sqlite db: {}", path.display());
        Ok(Self {
            inner: Arc::new(Mutex::new(conn)),
        })
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Connection> {
        self.inner.lock().expect("db mutex")
    }

    // ---------- settings ----------

    pub fn get_setting(&self, key: &str) -> Option<String> {
        let c = self.lock();
        c.query_row(
            "SELECT value FROM settings WHERE key=?1",
            params![key],
            |r| r.get(0),
        )
        .ok()
    }

    pub fn set_setting(&self, key: &str, value: &str) {
        let c = self.lock();
        let _ = c.execute(
            "INSERT INTO settings(key,value) VALUES(?1,?2)
             ON CONFLICT(key) DO UPDATE SET value=excluded.value",
            params![key, value],
        );
    }

    // ---------- users ----------

    pub fn user_count(&self) -> i64 {
        let c = self.lock();
        c.query_row("SELECT COUNT(*) FROM users", [], |r| r.get(0))
            .unwrap_or(0)
    }

    pub fn get_pass_hash(&self, username: &str) -> Option<String> {
        let c = self.lock();
        c.query_row(
            "SELECT pass_hash FROM users WHERE username=?1",
            params![username],
            |r| r.get(0),
        )
        .ok()
    }

    pub fn upsert_user(&self, username: &str, pass_hash: &str) {
        let c = self.lock();
        let _ = c.execute(
            "INSERT INTO users(username,pass_hash) VALUES(?1,?2)
             ON CONFLICT(username) DO UPDATE SET pass_hash=excluded.pass_hash",
            params![username, pass_hash],
        );
    }

    // ---------- sessions ----------

    pub fn create_session(&self, token: &str, username: &str, expires_unix: i64) {
        let c = self.lock();
        let _ = c.execute(
            "INSERT INTO sessions(token,username,expires_at) VALUES(?1,?2,?3)",
            params![token, username, expires_unix],
        );
    }

    /// Returns username if session valid (and purges expired ones lazily).
    pub fn check_session(&self, token: &str) -> Option<String> {
        let now = chrono::Utc::now().timestamp();
        let c = self.lock();
        let _ = c.execute("DELETE FROM sessions WHERE expires_at < ?1", params![now]);
        c.query_row(
            "SELECT username FROM sessions WHERE token=?1 AND expires_at >= ?2",
            params![token, now],
            |r| r.get(0),
        )
        .ok()
    }

    pub fn delete_session(&self, token: &str) {
        let c = self.lock();
        let _ = c.execute("DELETE FROM sessions WHERE token=?1", params![token]);
    }

    pub fn delete_user_sessions(&self, username: &str) {
        let c = self.lock();
        let _ = c.execute("DELETE FROM sessions WHERE username=?1", params![username]);
    }

    // ---------- clusters ----------

    pub fn insert_cluster(&self, c: &Cluster) -> Result<(), String> {
        let data = serde_json::to_string(c).map_err(|e| e.to_string())?;
        let lock = self.lock();
        lock.execute(
            "INSERT INTO clusters(id,name,status,progress,data) VALUES(?1,?2,?3,?4,?5)",
            params![c.id, c.name, c.status, c.progress, data],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn list_clusters(&self) -> Vec<Cluster> {
        let c = self.lock();
        let mut stmt = match c.prepare("SELECT data FROM clusters ORDER BY created_at DESC") {
            Ok(s) => s,
            Err(_) => return vec![],
        };
        stmt.query_map([], |r| r.get::<_, String>(0))
            .map(|rows| {
                rows.flatten()
                    .filter_map(|d| serde_json::from_str(&d).ok())
                    .collect()
            })
            .unwrap_or_default()
    }

    pub fn get_cluster(&self, id: &str) -> Option<Cluster> {
        let c = self.lock();
        c.query_row("SELECT data FROM clusters WHERE id=?1", params![id], |r| {
            r.get::<_, String>(0)
        })
        .ok()
        .and_then(|d| serde_json::from_str(&d).ok())
    }

    pub fn save_cluster(&self, c: &Cluster) {
        let data = match serde_json::to_string(c) {
            Ok(d) => d,
            Err(_) => return,
        };
        let lock = self.lock();
        let _ = lock.execute(
            "UPDATE clusters SET name=?1,status=?2,progress=?3,data=?4,updated_at=datetime('now') WHERE id=?5",
            params![c.name, c.status, c.progress, data, c.id],
        );
    }

    pub fn delete_cluster(&self, id: &str) {
        let c = self.lock();
        let _ = c.execute("DELETE FROM clusters WHERE id=?1", params![id]);
        let _ = c.execute("DELETE FROM logs WHERE cluster_id=?1", params![id]);
    }

    /// Mark interrupted deploys as error on boot. Returns affected ids.
    pub fn mark_interrupted(&self) -> Vec<String> {
        let c = self.lock();
        let ids: Vec<String> = c
            .prepare("SELECT id FROM clusters WHERE status IN ('deploying','provisioning')")
            .and_then(|mut s| {
                s.query_map([], |r| r.get(0))
                    .map(|rows| rows.flatten().collect())
            })
            .unwrap_or_default();
        for id in &ids {
            let _ = c.execute(
                "UPDATE clusters SET status='error',progress=0,updated_at=datetime('now') WHERE id=?1",
                params![id],
            );
        }
        ids
    }

    // ---------- logs ----------

    pub fn push_log(&self, e: &LogEntry) {
        let c = self.lock();
        let _ = c.execute(
            "INSERT INTO logs(id,cluster_id,phase,line,level,timestamp) VALUES(?1,?2,?3,?4,?5,?6)",
            params![
                e.id,
                e.cluster_id,
                e.phase,
                e.line,
                e.level,
                e.timestamp.to_rfc3339()
            ],
        );
        // prune old logs
        let _ = c.execute(
            "DELETE FROM logs WHERE cluster_id=?1 AND rowid NOT IN
             (SELECT rowid FROM logs WHERE cluster_id=?1 ORDER BY rowid DESC LIMIT ?2)",
            params![e.cluster_id, MAX_LOGS_PER_CLUSTER],
        );
    }

    pub fn get_logs(&self, cluster_id: &str) -> Vec<LogEntry> {
        let c = self.lock();
        let mut stmt =
            match c.prepare("SELECT id,cluster_id,phase,line,level,timestamp FROM logs WHERE cluster_id=?1 ORDER BY rowid ASC LIMIT 1000") {
                Ok(s) => s,
                Err(_) => return vec![],
            };
        stmt.query_map(params![cluster_id], |r| {
            let ts: String = r.get(5)?;
            Ok(LogEntry {
                id: r.get(0)?,
                cluster_id: r.get(1)?,
                phase: r.get(2)?,
                line: r.get(3)?,
                level: r.get(4)?,
                timestamp: ts.parse().unwrap_or_else(|_| chrono::Utc::now()),
            })
        })
        .map(|rows| rows.flatten().collect())
        .unwrap_or_default()
    }
}
