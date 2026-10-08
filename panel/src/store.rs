use crate::db::Db;
use crate::models::{Cluster, LogEntry};
use tokio::sync::broadcast;

#[derive(Debug, Clone)]
pub struct LogBroadcast {
    pub cluster_id: String,
    pub entry: LogEntry,
    pub status: String,
    pub progress: i32,
}

#[derive(Clone)]
pub struct AppState {
    pub db: Db,
    pub log_tx: broadcast::Sender<LogBroadcast>,
    pub infra_dir: String,
    pub static_dir: String,
}

impl AppState {
    pub fn new(db: Db, infra_dir: String, static_dir: String) -> Self {
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
        }
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
