use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

fn default_proxmox_url() -> String {
    "https://192.168.1.100:8006/api2/json".to_string()
}
fn default_user() -> String {
    "root@pam".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Cluster {
    #[serde(default)]
    pub id: String,
    pub name: String,
    #[serde(default = "default_proxmox_url")]
    pub proxmox_url: String,
    #[serde(default = "default_user")]
    pub proxmox_user: String,
    /// Token ID part, e.g. `root@pam!panel`
    #[serde(default)]
    pub token_id: String,
    /// Token secret (never logged). Serialized but masked on list.
    #[serde(default)]
    pub token_secret: String,
    #[serde(default = "default_target_node")]
    pub target_node: String,
    #[serde(default = "default_template")]
    pub clone_template: String,
    #[serde(default = "default_bridge")]
    pub network_bridge: String,
    #[serde(default = "default_gateway")]
    pub gateway: String,
    #[serde(default = "default_dns")]
    pub dns1: String,
    #[serde(default = "default_master_count")]
    pub master_count: i32,
    #[serde(default = "default_worker_count")]
    pub worker_count: i32,
    #[serde(default = "default_master_cpu")]
    pub master_cpu: i32,
    #[serde(default = "default_master_ram")]
    pub master_ram: i32,
    #[serde(default = "default_worker_cpu")]
    pub worker_cpu: i32,
    #[serde(default = "default_worker_ram")]
    pub worker_ram: i32,
    #[serde(default = "default_ssh_user")]
    pub ssh_user: String,
    #[serde(default)]
    pub ssh_public_key: String,
/// Remote exec via ssh. Empty ssh_host = run locally on this host.
    #[serde(default)]
    pub ssh_host: String,
    #[serde(default = "default_ssh_port")]
    pub ssh_port: u16,
    #[serde(default)]
    pub ssh_remote_user: String,
    #[serde(default = "default_true")]
    pub use_wsl: bool,
    #[serde(default)]
    pub verify_tls: bool,
    #[serde(default = "default_features")]
    pub enabled_features: Vec<String>,
    #[serde(default = "default_status")]
    pub status: String,
    #[serde(default)]
    pub progress: i32,
    /// True when the last deploy finished without real provisioning
    /// (Proxmox unreachable or tools missing). Shown as a badge in UI.
    #[serde(default)]
    pub simulated: bool,
    #[serde(default = "now_utc")]
    pub created_at: DateTime<Utc>,
    #[serde(default = "now_utc")]
    pub updated_at: DateTime<Utc>,
}

fn default_target_node() -> String {
    "pve".to_string()
}
fn default_template() -> String {
    "ubuntu-22-04-cloudinit".to_string()
}
fn default_bridge() -> String {
    "vmbr0".to_string()
}
fn default_gateway() -> String {
    "192.168.1.1".to_string()
}
fn default_dns() -> String {
    "8.8.8.8".to_string()
}
fn default_master_count() -> i32 {
    1
}
fn default_worker_count() -> i32 {
    2
}
fn default_master_cpu() -> i32 {
    4
}
fn default_master_ram() -> i32 {
    8192
}
fn default_worker_cpu() -> i32 {
    2
}
fn default_worker_ram() -> i32 {
    4096
}
fn default_ssh_user() -> String {
    "ubuntu".to_string()
}
fn default_ssh_port() -> u16 {
    22
}
fn default_true() -> bool {
    true
}
fn default_features() -> Vec<String> {
    vec!["k8s".to_string(), "nginx".to_string()]
}
fn default_status() -> String {
    "pending".to_string()
}
fn now_utc() -> DateTime<Utc> {
    Utc::now()
}

impl Cluster {
    pub fn new(mut c: Cluster) -> Self {
        if c.id.is_empty() {
            c.id = Uuid::new_v4().to_string();
        }
        if c.master_count < 1 {
            c.master_count = 1;
        }
        if c.master_cpu < 1 {
            c.master_cpu = 4;
        }
        if c.master_ram < 1024 {
            c.master_ram = 8192;
        }
        if c.worker_cpu < 1 {
            c.worker_cpu = 2;
        }
        if c.worker_ram < 512 {
            c.worker_ram = 4096;
        }
        if c.ssh_user.is_empty() {
            c.ssh_user = "ubuntu".to_string();
        }
        if c.network_bridge.is_empty() {
            c.network_bridge = "vmbr0".to_string();
        }
        if c.gateway.is_empty() {
            c.gateway = "192.168.1.1".to_string();
        }
        if c.dns1.is_empty() {
            c.dns1 = "8.8.8.8".to_string();
        }
        if c.target_node.is_empty() {
            c.target_node = "pve".to_string();
        }
        if c.enabled_features.is_empty() {
            c.enabled_features = default_features();
        }
        let now = Utc::now();
        c.status = "pending".to_string();
        c.progress = 0;
        c.created_at = now;
        c.updated_at = now;
        c
    }

    /// Public view with secret masked.
    pub fn masked(&self) -> serde_json::Value {
        let mut v = serde_json::to_value(self).unwrap_or_default();
        if let Some(obj) = v.as_object_mut() {
            if let Some(s) = obj.get("token_secret").and_then(|x| x.as_str()) {
                if !s.is_empty() {
                    obj.insert("token_secret".to_string(), serde_json::Value::String("******".to_string()));
                }
            }
            obj.insert(
                "has_token".to_string(),
                serde_json::Value::Bool(!self.token_secret.is_empty()),
            );
        }
        v
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogEntry {
    pub id: String,
    pub cluster_id: String,
    pub phase: String,
    pub line: String,
    pub level: String,
    pub timestamp: DateTime<Utc>,
}

impl LogEntry {
    pub fn new(cluster_id: &str, phase: &str, line: &str, level: &str) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            cluster_id: cluster_id.to_string(),
            phase: phase.to_string(),
            line: line.to_string(),
            level: level.to_string(),
            timestamp: Utc::now(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProxmoxNode {
    pub name: String,
    pub status: String,
    pub cpu: f64,
    pub memory: u64,
    pub used_mem: u64,
    pub disk: u64,
    pub used_disk: u64,
    #[serde(default)]
    pub live: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemplateOption {
    pub name: String,
    pub storage: String,
    pub size: String,
    pub description: String,
}

// ---- request payloads ----

#[derive(Debug, Deserialize)]
pub struct ProxmoxTestRequest {
    pub proxmox_url: String,
    pub proxmox_user: String,
    pub token_id: String,
    pub token_secret: String,
    #[serde(default)]
    pub verify_tls: bool,
}

#[derive(Debug, Deserialize)]
pub struct SshTestRequest {
    pub ssh_host: String,
    #[serde(default = "default_ssh_remote")]
    pub ssh_user: String,
    #[serde(default = "default_ssh_port_fn")]
    pub ssh_port: u16,
}

fn default_ssh_remote() -> String {
    "root".to_string()
}
fn default_ssh_port_fn() -> u16 {
    22
}
