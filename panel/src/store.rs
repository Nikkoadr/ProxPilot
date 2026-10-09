use crate::db::Db;
use crate::models::{Cluster, LogEntry};
use std::collections::HashSet;
use std::sync::{Arc, Mutex};
use tokio::sync::broadcast;

#[derive(Debug, Clone)]
pub struct LogBroadcast {
    pub cluster_id: String,
    pub entry: LogEntry,
    pub status: String,
    pub progress: i32,
}

/// Hasil eksekusi ansible ad-hoc (menu Configure). In-memory, maks 20 terakhir.
#[derive(Debug, Clone)]
pub struct ConfigRun {
    pub id: String,
    pub template: String,
    pub vms: Vec<String>,
    pub status: String,
    pub output: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Clone)]
pub struct AppState {
    pub db: Db,
    pub log_tx: broadcast::Sender<LogBroadcast>,
    pub infra_dir: String,
    pub static_dir: String,
    pub ansible_dir: String,
    /// Terraform ops (deploy/destroy/plan) yang sedang berjalan per cluster.
    /// Mencegah dua `apply` concurrent di folder yang sama (state lock).
    inflight: Arc<Mutex<HashSet<String>>>,
    config_runs: Arc<Mutex<std::collections::HashMap<String, ConfigRun>>>,
}

impl AppState {
    pub fn new(db: Db, infra_dir: String, static_dir: String, ansible_dir: String) -> Self {
        // Deployments interrupted by restart can never finish — mark error.
        for id in db.mark_interrupted() {
            let e = LogEntry::new(&id, "provision", "Panel restarted: deployment interrupted.", "error");
            db.push_log(&e);
        }
        let (tx, _) = broadcast::channel::<LogBroadcast>(512);
        Self {
            db,
            log_tx: tx,
            infra_dir,
            static_dir,
            ansible_dir,
            inflight: Arc::new(Mutex::new(HashSet::new())),
            config_runs: Arc::new(Mutex::new(std::collections::HashMap::new())),
        }
    }

    /// Coba tandai op terraform mulai. False = sudah ada op berjalan.
    pub fn try_begin_op(&self, cluster_id: &str) -> bool {
        match self.inflight.lock() {
            Ok(mut set) => set.insert(cluster_id.to_string()),
            Err(_) => false,
        }
    }

    pub fn end_op(&self, cluster_id: &str) {
        if let Ok(mut set) = self.inflight.lock() {
            set.remove(cluster_id);
        }
    }

    pub fn op_running(&self, cluster_id: &str) -> bool {
        self.inflight
            .lock()
            .map(|set| set.contains(cluster_id))
            .unwrap_or(false)
    }

    // ---------- configure runs (ad-hoc ansible) ----------

    pub fn create_config_run(&self, template: &str, vms: Vec<String>) -> String {
        let id = uuid::Uuid::new_v4().to_string().chars().take(8).collect::<String>();
        let run = ConfigRun {
            id: id.clone(),
            template: template.to_string(),
            vms,
            status: "running".to_string(),
            output: String::new(),
            created_at: chrono::Utc::now(),
        };
        if let Ok(mut map) = self.config_runs.lock() {
            map.insert(id.clone(), run);
            // Prune: simpan 20 terakhir saja.
            if map.len() > 20 {
                let mut ids: Vec<(String, chrono::DateTime<chrono::Utc>)> =
                    map.iter().map(|(k, v)| (k.clone(), v.created_at)).collect();
                ids.sort_by(|a, b| a.1.cmp(&b.1));
                for (old, _) in ids.iter().take(map.len() - 20) {
                    map.remove(old);
                }
            }
        }
        id
    }

    pub fn push_config_log(&self, id: &str, chunk: &str) {
        if let Ok(mut map) = self.config_runs.lock() {
            if let Some(r) = map.get_mut(id) {
                r.output.push_str(chunk);
                if r.output.len() > 30000 {
                    let cut = r.output.len() - 30000;
                    r.output = format!("...[truncated]\n{}", &r.output[cut..]);
                }
            }
        }
    }

    pub fn finish_config_run(&self, id: &str, status: &str) {
        if let Ok(mut map) = self.config_runs.lock() {
            if let Some(r) = map.get_mut(id) {
                r.status = status.to_string();
            }
        }
    }

    pub fn get_config_run(&self, id: &str) -> Option<ConfigRun> {
        self.config_runs.lock().ok()?.get(id).cloned()
    }

    pub fn push_log(&self, cluster_id: &str, phase: &str, line: &str, level: &str) {
        let entry = LogEntry::new(cluster_id, phase, line, level);
        self.db.push_log(&entry);
        let (status, progress) = self
            .db
            .get_cluster(cluster_id)
            .map(|c| (c.status, c.progress))
            .unwrap_or(("unknown".to_string(), 0));
        let _ = self.log_tx.send(LogBroadcast {
            cluster_id: cluster_id.to_string(),
            entry,
            status,
            progress,
        });
    }

    pub fn set_status(&self, cluster_id: &str, status: &str, progress: i32) {
        if let Some(mut c) = self.db.get_cluster(cluster_id) {
            c.status = status.to_string();
            c.progress = progress;
            c.updated_at = chrono::Utc::now();
            self.db.save_cluster(&c);
        }
        // notify listeners so progress bar moves realtime even without new log
        let entry = LogEntry::new(cluster_id, "status", &format!("status -> {status} ({progress}%)"), "info");
        let _ = self.log_tx.send(LogBroadcast {
            cluster_id: cluster_id.to_string(),
            entry,
            status: status.to_string(),
            progress,
        });
    }

    pub fn cluster_exists(&self, id: &str) -> bool {
        self.db.get_cluster(id).is_some()
    }

    pub fn set_simulated(&self, cluster_id: &str, simulated: bool) {
        if let Some(mut c) = self.db.get_cluster(cluster_id) {
            c.simulated = simulated;
            c.updated_at = chrono::Utc::now();
            self.db.save_cluster(&c);
        }
    }

    pub fn status_view(&self, id: &str) -> Option<serde_json::Value> {
        let c = self.db.get_cluster(id)?;
        let logs = self.db.get_logs(id);
        Some(serde_json::json!({
            "status": c.status,
            "progress": c.progress,
            "simulated": c.simulated,
            "logs": logs,
            "cluster": c.masked(),
        }))
    }

    pub fn summary_counts(&self) -> (usize, i32, i32, i32, i32) {
        let mut running = 0;
        let mut deploying = 0;
        let mut masters = 0;
        let mut workers = 0;
        let list = self.db.list_clusters();
        for c in &list {
            match c.status.as_str() {
                "running" => running += 1,
                "provisioning" | "deploying" => deploying += 1,
                _ => {}
            }
            masters += c.master_count;
            workers += c.worker_count;
        }
        (list.len(), running, deploying, masters, workers)
    }

    pub fn first_creds(&self) -> Option<crate::proxmox::ProxmoxCreds> {
        self.db.list_clusters().into_iter().next().map(|c| c.into_creds())
    }

    pub fn creds_of(&self, id: &str) -> Option<crate::proxmox::ProxmoxCreds> {
        self.db.get_cluster(id).map(|c| c.into_creds())
    }
}

// helper to keep routes.rs tidy
pub trait IntoCreds {
    fn into_creds(&self) -> crate::proxmox::ProxmoxCreds;
}
impl IntoCreds for Cluster {
    fn into_creds(&self) -> crate::proxmox::ProxmoxCreds {
        crate::proxmox::ProxmoxCreds {
            base_url: self.proxmox_url.clone(),
            user: self.proxmox_user.clone(),
            token_id: self.token_id.clone(),
            token_secret: self.token_secret.clone(),
            verify_tls: self.verify_tls,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn op_guard_exclusive_per_cluster() {
        // Satu-satunya test yang menyentuh PANEL_DATA — dir unik per run.
        let dir = std::env::temp_dir().join(format!("pp-store-test-{}", uuid::Uuid::new_v4()));
        std::env::set_var("PANEL_DATA", &dir);
        let db = Db::open().expect("open test db");
        let st = AppState::new(db, "/tmp/infra".to_string(), "/tmp/static".to_string(), "/tmp/ansible".to_string());
        assert!(st.try_begin_op("c1"));
        assert!(!st.try_begin_op("c1"), "op kedua harus ditolak");
        assert!(st.op_running("c1"));
        assert!(!st.op_running("c2"));
        assert!(st.try_begin_op("c2"), "cluster lain tidak terpengaruh");
        st.end_op("c1");
        assert!(!st.op_running("c1"));
        assert!(st.try_begin_op("c1"), "setelah selesai bisa mulai lagi");
        drop(st);
        std::env::remove_var("PANEL_DATA");
        std::fs::remove_dir_all(&dir).ok();
    }
}
