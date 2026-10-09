use crate::exec;
use crate::models::{Cluster, ProxmoxTestRequest, SshCopyIdRequest, SshTestRequest, TemplateOption};
use crate::proxmox::{self, ProxmoxCreds};
use crate::store::{AppState, IntoCreds};
use axum::{
    extract::{Path, Query, State, WebSocketUpgrade},
    http::StatusCode,
    response::{IntoResponse, Json},
    routing::{delete, get, post},
    Router,
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::HashMap;

pub fn router(state: AppState) -> Router {
    Router::new()
        // ---- clusters ----
        .route("/api/clusters", get(list_clusters).post(create_cluster))
        .route("/api/clusters/:id", delete(delete_cluster).put(update_cluster))
        .route("/api/clusters/:id/status", get(cluster_status))
        .route("/api/clusters/:id/deploy", post(deploy_cluster))
        .route("/api/clusters/:id/destroy", post(destroy_cluster))
        .route("/api/clusters/:id/plan", post(plan_cluster))
        .route("/api/clusters/:id/preflight", post(preflight))
        .route("/api/clusters/:id/refresh-ips", post(refresh_ips))
        .route("/api/clusters/:id/vms", get(cluster_vms))
        .route("/api/clusters/:id/vms/:vmid/:action", post(vm_action))
        // ---- setup / config ----
        .route("/api/setup", get(get_setup).put(put_setup))
        .route("/api/config", get(get_config))
        // ---- VM clone (direct Proxmox API) ----
        .route("/api/vms", get(list_all_vms))
        .route("/api/vms/clone", post(clone_vm))
        .route("/api/vms/:vmid", delete(delete_vm))
        .route("/api/vms/:vmid/ip", get(vm_ip_addr))
        .route("/api/vms/:vmid/:action", post(vm_power))
        // ---- configure (ansible) ----
        .route("/api/configure/templates", get(config_templates))
        .route("/api/configure", post(configure_run))
        .route("/api/configure/runs/:id", get(configure_status))
        // ---- nodes / templates ----
        .route("/api/nodes", get(list_nodes))
        .route("/api/templates", get(list_templates))
        // ---- health / tools / realtime ----
        .route("/api/tools", get(tools))
        .route("/api/realtime/summary", get(summary))
        .route("/api/health/proxmox-test", post(proxmox_test))
        // ---- SSH (from any host: WSL or native) ----
        .route("/api/ssh/test", post(ssh_test))
        .route("/api/ssh/key", get(ssh_key))
        .route("/api/ssh/keygen", post(ssh_keygen))
        .route("/api/ssh/copy-id", post(ssh_copy_id))
        // ---- websocket ----
        .route("/api/ws/logs/:id", get(ws_logs))
        .with_state(state)
}

// ---------- clusters ----------

async fn list_clusters(State(s): State<AppState>) -> Json<Value> {
    let v: Vec<Value> = s.db.list_clusters().iter().map(|c| c.masked()).collect();
    Json(json!(v))
}

async fn create_cluster(State(s): State<AppState>, Json(payload): Json<Cluster>) -> impl IntoResponse {
    if payload.name.trim().is_empty() {
        return (StatusCode::BAD_REQUEST, Json(json!({"error": "Cluster name is required"})));
    }
    if payload.token_secret.trim().is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "API Token secret is required"})),
        );
    }
    let c = Cluster::new(payload);
    let id = c.id.clone();
    let masked = c.masked();
    if s.db.insert_cluster(&c).is_err() {
        return (StatusCode::CONFLICT, Json(json!({"error": "cluster id already exists"})));
    }
    s.push_log(&id, "provision", "Cluster created. Click Deploy to start.", "info");
    (StatusCode::CREATED, Json(masked))
}

async fn cluster_status(State(s): State<AppState>, Path(id): Path<String>) -> impl IntoResponse {
    match s.status_view(&id) {
        Some(v) => (StatusCode::OK, Json(v)).into_response(),
        None => (StatusCode::NOT_FOUND, Json(json!({"error": "cluster not found"}))).into_response(),
    }
}

async fn delete_cluster(State(s): State<AppState>, Path(id): Path<String>) -> impl IntoResponse {
    s.db.delete_cluster(&id);
    (StatusCode::OK, Json(json!({"message": "cluster deleted"})))
}

/// PUT /api/clusters/:id — ubah koneksi/template cluster tanpa hapus.
/// Dipakai kalau alamat Proxmox / SSH / template berubah.
/// Field yang dijaga backend: id, status, progress, simulated, created_at.
/// `token_secret` kosong atau "******" = pakai secret lama.
async fn update_cluster(
    State(s): State<AppState>,
    Path(id): Path<String>,
    Json(payload): Json<Cluster>,
) -> impl IntoResponse {
    let mut cur = match s.db.get_cluster(&id) {
        Some(c) => c,
        None => return (StatusCode::NOT_FOUND, Json(json!({"error": "cluster not found"}))).into_response(),
    };
    if payload.name.trim().is_empty() {
        return (StatusCode::BAD_REQUEST, Json(json!({"error": "Cluster name is required"}))).into_response();
    }
    cur.name = payload.name.trim().to_string();
    cur.proxmox_url = payload.proxmox_url.trim().to_string();
    cur.proxmox_user = payload.proxmox_user.trim().to_string();
    if !payload.token_id.trim().is_empty() {
        cur.token_id = payload.token_id.trim().to_string();
    }
    let sec = payload.token_secret.trim().to_string();
    if !sec.is_empty() && sec != "******" {
        cur.token_secret = sec;
    }
    if !payload.target_node.trim().is_empty() {
        cur.target_node = payload.target_node.trim().to_string();
    }
    if !payload.clone_template.trim().is_empty() {
        cur.clone_template = payload.clone_template.trim().to_string();
    }
    if !payload.network_bridge.trim().is_empty() {
        cur.network_bridge = payload.network_bridge.trim().to_string();
    }
    // Disk: 0 = ikut template. Storage hanya dioverwrite bila diisi.
    if payload.disk_size_gb >= 0 {
        cur.disk_size_gb = payload.disk_size_gb;
    }
    if !payload.disk_storage.trim().is_empty() {
        cur.disk_storage = payload.disk_storage.trim().to_string();
    }
    // VLAN: valid 1-4094, selain itu = tanpa tag.
    if payload.vlan_tag >= 1 && payload.vlan_tag <= 4094 {
        cur.vlan_tag = payload.vlan_tag;
    } else if payload.vlan_tag == -1 {
        cur.vlan_tag = -1;
    }
    if !payload.gateway.trim().is_empty() {
        cur.gateway = payload.gateway.trim().to_string();
    }
    if !payload.dns1.trim().is_empty() {
        cur.dns1 = payload.dns1.trim().to_string();
    }
    // Mode IP: ganti dhcp<->static atau base berubah = IP simpanan lama
    // tidak valid lagi, bersihkan agar diisi ulang saat deploy berikutnya.
    let new_mode = if payload.ip_mode.trim() == "static" { "static" } else { "dhcp" };
    let new_base = payload.static_ip_base.trim().to_string();
    if cur.ip_mode != new_mode || cur.static_ip_base != new_base {
        cur.master_ips.clear();
        cur.worker_ips.clear();
    }
    cur.ip_mode = new_mode.to_string();
    cur.static_ip_base = new_base;
    // Prefix nama VM custom (kosong = otomatis). Ganti prefix + redeploy =
    // VM BARU (nama beda) — VM lama tidak dihapus otomatis, hapus manual.
    cur.vm_name_prefix = payload.vm_name_prefix.trim().to_string();
    // Fitur selalu dioverwrite (boleh kosong = VM polos, hanya common).
    cur.enabled_features = payload.enabled_features.clone();
    if !payload.ssh_user.trim().is_empty() {
        cur.ssh_user = payload.ssh_user.trim().to_string();
    }
    cur.ssh_host = payload.ssh_host.trim().to_string();
    cur.ssh_port = payload.ssh_port;
    cur.ssh_remote_user = payload.ssh_remote_user.trim().to_string();
    cur.verify_tls = payload.verify_tls;
    if payload.master_count >= 1 {
        cur.master_count = payload.master_count;
    }
    if payload.worker_count >= 0 {
        cur.worker_count = payload.worker_count;
    }
    if payload.master_cpu >= 1 {
        cur.master_cpu = payload.master_cpu;
    }
    if payload.master_ram >= 512 {
        cur.master_ram = payload.master_ram;
    }
    if payload.worker_cpu >= 1 {
        cur.worker_cpu = payload.worker_cpu;
    }
    if payload.worker_ram >= 512 {
        cur.worker_ram = payload.worker_ram;
    }
    cur.updated_at = chrono::Utc::now();
    s.db.save_cluster(&cur);
    // Fitur (isi VM) bisa berubah tanpa deploy — tulis ulang run-ansible.sh
    // agar menu Configure + file selalu sinkron. Terraform lain butuh deploy.
    let dir = format!("{}/terraform/{}", s.infra_dir, cur.id);
    if std::fs::create_dir_all(&dir).is_ok() {
        if let Err(e) = write_run_ansible(&dir, &cur) {
            s.push_log(&id, "provision", &format!("WARNING: run-ansible.sh gagal ditulis ulang ({e})"), "warning");
        }
    }
    s.push_log(&id, "provision", "Connection settings updated. Re-deploy to apply (kecuali fitur: langsung berlaku di menu Configure).", "info");
    (StatusCode::OK, Json(cur.masked())).into_response()
}

async fn deploy_cluster(State(s): State<AppState>, Path(id): Path<String>) -> impl IntoResponse {
    if !s.cluster_exists(&id) {
        return (StatusCode::NOT_FOUND, Json(json!({"error": "cluster not found"})));
    }
    if !s.try_begin_op(&id) {
        return (
            StatusCode::CONFLICT,
            Json(json!({"error": "another terraform operation is already running for this cluster"})),
        );
    }
    let bg = s.clone();
    let id2 = id.clone();
    tokio::spawn(async move {
        run_deployment(bg.clone(), &id2).await;
        bg.end_op(&id2);
    });
    (StatusCode::OK, Json(json!({"message": "deployment started", "id": id})))
}

/// POST /api/clusters/:id/destroy — hapus SEMUA VM via
/// `terraform destroy -auto-approve`. Definisi cluster (DB) tetap tersimpan
/// sehingga bisa Deploy ulang. Status kembali `pending`, IP dibersihkan.
async fn destroy_cluster(State(s): State<AppState>, Path(id): Path<String>) -> impl IntoResponse {
    if !s.cluster_exists(&id) {
        return (StatusCode::NOT_FOUND, Json(json!({"error": "cluster not found"})));
    }
    if !s.try_begin_op(&id) {
        return (
            StatusCode::CONFLICT,
            Json(json!({"error": "another terraform operation is already running for this cluster"})),
        );
    }
    let bg = s.clone();
    let id2 = id.clone();
    tokio::spawn(async move {
        run_destroy(bg.clone(), &id2).await;
        bg.end_op(&id2);
    });
    (StatusCode::OK, Json(json!({"message": "destroy started", "id": id})))
}

/// POST /api/clusters/:id/plan — `terraform init + plan` tanpa apply.
/// Read-only terhadap infra (tidak membuat/menghapus VM). Async seperti
/// deploy: hasilnya mengalir ke log fase `plan`. Ditolak (409) bila ada
/// op terraform lain berjalan untuk cluster ini.
async fn plan_cluster(State(s): State<AppState>, Path(id): Path<String>) -> impl IntoResponse {
    if !s.cluster_exists(&id) {
        return (StatusCode::NOT_FOUND, Json(json!({"error": "cluster not found"})));
    }
    if !s.try_begin_op(&id) {
        return (
            StatusCode::CONFLICT,
            Json(json!({"error": "another terraform operation is already running for this cluster"})),
        );
    }
    let bg = s.clone();
    let id2 = id.clone();
    tokio::spawn(async move {
        run_plan(bg.clone(), &id2).await;
        bg.end_op(&id2);
    });
    (StatusCode::OK, Json(json!({"message": "plan started", "id": id})))
}

// ---------- preflight (cek kesiapan, tanpa mengubah apa pun) ----------

/// POST /api/clusters/:id/preflight — cek berurutan sebelum Deploy:
/// API Proxmox nyambung? template cloud-init ADA? terraform ada?
/// (remote: ssh nyambung?) + sanity mode IP. Read-only: tidak mengubah
/// status, tidak menulis file, tidak membuat VM.
async fn preflight(State(s): State<AppState>, Path(id): Path<String>) -> impl IntoResponse {
    let cluster = match s.db.get_cluster(&id) {
        Some(c) => c,
        None => return (StatusCode::NOT_FOUND, Json(json!({"error": "cluster not found"}))).into_response(),
    };
    let mut checks: Vec<serde_json::Value> = vec![];
    let mut check = |name: &str, ok: bool, detail: String| {
        checks.push(json!({"name": name, "ok": ok, "detail": detail}));
    };

    // 1. API Proxmox (dari host panel — jalur yang sama dipakai deploy).
    let creds = cluster.into_creds();
    let api = proxmox::test_connection(&creds).await;
    let api_ok = api.get("ok").and_then(|x| x.as_bool()).unwrap_or(false);
    check(
        "proxmox_api",
        api_ok,
        if api_ok {
            format!(
                "OK (versi {}, {} ms)",
                api.get("version").and_then(|x| x.as_str()).unwrap_or("?"),
                api.get("latency_ms").and_then(|x| x.as_u64()).unwrap_or(0)
            )
        } else {
            api.get("error").and_then(|x| x.as_str()).unwrap_or("unreachable").to_string()
        },
    );

    // 2. Template cloud-init ADA di node target? (penyebab gagal apply paling konyol)
    // Cari di semua node agar target_node yang salah tidak menyesatkan.
    if api_ok {
        match fetch_vms_any(&s, None).await {
            Some((all, found_node, ok_nodes)) => {
                match all.iter().find(|v| v.template && v.name == cluster.clone_template) {
                    Some(_) => check(
                        "template",
                        true,
                        format!("{} ada (node {}, dicari di {:?})", cluster.clone_template, found_node, ok_nodes),
                    ),
                    None => {
                        let sample: Vec<String> = all.iter().take(8).map(|v| format!("{}(template={})", v.name, v.template)).collect();
                        check(
                            "template",
                            false,
                            format!(
                                "TIDAK ADA: '{}' tidak ditemukan (dicari di {:?}, {} VM terlihat: {}). Buat template dulu di Proxmox (Convert to template) atau betulkan nama template.",
                                cluster.clone_template, ok_nodes, all.len(), sample.join(", ")
                            ),
                        );
                    }
                }
            }
            None => check("template", false, "gagal list VM di semua node".to_string()),
        }
    } else {
        check("template", false, "skip (API gagal)".to_string());
    }

    // 3. Terraform + (remote: SSH) di jalur eksekusi yang akan dipakai deploy.
    let use_remote = !cluster.ssh_host.trim().is_empty();
    let via_wsl = tokio::task::spawn_blocking(exec::wsl_available).await.unwrap_or(false);
    if use_remote {
        let (h, u, p) = (cluster.ssh_host.clone(), cluster.ssh_remote_user_or_default(), cluster.ssh_port);
        let ssh = tokio::task::spawn_blocking(move || exec::ssh_test_auto(&h, &u, p, via_wsl))
            .await
            .unwrap_or(exec::CmdResult { ok: false, output: "ssh task gagal".into(), ms: 0 });
        check("ssh", ssh.ok, truncate(&ssh.output, 300));
        if ssh.ok {
            let (h2, u2) = (cluster.ssh_host.clone(), cluster.ssh_remote_user_or_default());
            let tf = tokio::task::spawn_blocking(move || {
                exec::ssh_exec_auto(&h2, &u2, p, "terraform version 2>&1 | head -1", via_wsl)
            })
            .await
            .unwrap_or(exec::CmdResult { ok: false, output: "task gagal".into(), ms: 0 });
            check("terraform", tf.ok, truncate(&tf.output, 200));
        } else {
            check("terraform", false, "skip (SSH gagal)".to_string());
        }
    } else {
        check("ssh", true, "skip (local mode)".to_string());
        let tf = tokio::task::spawn_blocking(move || {
            if via_wsl {
                exec::tool_version_wsl("terraform")
            } else {
                exec::tool_version_local("terraform")
            }
        })
        .await
        .unwrap_or(exec::CmdResult { ok: false, output: "task gagal".into(), ms: 0 });
        check("terraform", tf.ok, truncate(&tf.output.lines().next().unwrap_or("").to_string(), 200));
    }

    // 4. Sanity mode IP statik.
    if cluster.ip_mode == "static" {
        match static_ips_for_cluster(&cluster) {
            Some((m, w)) => check(
                "static_ip",
                true,
                format!("master [{}] worker [{}]", m.join(", "), w.join(", ")),
            ),
            None => check("static_ip", false, "static_ip_base invalid / blok lewat .254".to_string()),
        }
    } else {
        check("static_ip", true, "skip (DHCP)".to_string());
    }

    let ok = checks.iter().all(|c| c.get("ok").and_then(|x| x.as_bool()).unwrap_or(false));
    (StatusCode::OK, Json(json!({"ok": ok, "checks": checks}))).into_response()
}

// ---------- nodes / templates / config ----------

#[derive(Debug, Deserialize)]
struct NodesQuery {
    cluster_id: Option<String>,
    node: Option<String>,
}

async fn list_nodes(State(s): State<AppState>, Query(q): Query<NodesQuery>) -> Json<Value> {
    // Setup dulu (sumber utama alur Clone VM), lalu cluster — sebelumnya
    // Setup tidak pernah dicoba sehingga tanpa cluster selalu mock "pve".
    let mut candidates: Vec<ProxmoxCreds> = vec![];
    if let Some(c) = q.cluster_id.as_deref().and_then(|id| s.creds_of(id)) {
        candidates.push(c);
    }
    if let Some(c) = setup_creds(&s.db) {
        candidates.push(c);
    }
    if let Some(c) = s.first_creds() {
        candidates.push(c);
    }

    for c in candidates {
        if !c.token_secret.is_empty() {
            if let Ok(nodes) = proxmox::list_nodes(&c).await {
                if !nodes.is_empty() {
                    return Json(json!(nodes));
                }
            }
        }
    }
    Json(json!(proxmox::mock_nodes()))
}

/// Kredensial kandidat berurutan: Setup dulu, lalu semua cluster.
/// Dipakai pencarian VM/template agar hasil test di Health langsung
/// kepakai tanpa harus buat cluster dulu.
fn candidate_creds(s: &AppState) -> Vec<ProxmoxCreds> {
    let mut out: Vec<ProxmoxCreds> = vec![];
    if let Some(c) = setup_creds(&s.db) {
        out.push(c);
    }
    for cl in s.db.list_clusters() {
        if !cl.token_secret.trim().is_empty() {
            out.push(cl.into_creds());
        }
    }
    out
}

/// Coba list VM ke SEMUA node dan gabungkan hasilnya.
/// Urutan node per kredensial: override -> setup_node/cluster node -> semua live nodes.
/// Mengembalikan (semua vms gabungan, node primer, daftar node yang sukses).
/// Agregasi ini yang memperbaiki "template tidak ketemu padahal node sudah
/// benar" — template bisa ada di node lain dari node primer.
async fn fetch_vms_any(
    s: &AppState,
    node_override: Option<String>,
) -> Option<(Vec<crate::proxmox::VmInfo>, String, Vec<String>)> {
    let creds_list = candidate_creds(s);
    if creds_list.is_empty() {
        return None;
    }
    // Kumpulkan cluster target_nodes untuk dicoba juga.
    let cluster_nodes: Vec<String> = s
        .db
        .list_clusters()
        .into_iter()
        .map(|c| c.target_node)
        .filter(|n| !n.trim().is_empty())
        .collect();

    for c in &creds_list {
        // Jalur utama: /cluster/resources?type=vm (se-cluster, tanpa tebak node).
        if node_override.as_deref().map(|o| o.trim().is_empty()).unwrap_or(true) {
            if let Ok(cluster_all) = proxmox::list_vms_cluster(c).await {
                let mut merged: Vec<crate::proxmox::VmInfo> = vec![];
                let mut ok_nodes: Vec<String> = vec![];
                for (v, node) in cluster_all {
                    if !ok_nodes.contains(&node) {
                        ok_nodes.push(node);
                    }
                    if !merged.iter().any(|m: &crate::proxmox::VmInfo| m.vmid == v.vmid) {
                        merged.push(v);
                    }
                }
                // Cluster-resources sukses (walau 0) = jawaban resmi API.
                // Kembalikan langsung agar tidak tertutup hasil per-node yang usang.
                ok_nodes.sort();
                merged.sort_by(|a, b| a.name.cmp(&b.name));
                let primary = ok_nodes.first().cloned().unwrap_or_else(|| setup_node(&s.db));
                return Some((merged, primary, ok_nodes));
            }
        }
        let mut try_nodes: Vec<String> = vec![];
        if let Some(o) = node_override.clone() {
            if !o.trim().is_empty() {
                try_nodes.push(o.trim().to_string());
            }
        }
        let setup_n = setup_node(&s.db);
        if !try_nodes.contains(&setup_n) {
            try_nodes.push(setup_n);
        }
        for n in &cluster_nodes {
            if !try_nodes.contains(n) {
                try_nodes.push(n.clone());
            }
        }
        // Node live dari API (nama asli server) — kunci perbaikan nama salah.
        if let Ok(live_nodes) = proxmox::list_nodes(c).await {
            for n in live_nodes {
                if !try_nodes.contains(&n.name) {
                    try_nodes.push(n.name);
                }
            }
        }
        let mut merged: Vec<crate::proxmox::VmInfo> = vec![];
        let mut ok_nodes: Vec<String> = vec![];
        for node in try_nodes {
            if let Ok(vms) = proxmox::list_vms(c, &node).await {
                ok_nodes.push(node.clone());
                for v in vms {
                    if !merged.iter().any(|m: &crate::proxmox::VmInfo| m.vmid == v.vmid) {
                        merged.push(v);
                    }
                }
            }
        }
        if !ok_nodes.is_empty() {
            merged.sort_by(|a, b| a.name.cmp(&b.name));
            let primary = ok_nodes[0].clone();
            return Some((merged, primary, ok_nodes));
        }
    }
    None
}

/// Cari template berdasarkan nama di SEMUA node (bukan cuma setup_node).
/// Mengembalikan (vmid template, node asalnya).
async fn resolve_template_any(s: &AppState, template_name: &str) -> Option<(u64, String)> {
    let creds_list = candidate_creds(s);
    // 1. Jalur cepat: cluster-resources (se-cluster sekaligus).
    for c in &creds_list {
        if let Ok(all) = proxmox::list_vms_cluster(c).await {
            if let Some((v, node)) = all.iter().find(|(v, _)| v.template && v.name == template_name) {
                return Some((v.vmid, node.clone()));
            }
        }
    }
    // 2. Fallback per-node (untuk server lama tanpa /cluster/resources).
    // Kumpulkan kandidat node sama seperti fetch_vms_any.
    let mut try_nodes: Vec<String> = vec![];
    let setup_n = setup_node(&s.db);
    try_nodes.push(setup_n);
    for cl in s.db.list_clusters() {
        if !cl.target_node.trim().is_empty() && !try_nodes.contains(&cl.target_node) {
            try_nodes.push(cl.target_node);
        }
    }
    for c in &creds_list {
        if let Ok(live_nodes) = proxmox::list_nodes(c).await {
            for n in live_nodes {
                if !try_nodes.contains(&n.name) {
                    try_nodes.push(n.name);
                }
            }
        }
    }
    for c in &creds_list {
        for node in &try_nodes {
            if let Ok(all) = proxmox::list_vms(c, node).await {
                if let Some(v) = all.iter().find(|v| v.template && v.name == template_name) {
                    return Some((v.vmid, node.clone()));
                }
            }
        }
    }
    None
}

/// Fallback template statis bila Setup/API belum bisa (dipakai list_templates).
fn static_templates() -> Vec<Value> {
    vec![
        json!({"name": "ubuntu-22-04-cloudinit", "vmid": null, "storage": "local", "size": "4GB", "description": "Ubuntu 22.04 LTS Cloud-Init"}),
        json!({"name": "ubuntu-24-04-cloudinit", "vmid": null, "storage": "local", "size": "4GB", "description": "Ubuntu 24.04 LTS Cloud-Init"}),
        json!({"name": "debian-12-cloudinit", "vmid": null, "storage": "local", "size": "3GB", "description": "Debian 12 Cloud-Init"}),
        json!({"name": "rocky-9-cloudinit", "vmid": null, "storage": "local", "size": "4GB", "description": "Rocky Linux 9 Cloud-Init (user: rocky)"}),
        json!({"name": "rocky-8-cloudinit", "vmid": null, "storage": "local", "size": "4GB", "description": "Rocky Linux 8 Cloud-Init (user: rocky)"}),
    ]
}

async fn get_config() -> Json<Value> {
    Json(json!({
        "defaults": {
            "proxmox_url": "https://192.168.1.100:8006/api2/json",
            "proxmox_user": "root@pam",
            "master_cpu": 4, "master_ram": 8192,
            "worker_cpu": 2, "worker_ram": 4096,
            "ssh_user": "ubuntu", "network_bridge": "vmbr0",
            "gateway": "192.168.1.1", "dns1": "8.8.8.8",
            "ssh_port": 22, "use_wsl": true, "verify_tls": false
        },
        "features": ["k8s", "nginx", "nodejs"]
    }))
}

// ---------- health / tools / realtime ----------

async fn tools() -> Json<Value> {
    Json(exec::tools_summary())
}

async fn summary(State(s): State<AppState>) -> Json<Value> {
    let (total, running, deploying, masters, workers) = s.summary_counts();
    let wsl = exec::wsl_available();
    Json(json!({
        "clusters_total": total,
        "running": running, "deploying": deploying,
        "master_nodes": masters, "worker_nodes": workers,
        "wsl_available": wsl,
        "runtime": exec::runtime_info_cached(wsl),
        "server_time": chrono::Utc::now(),
    }))
}

async fn proxmox_test(Json(mut req): Json<ProxmoxTestRequest>) -> Json<Value> {
    req.proxmox_url = req.proxmox_url.trim().to_string();
    req.proxmox_user = req.proxmox_user.trim().to_string();
    req.token_id = req.token_id.trim().trim_start_matches('!').to_string();
    req.token_secret = req.token_secret.trim().to_string();
    if req.proxmox_url.trim().is_empty() || req.token_secret.trim().is_empty() {
        return Json(json!({"ok": false, "error": "proxmox_url and token_secret are required"}));
    }
    // Common paste mistake: full "user!id" in the token_id box.
    if req.token_id.contains('!') {
        return Json(json!({"ok": false,
            "error": "token_id must be only the part after '!'", 
            "hint": "Kamu paste full 'root@pam!panel'. Isi token_id dengan 'panel' saja (setelah '!').",
            "got": req.token_id}));
    }
    if req.token_id.contains('@') || req.token_id.contains(' ') {
        return Json(json!({"ok": false,
            "error": "token_id looks wrong",
            "hint": "token_id hanya nama token (mis. 'panel'), tanpa '@', spasi, atau '!'.",
            "got": req.token_id}));
    }
    let creds = ProxmoxCreds {
        base_url: req.proxmox_url,
        user: if req.proxmox_user.is_empty() {
            "root@pam".to_string()
        } else {
            req.proxmox_user
        },
        token_id: req.token_id,
        token_secret: req.token_secret,
        verify_tls: req.verify_tls,
    };
    Json(proxmox::test_connection(&creds).await)
}

async fn ssh_test(Json(req): Json<SshTestRequest>) -> Json<Value> {
    if req.ssh_host.trim().is_empty() {
        return Json(json!({"ok": false, "error": "ssh_host is required"}));
    }
    // Direct ssh (panel runs on Linux; keys in ~/.ssh), BatchMode so it
    // fails fast instead of hanging on password prompt.
    let via_wsl = exec::wsl_available();
    let host = req.ssh_host.clone();
    let user = if req.ssh_user.is_empty() {
        "root".to_string()
    } else {
        req.ssh_user
    };
    let port = req.ssh_port;
    // Run blocking ssh via spawn_blocking to avoid blocking the runtime.
    let (h1, u1) = (host.clone(), user.clone());
    let r = tokio::task::spawn_blocking(move || {
        if via_wsl {
            exec::ssh_test_via_wsl(&h1, &u1, port)
        } else {
            exec::ssh_test_native(&h1, &u1, port)
        }
    })
    .await
    .unwrap_or(exec::CmdResult { ok: false, output: "ssh task join failed".into(), ms: 0 });
    // Kalau gagal, sertakan diagnosa: key apa yang tersedia + metode auth server.
    // Ini menjawab kasus "PuTTY bisa, panel ditolak".
    let (keys, auth_debug) = if r.ok {
        (String::new(), String::new())
    } else {
        let (h2, u2) = (host.clone(), user.clone());
        tokio::task::spawn_blocking(move || {
            (exec::ssh_keys_list(via_wsl), exec::ssh_auth_debug(&h2, &u2, port, via_wsl))
        })
        .await
        .unwrap_or_default()
    };
    Json(json!({"ok": r.ok, "output": r.output, "ms": r.ms, "via": if via_wsl { "wsl" } else { "native" },
        "keys": keys, "auth_debug": auth_debug}))
}

/// GET /api/ssh/key — public key host panel (setup sekali, lalu copy ke server).
async fn ssh_key() -> Json<Value> {
    let via_wsl = exec::wsl_available();
    // spawn_blocking: fungsi exec memanggil proses eksternal yang blocking.
    let key = tokio::task::spawn_blocking(move || exec::ssh_pubkey_get(via_wsl))
        .await
        .unwrap_or(None);
    Json(json!({
        "exists": key.is_some(),
        "public_key": key.unwrap_or_default(),
        "via": if via_wsl { "wsl" } else { "native" },
    }))
}

/// POST /api/ssh/keygen — buat key ed25519 kalau belum ada (idempoten).
async fn ssh_keygen() -> Json<Value> {
    let via_wsl = exec::wsl_available();
    let r = tokio::task::spawn_blocking(move || exec::ssh_keygen(via_wsl))
        .await
        .unwrap_or(exec::CmdResult { ok: false, output: "keygen task join failed".into(), ms: 0 });
    Json(json!({"ok": r.ok, "public_key": if r.ok { r.output.clone() } else { String::new() },
        "output": if r.ok { "ok".to_string() } else { r.output }, "ms": r.ms,
        "via": if via_wsl { "wsl" } else { "native" }}))
}

/// POST /api/ssh/copy-id — `ssh-copy-id` dengan password sekali saja.
/// Password tidak disimpan; tidak pernah masuk log.
async fn ssh_copy_id(Json(req): Json<SshCopyIdRequest>) -> Json<Value> {
    if req.ssh_host.trim().is_empty() {
        return Json(json!({"ok": false, "error": "ssh_host is required"}));
    }
    if req.ssh_password.is_empty() {
        return Json(json!({"ok": false, "error": "ssh_password is required (one-time, never stored)"}));
    }
    let via_wsl = exec::wsl_available();
    let (host, user, port, pw) = (
        req.ssh_host.clone(),
        if req.ssh_user.is_empty() { "root".to_string() } else { req.ssh_user.clone() },
        req.ssh_port,
        req.ssh_password.clone(),
    );
    let r = tokio::task::spawn_blocking(move || exec::ssh_copy_id(&host, &user, port, &pw, via_wsl))
        .await
        .unwrap_or(exec::CmdResult { ok: false, output: "copy-id task join failed".into(), ms: 0 });
    Json(json!({"ok": r.ok, "output": r.output, "ms": r.ms,
        "via": if via_wsl { "wsl" } else { "native" }}))
}

// ---------- websocket ----------

async fn ws_logs(
    State(s): State<AppState>,
    Path(id): Path<String>,
    ws: WebSocketUpgrade,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| ws_task(socket, s, id))
}

async fn ws_task(
    mut socket: axum::extract::ws::WebSocket,
    s: AppState,
    id: String,
) {
    use axum::extract::ws::Message;
    use futures::StreamExt;
    let mut rx = s.log_tx.subscribe();

    // initial snapshot
    if let Some(st) = s.db.get_cluster(&id) {
        let logs = s.db.get_logs(&id);
        let init = json!({
            "type": "logs",
            "cluster_id": id,
            "logs": logs,
            "status": st.status,
            "progress": st.progress,
        });
        let _ = socket.send(Message::Text(init.to_string())).await;
    } else {
        let _ = socket
            .send(Message::Text(json!({"type":"error","error":"cluster not found"}).to_string()))
            .await;
        return;
    }
    // heartbeat + broadcast forward
    let mut tick = tokio::time::interval(std::time::Duration::from_secs(5));
    loop {
        tokio::select! {
            msg = rx.recv() => {
                match msg {
                    Ok(b) if b.cluster_id == id => {
                        let payload = json!({
                            "type": "log",
                            "cluster_id": b.cluster_id,
                            "entry": b.entry,
                            "status": b.status,
                            "progress": b.progress,
                        });
                        if socket.send(Message::Text(payload.to_string())).await.is_err() {
                            break;
                        }
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                    _ => {}
                }
            }
            _ = tick.tick() => {
                if let Some(st) = s.db.get_cluster(&id) {
                    let ping = json!({
                        "type": "heartbeat",
                        "cluster_id": id,
                        "status": st.status,
                        "progress": st.progress,
                        "server_time": chrono::Utc::now(),
                    });
                    if socket.send(Message::Text(ping.to_string())).await.is_err() {
                        break;
                    }
                } else {
                    break;
                }
            }
            msg = socket.next() => {
                match msg {
                    Some(Ok(Message::Close(_))) | None => break,
                    Some(Ok(Message::Text(t))) => {
                        // client subscribe / ping -> reply snapshot
                        if t.contains("subscribe") || t.contains("ping") {
                            if let Some(st) = s.db.get_cluster(&id) {
                                let logs = s.db.get_logs(&id);
                                let snap = json!({
                                    "type": "logs",
                                    "cluster_id": id,
                                    "logs": logs,
                                    "status": st.status,
                                    "progress": st.progress,
                                });
                                if socket.send(Message::Text(snap.to_string())).await.is_err() {
                                    break;
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
    }
}

// ---------- deployment engine ----------

async fn run_deployment(state: AppState, id: &str) {
    let cluster = match state.db.get_cluster(id) {
        Some(s) => s,
        None => return,
    };
    let id = id.to_string();
    state.set_simulated(&id, false);
    state.set_status(&id, "deploying", 5);
    state.push_log(&id, "provision", "Validating cluster configuration (API Token auth)...", "info");

    // Validate token present
    if cluster.token_secret.is_empty() {
        state.push_log(&id, "provision", "ERROR: API Token secret is empty.", "error");
        state.set_status(&id, "error", 0);
        return;
    }
    tokio::time::sleep(std::time::Duration::from_millis(600)).await;

    // 1. Test Proxmox connection live. Abort on failure — a green
    // "success" without reachable Proxmox would be a lie.
    state.set_status(&id, "deploying", 12);
    state.push_log(&id, "provision", &format!("Testing Proxmox API {} ...", cluster.proxmox_url), "info");
    let creds = ProxmoxCreds {
        base_url: cluster.proxmox_url.clone(),
        user: cluster.proxmox_user.clone(),
        token_id: cluster.token_id.clone(),
        token_secret: cluster.token_secret.clone(),
        verify_tls: cluster.verify_tls,
    };
    let test = proxmox::test_connection(&creds).await;
    let proxmox_ok = test.get("ok").and_then(|x| x.as_bool()).unwrap_or(false);
    if proxmox_ok {
        state.push_log(
            &id,
            "provision",
            &format!(
                "Proxmox OK (version {}, {} ms)",
                test.get("version").and_then(|x| x.as_str()).unwrap_or("?"),
                test.get("latency_ms").and_then(|x| x.as_u64()).unwrap_or(0)
            ),
            "info",
        );
    } else {
        state.push_log(
            &id,
            "provision",
            &format!(
                "Proxmox unreachable ({}). Aborting: no VMs were created. Fix URL/token/TLS, then re-deploy.",
                test.get("error").and_then(|x| x.as_str()).unwrap_or("unknown error")
            ),
            "error",
        );
        state.set_simulated(&id, true);
        state.set_status(&id, "error", 12);
        return;
    }

    // 2. Generate terraform files (real token secret, mode 0600).
    state.set_status(&id, "deploying", 22);
    state.push_log(&id, "provision", "Generating Terraform configuration...", "info");
    // Ambil public key SSH host panel untuk diinject via cloud-init
    // (ciuser+sshkeys) — ansible selalu bisa login ke VM hasil clone.
    let panel_key = panel_ssh_key().await;
    if panel_key.is_none() {
        state.push_log(
            &id,
            "provision",
            "WARNING: public key SSH panel tidak ditemukan (Generate key di Health dulu). VM mengandalkan key bawaan template — ansible bisa gagal login.",
            "warning",
        );
    }
    if let Err(e) = write_terraform(&state.infra_dir, &cluster, panel_key.as_deref()) {
        state.push_log(&id, "provision", &format!("Failed to write terraform files: {e}"), "error");
        state.set_status(&id, "error", 22);
        return;
    }
    state.push_log(
        &id,
        "provision",
        &format!(
            "Terraform files written to {}/terraform/{}/ (main.tf, terraform.tfvars [0600], inventory.ini, deploy-remote.sh)",
            state.infra_dir, cluster.id
        ),
        "info",
    );
    // Static mode: IP sudah pasti sebelum apply — simpan ke DB sekarang juga
    // (tanpa menunggu guest-agent), inventory.ini ikut memakai IP asli.
    if cluster.ip_mode == "static" {
        match static_ips_for_cluster(&cluster) {
            Some((m, w)) => {
                save_discovered_ips(&state, &id, m.clone(), w.clone());
                state.push_log(
                    &id,
                    "provision",
                    &format!(
                        "Static IP mode: masters [{}] workers [{}] (gw {}). cloud-init tiap VM dikonfigurasi statis.",
                        m.join(", "),
                        w.join(", "),
                        cluster.gateway
                    ),
                    "info",
                );
            }
            None => {
                state.push_log(
                    &id,
                    "provision",
                    "WARNING: ip_mode=static tapi static_ip_base invalid (format A.B.C.D, blok tidak boleh lewat .254). Fallback ke DHCP untuk deploy ini.",
                    "warning",
                );
            }
        }
    }

    // 3. Provision for real (terraform apply). No fake K8s phases:
    // every log line below reflects a command that actually ran.
    let use_remote = !cluster.ssh_host.trim().is_empty();
    let provisioned = if use_remote {
        state.push_log(
            &id,
            "provision",
            &format!(
                "Remote mode: provisioning on {}@{}:{} via ssh",
                cluster.ssh_remote_user_or_default(),
                cluster.ssh_host,
                cluster.ssh_port
            ),
            "info",
        );
        remote_path(&state, &id, &cluster).await
    } else {
        state.push_log(&id, "provision", "Local mode: provisioning on this host.", "info");
        local_path(&state, &id, &cluster).await
    };
    if !provisioned {
        // local_path/remote_path already logged the specific error.
        state.set_simulated(&id, true);
        state.set_status(&id, "error", 40);
        state.push_log(&id, "provision", "Provisioning FAILED — no VMs were created. Fix the error above, then re-deploy.", "error");
        return;
    }

    // 4. VMs exist now. Capture `terraform output` (DHCP IPs) -> simpan ke DB
    // + tulis ulang inventory.ini agar ansible tidak pakai default bawaan.
    state.set_status(&id, "deploying", 85);
    let (masters, workers) = fetch_and_save_ips(&state, &id, &cluster).await;
    if masters.is_empty() && workers.is_empty() {
        state.push_log(
            &id,
            "provision",
            "WARNING: terraform apply OK tapi `terraform output` kosong (qemu-guest-agent belum lapor IP / DHCP lambat). inventory.ini masih placeholder — klik Refresh IPs / re-deploy setelah agent aktif.",
            "warning",
        );
    } else {
        state.push_log(
            &id,
            "provision",
            &format!(
                "Discovered IPs — masters: [{}] workers: [{}]. Tersimpan di DB + inventory.ini.",
                masters.join(", "),
                workers.join(", ")
            ),
            "info",
        );
    }
    state.set_status(&id, "deploying", 90);
    state.push_log(&id, "provision", "Terraform apply succeeded — VMs are provisioned.", "info");
    let inv = format!("{}/terraform/{}/inventory.ini", state.infra_dir, cluster.id);
    let runner = format!("{}/terraform/{}/run-ansible.sh", state.infra_dir, cluster.id);
    // Terraform selesai di sini. Isi VM (ansible) langkah terpisah di menu Configure.
    state.push_log(
        &id,
        "ansible",
        &format!(
            "VMs ready (kosong sesuai template — isi via menu Configure). inventory: {inv}\nNext: buka menu Configure (/configure.html?id={}) untuk memilih isi VM, atau jalankan {runner} dari repo root.",
            cluster.id
        ),
        "info",
    );

    state.set_simulated(&id, false);
    state.set_status(&id, "running", 100);
    state.push_log(&id, "provision", "Provisioning complete. Cluster is marked running.", "info");
}

/// Provision on this host: `terraform init` + `terraform apply`.
/// Returns true only if `terraform apply` exited 0 (VMs really exist).
async fn local_path(state: &AppState, id: &str, cluster: &Cluster) -> bool {
    let via_wsl = tokio::task::spawn_blocking(exec::wsl_available)
        .await
        .unwrap_or(false);
    let dir = format!("{}/terraform/{}", state.infra_dir, cluster.id);

    if !via_wsl {
        // Direct on-host path (panel runs on Linux).
        let tf = tokio::task::spawn_blocking(|| exec::tool_version_local("terraform"))
            .await
            .unwrap_or(exec::CmdResult { ok: false, output: "tool check failed".into(), ms: 0 });
        if !tf.ok {
            state.push_log(id, "provision", "terraform not found on this host. Install it (or use remote mode), then re-deploy.", "error");
            return false;
        }
        state.push_log(id, "provision", &format!("Terraform native: {}", tf.output.lines().next().unwrap_or("")), "info");
        state.set_status(id, "deploying", 32);
        state.push_log(id, "provision", "Exec: terraform init -input=false (native)...", "info");
        let dir2 = dir.clone();
        let init = tokio::task::spawn_blocking(move || {
            std::process::Command::new("terraform")
                .args(["init", "-input=false"])
                .current_dir(&dir2)
                .output()
        })
        .await;
        match init {
            Ok(Ok(o)) if o.status.success() => {
                let s = String::from_utf8_lossy(&o.stdout).to_string();
                state.push_log(id, "provision", &format!("terraform init ok:\n{}", truncate(&s, 1200)), "info");
            }
            Ok(Ok(o)) => {
                let err = String::from_utf8_lossy(&o.stderr).to_string();
                state.push_log(id, "provision", &format!("terraform init FAILED:\n{}", truncate(&err, 1500)), "error");
                return false;
            }
            _ => {
                state.push_log(id, "provision", "terraform init (native) gagal dijalankan.", "error");
                return false;
            }
        }
        state.set_status(id, "deploying", 55);
        state.push_log(id, "provision", "Exec: terraform apply -auto-approve -input=false (native)...", "info");
        let apply = tokio::task::spawn_blocking(move || {
            std::process::Command::new("terraform")
                .args(["apply", "-auto-approve", "-input=false"])
                .current_dir(&dir)
                .output()
        })
        .await;
        match apply {
            Ok(Ok(o)) => {
                let out = format!(
                    "{}\n[stderr]\n{}",
                    String::from_utf8_lossy(&o.stdout),
                    String::from_utf8_lossy(&o.stderr)
                );
                let ok = o.status.success();
                state.push_log(id, "provision", &format!("terraform apply ok={ok}:\n{}", truncate(&out, 2000)), if ok { "info" } else { "error" });
                return ok;
            }
            _ => {
                state.push_log(id, "provision", "terraform apply (native) gagal dijalankan.", "error");
                return false;
            }
        }
    }
    // WSL-bridged path (binary runs on Windows, tools live in WSL).
    let tools = tokio::task::spawn_blocking(exec::tools_summary).await.unwrap_or(json!({}));
    let tf_ok = tools.pointer("/terraform/wsl/ok").and_then(|x| x.as_bool()).unwrap_or(false);
    state.push_log(
        id,
        "provision",
        &format!(
            "Tools (wsl): terraform={} | {}",
            tf_ok,
            tools.pointer("/terraform/wsl/output").and_then(|x| x.as_str()).unwrap_or("-"),
        ),
        if tf_ok { "info" } else { "error" },
    );
    if !tf_ok {
        state.push_log(id, "provision", "terraform not found in WSL (run install.sh to install it), then re-deploy.", "error");
        return false;
    }
    state.set_status(id, "deploying", 32);
    let dir_wsl = win_to_wsl(&dir);
    let cmd = format!(
        "cd \"{}\" && terraform init -input=false 2>&1 && terraform apply -auto-approve -input=false 2>&1 | tail -40",
        dir_wsl.replace('"', "\\\"")
    );
    state.push_log(id, "provision", "Exec (wsl): terraform init + apply...", "info");
    let out = tokio::task::spawn_blocking(move || exec::run_in_wsl(&cmd)).await;
    match out {
        Ok(r) => {
            state.push_log(
                id,
                "provision",
                &format!("terraform init+apply ok={}:\n{}", r.ok, truncate(&r.output, 2000)),
                if r.ok { "info" } else { "error" },
            );
            r.ok
        }
        Err(e) => {
            state.push_log(id, "provision", &format!("Exec failed: {e}"), "error");
            false
        }
    }
}

/// Provision on the Proxmox server over ssh: scp the generated dir,
/// then `terraform init + apply` there. Returns true only if the
/// remote apply exited 0.
async fn remote_path(state: &AppState, id: &str, cluster: &Cluster) -> bool {
    let host = cluster.ssh_host.clone();
    let user = cluster.ssh_remote_user_or_default();
    let port = cluster.ssh_port;
    let via_wsl = tokio::task::spawn_blocking(exec::wsl_available)
        .await
        .unwrap_or(false);
    // 1. SSH connectivity test (native on Linux, WSL-bridged on Windows).
    state.push_log(id, "provision", &format!("SSH: ssh -p {port} {user}@{host} ..."), "info");
    let (h, u) = (host.clone(), user.clone());
    let r = tokio::task::spawn_blocking(move || exec::ssh_test_auto(&h, &u, port, via_wsl)).await;
    match r {
        Ok(res) if res.ok => {
            state.push_log(id, "provision", &format!("SSH OK ({} ms):\n{}", res.ms, truncate(&res.output, 800)), "info");
        }
        Ok(res) => {
            state.push_log(
                id,
                "provision",
                &format!("SSH FAILED ({} ms). Check ~/.ssh keys on this host + authorized_keys on Proxmox. Output:\n{}", res.ms, truncate(&res.output, 800)),
                "error",
            );
            return false;
        }
        Err(e) => {
            state.push_log(id, "provision", &format!("SSH task failed: {e}"), "error");
            return false;
        }
    }
    // 2. Remote tool versions
    let (host2, user2) = (host.clone(), user.clone());
    let r2 = tokio::task::spawn_blocking(move || {
        exec::ssh_exec_auto(&host2, &user2, port, "terraform version 2>&1 | head -2; echo ---; ansible --version 2>&1 | head -2", via_wsl)
    })
    .await;
    match r2 {
        Ok(res) => state.push_log(
            id,
            "provision",
            &format!("Remote tools ({} ms):\n{}", res.ms, truncate(&res.output, 800)),
            if res.ok { "info" } else { "warning" },
        ),
        Err(e) => state.push_log(id, "provision", &format!("Remote tool check failed: {e}"), "warning"),
    }
    // 3. Sync generated files to the server.
    let local_dir = format!("{}/terraform/{}", state.infra_dir, cluster.id);
    let scp_src = if via_wsl { win_to_wsl(&local_dir) } else { local_dir.clone() };
    let remote_dir = format!("/tmp/proxpilot/{}", cluster.id);
    state.set_status(id, "deploying", 45);
    state.push_log(id, "provision", &format!("Sync: scp -r -> {user}@{host}:{remote_dir} ..."), "info");
    let (h3, u3, d3) = (host.clone(), user.clone(), remote_dir.clone());
    let mkdir = tokio::task::spawn_blocking(move || {
        exec::ssh_exec_auto(&h3, &u3, port, &format!("mkdir -p '{remote_dir_s}'", remote_dir_s = d3.replace('\'', "'\\''")), via_wsl)
    })
    .await;
    if !matches!(mkdir, Ok(ref r) if r.ok) {
        state.push_log(id, "provision", "Remote mkdir failed — aborting.", "error");
        return false;
    }
    let (h4, u4) = (host.clone(), user.clone());
    let scp_dst = format!("{remote_dir}/");
    let cp = tokio::task::spawn_blocking(move || {
        exec::scp_to_remote(&h4, &u4, port, &scp_src, &scp_dst, via_wsl)
    })
    .await;
    match cp {
        Ok(res) if res.ok => {
            state.push_log(id, "provision", "Sync OK.", "info");
        }
        Ok(res) => {
            state.push_log(id, "provision", &format!("Sync (scp) FAILED:\n{}", truncate(&res.output, 1200)), "error");
            state.push_log(id, "provision", "Fallback manual: run the generated deploy-remote.sh from a shell that has the files.", "warning");
            return false;
        }
        Err(e) => {
            state.push_log(id, "provision", &format!("Sync task failed: {e}"), "error");
            return false;
        }
    }
    // 4. Remote init + apply.
    state.set_status(id, "deploying", 60);
    state.push_log(id, "provision", "Exec (remote): terraform init + apply -auto-approve ...", "info");
    let (h5, u5, d5) = (host.clone(), user.clone(), remote_dir.clone());
    let ap = tokio::task::spawn_blocking(move || {
        exec::ssh_exec_auto(
            &h5,
            &u5,
            port,
            &format!("cd '{d5}' && terraform init -input=false 2>&1 && terraform apply -auto-approve -input=false 2>&1 | tail -40"),
            via_wsl,
        )
    })
    .await;
    match ap {
        Ok(res) => {
            state.push_log(
                id,
                "provision",
                &format!("Remote terraform ok={} ({} ms):\n{}", res.ok, res.ms, truncate(&res.output, 2500)),
                if res.ok { "info" } else { "error" },
            );
            res.ok
        }
        Err(e) => {
            state.push_log(id, "provision", &format!("Remote apply task failed: {e}"), "error");
            false
        }
    }
}

// ---------- destroy engine ----------

async fn run_destroy(state: AppState, id: &str) {
    let cluster = match state.db.get_cluster(id) {
        Some(s) => s,
        None => return,
    };
    let id = id.to_string();
    state.set_status(&id, "deploying", 5);
    state.push_log(&id, "destroy", "Destroying VMs with `terraform destroy -auto-approve` ...", "info");
    let use_remote = !cluster.ssh_host.trim().is_empty();
    let destroyed = if use_remote {
        remote_destroy(&state, &id, &cluster).await
    } else {
        local_destroy(&state, &id, &cluster).await
    };
    if !destroyed {
        state.set_status(&id, "error", 5);
        state.push_log(&id, "destroy", "Destroy FAILED — lihat error di atas. Sebagian VM mungkin masih ada di Proxmox; cek manual.", "error");
        return;
    }
    // Bersihkan IP simpanan (inventory kembali placeholder), status ke pending.
    // Definisi cluster tetap ada — siap Deploy ulang kapan saja.
    save_discovered_ips(&state, &id, vec![], vec![]);
    state.set_simulated(&id, false);
    state.set_status(&id, "pending", 0);
    state.push_log(&id, "destroy", "Destroy complete — semua VM dihapus. Definisi cluster tersimpan, siap Deploy ulang.", "info");
}

/// `terraform destroy` di host ini (native / WSL bridge).
async fn local_destroy(state: &AppState, id: &str, cluster: &Cluster) -> bool {
    let via_wsl = tokio::task::spawn_blocking(exec::wsl_available)
        .await
        .unwrap_or(false);
    let dir = format!("{}/terraform/{}", state.infra_dir, cluster.id);
    state.set_status(id, "deploying", 20);
    if !via_wsl {
        state.push_log(id, "destroy", "Exec: terraform destroy -auto-approve -input=false (native)...", "info");
        let dir2 = dir.clone();
        let out = tokio::task::spawn_blocking(move || {
            std::process::Command::new("terraform")
                .args(["destroy", "-auto-approve", "-input=false"])
                .current_dir(&dir2)
                .output()
        })
        .await;
        return destroy_output_done(state, id, out);
    }
    let dir_wsl = win_to_wsl(&dir);
    let cmd = format!(
        "cd \"{}\" && terraform destroy -auto-approve -input=false 2>&1 | tail -30",
        dir_wsl.replace('"', "\\\"")
    );
    state.push_log(id, "destroy", "Exec (wsl): terraform destroy...", "info");
    match tokio::task::spawn_blocking(move || exec::run_in_wsl(&cmd)).await {
        Ok(r) => {
            let no_state = r.output.contains("No state");
            let ok = r.ok || no_state;
            state.push_log(
                id,
                "destroy",
                &format!("terraform destroy ok={ok}:\n{}", truncate(&r.output, 2000)),
                if ok { "info" } else { "error" },
            );
            ok
        }
        Err(e) => {
            state.push_log(id, "destroy", &format!("Exec failed: {e}"), "error");
            false
        }
    }
}

/// `terraform destroy` di server Proxmox via ssh (remote mode).
async fn remote_destroy(state: &AppState, id: &str, cluster: &Cluster) -> bool {
    let host = cluster.ssh_host.clone();
    let user = cluster.ssh_remote_user_or_default();
    let port = cluster.ssh_port;
    let remote_dir = format!("/tmp/proxpilot/{}", cluster.id);
    let via_wsl = tokio::task::spawn_blocking(exec::wsl_available)
        .await
        .unwrap_or(false);
    state.set_status(id, "deploying", 20);
    state.push_log(id, "destroy", &format!("Exec (remote): terraform destroy di {user}@{host}:{remote_dir} ..."), "info");
    let (h, u) = (host.clone(), user.clone());
    match tokio::task::spawn_blocking(move || {
        exec::ssh_exec_auto(
            &h,
            &u,
            port,
            &format!("cd '{remote_dir}' && terraform destroy -auto-approve -input=false 2>&1 | tail -30"),
            via_wsl,
        )
    })
    .await
    {
        Ok(res) => {
            let no_state = res.output.contains("No state");
            let ok = res.ok || no_state;
            state.push_log(
                id,
                "destroy",
                &format!("Remote terraform destroy ok={ok} ({} ms):\n{}", res.ms, truncate(&res.output, 2000)),
                if ok { "info" } else { "error" },
            );
            ok
        }
        Err(e) => {
            state.push_log(id, "destroy", &format!("Remote destroy task failed: {e}"), "error");
            false
        }
    }
}

/// Public key SSH host panel (None bila belum Generate di Health).
/// Dijalankan di thread blocking karena mem-spawn proses eksternal.
async fn panel_ssh_key() -> Option<String> {
    tokio::task::spawn_blocking(|| {
        let via_wsl = exec::wsl_available();
        exec::ssh_pubkey_get(via_wsl)
    })
    .await
    .unwrap_or(None)
}

// ---------- plan engine (preview tanpa apply) ----------

async fn run_plan(state: AppState, id: &str) {
    let cluster = match state.db.get_cluster(id) {
        Some(s) => s,
        None => return,
    };
    let id = id.to_string();
    state.push_log(&id, "plan", "Plan preview: generate files + `terraform init + plan` (TIDAK ada yang diterapkan)...", "info");
    let panel_key = panel_ssh_key().await;
    if let Err(e) = write_terraform(&state.infra_dir, &cluster, panel_key.as_deref()) {
        state.push_log(&id, "plan", &format!("Failed to write terraform files: {e}"), "error");
        return;
    }
    let use_remote = !cluster.ssh_host.trim().is_empty();
    if use_remote {
        remote_plan(&state, &id, &cluster).await;
    } else {
        local_plan(&state, &id, &cluster).await;
    }
    state.push_log(&id, "plan", "Plan finished — tidak ada perubahan yang diterapkan. Klik Deploy untuk eksekusi.", "info");
}

/// Ringkasan satu baris `Plan: X to add, ...` dari output (None bila tak ada).
fn plan_summary(output: &str) -> Option<String> {
    output
        .lines()
        .map(str::trim)
        .find(|l| l.starts_with("Plan:"))
        .map(|l| l.to_string())
}

/// `terraform init + plan` di host ini (native / WSL bridge).
async fn local_plan(state: &AppState, id: &str, cluster: &Cluster) {
    let via_wsl = tokio::task::spawn_blocking(exec::wsl_available)
        .await
        .unwrap_or(false);
    let dir = format!("{}/terraform/{}", state.infra_dir, cluster.id);
    if !via_wsl {
        for (step, args) in [
            ("init", vec!["init", "-input=false"]),
            ("plan", vec!["plan", "-input=false", "-no-color"]),
        ] {
            state.push_log(id, "plan", &format!("Exec: terraform {step} (native)..."), "info");
            let dir2 = dir.clone();
            let out = tokio::task::spawn_blocking(move || {
                std::process::Command::new("terraform")
                    .args(&args)
                    .current_dir(&dir2)
                    .output()
            })
            .await;
            match out {
                Ok(Ok(o)) => {
                    let txt = format!(
                        "{}\n[stderr]\n{}",
                        String::from_utf8_lossy(&o.stdout),
                        String::from_utf8_lossy(&o.stderr)
                    );
                    let ok = o.status.success();
                    state.push_log(
                        id,
                        "plan",
                        &format!("terraform {step} ok={ok}:\n{}", truncate(&txt, 2500)),
                        if ok { "info" } else { "error" },
                    );
                    if !ok {
                        return;
                    }
                    if step == "plan" {
                        if let Some(sum) = plan_summary(&txt) {
                            state.push_log(id, "plan", &format!("Ringkasan: {sum}"), "info");
                        }
                    }
                }
                _ => {
                    state.push_log(id, "plan", &format!("terraform {step} (native) gagal dijalankan."), "error");
                    return;
                }
            }
        }
        return;
    }
    let dir_wsl = win_to_wsl(&dir);
    let cmd = format!(
        "cd \"{}\" && terraform init -input=false 2>&1 | tail -5 && terraform plan -input=false -no-color 2>&1 | tail -60",
        dir_wsl.replace('"', "\\\"")
    );
    state.push_log(id, "plan", "Exec (wsl): terraform init + plan...", "info");
    match tokio::task::spawn_blocking(move || exec::run_in_wsl(&cmd)).await {
        Ok(r) => {
            state.push_log(
                id,
                "plan",
                &format!("terraform init+plan ok={}:\n{}", r.ok, truncate(&r.output, 2500)),
                if r.ok { "info" } else { "error" },
            );
            if r.ok {
                if let Some(sum) = plan_summary(&r.output) {
                    state.push_log(id, "plan", &format!("Ringkasan: {sum}"), "info");
                }
            }
        }
        Err(e) => {
            state.push_log(id, "plan", &format!("Exec failed: {e}"), "error");
        }
    }
}

/// `terraform init + plan` di server Proxmox via ssh (remote mode).
/// File di-sync dulu (mkdir + scp), sama seperti deploy.
async fn remote_plan(state: &AppState, id: &str, cluster: &Cluster) {
    let host = cluster.ssh_host.clone();
    let user = cluster.ssh_remote_user_or_default();
    let port = cluster.ssh_port;
    let via_wsl = tokio::task::spawn_blocking(exec::wsl_available)
        .await
        .unwrap_or(false);
    let remote_dir = match remote_sync(state, id, cluster, "plan", via_wsl, &host, &user, port).await {
        Some(d) => d,
        None => return,
    };
    state.push_log(id, "plan", "Exec (remote): terraform init + plan ...", "info");
    let (h, u) = (host.clone(), user.clone());
    match tokio::task::spawn_blocking(move || {
        exec::ssh_exec_auto(
            &h,
            &u,
            port,
            &format!("cd '{remote_dir}' && terraform init -input=false 2>&1 | tail -5 && terraform plan -input=false -no-color 2>&1 | tail -60"),
            via_wsl,
        )
    })
    .await
    {
        Ok(res) => {
            state.push_log(
                id,
                "plan",
                &format!("Remote terraform plan ok={} ({} ms):\n{}", res.ok, res.ms, truncate(&res.output, 2500)),
                if res.ok { "info" } else { "error" },
            );
            if res.ok {
                if let Some(sum) = plan_summary(&res.output) {
                    state.push_log(id, "plan", &format!("Ringkasan: {sum}"), "info");
                }
            }
        }
        Err(e) => {
            state.push_log(id, "plan", &format!("Remote plan task failed: {e}"), "error");
        }
    }
}

/// mkdir + scp file cluster ke server. Returns remote_dir bila sukses.
/// (remote_path punya alur serupa inline; helper ini untuk plan agar mandiri.)
async fn remote_sync(
    state: &AppState,
    id: &str,
    cluster: &Cluster,
    phase: &str,
    via_wsl: bool,
    host: &str,
    user: &str,
    port: u16,
) -> Option<String> {
    let local_dir = format!("{}/terraform/{}", state.infra_dir, cluster.id);
    let scp_src = if via_wsl { win_to_wsl(&local_dir) } else { local_dir.clone() };
    let remote_dir = format!("/tmp/proxpilot/{}", cluster.id);
    state.push_log(id, phase, &format!("Sync: scp -r -> {user}@{host}:{remote_dir} ..."), "info");
    let (h, u, d) = (host.to_string(), user.to_string(), remote_dir.clone());
    let mkdir = tokio::task::spawn_blocking(move || {
        exec::ssh_exec_auto(&h, &u, port, &format!("mkdir -p '{remote_dir_s}'", remote_dir_s = d.replace('\'', "'\\''")), via_wsl)
    })
    .await;
    if !matches!(mkdir, Ok(ref r) if r.ok) {
        state.push_log(id, phase, "Remote mkdir failed — aborting.", "error");
        return None;
    }
    let (h2, u2) = (host.to_string(), user.to_string());
    let scp_dst = format!("{remote_dir}/");
    match tokio::task::spawn_blocking(move || exec::scp_to_remote(&h2, &u2, port, &scp_src, &scp_dst, via_wsl)).await {
        Ok(res) if res.ok => {
            state.push_log(id, phase, "Sync OK.", "info");
            Some(remote_dir)
        }
        Ok(res) => {
            state.push_log(id, phase, &format!("Sync (scp) FAILED:\n{}", truncate(&res.output, 1200)), "error");
            None
        }
        Err(e) => {
            state.push_log(id, phase, &format!("Sync task failed: {e}"), "error");
            None
        }
    }
}
fn destroy_output_done(
    state: &AppState,
    id: &str,
    out: Result<Result<std::process::Output, std::io::Error>, tokio::task::JoinError>,
) -> bool {
    let out = match out {
        Ok(Ok(o)) => o,
        Ok(Err(e)) => {
            state.push_log(id, "destroy", &format!("terraform destroy gagal dijalankan: {e}"), "error");
            return false;
        }
        Err(e) => {
            state.push_log(id, "destroy", &format!("terraform destroy task gagal: {e}"), "error");
            return false;
        }
    };
    let txt = format!(
        "{}\n[stderr]\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    let ok = out.status.success() || txt.contains("No state");
    state.push_log(
        id,
        "destroy",
        &format!("terraform destroy ok={ok}:\n{}", truncate(&txt, 2000)),
        if ok { "info" } else { "error" },
    );
    ok
}

// ---------- terraform filegen ----------

/// Generate file terraform per cluster. `panel_key` = public key SSH host
/// panel (None bila belum ada) — diinject via cloud-init (ciuser+sshkeys)
/// agar ansible selalu bisa login, apa pun default user template-nya.
fn write_terraform(infra_dir: &str, c: &Cluster, panel_key: Option<&str>) -> std::io::Result<()> {
    let dir = format!("{infra_dir}/terraform/{}", c.id);
    std::fs::create_dir_all(&dir)?;
    // Restrict the dir: terraform.tfvars below holds the real API token.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700));
    }
    // NOTE: the real token secret is required here — `terraform apply`
    // cannot authenticate with a redacted placeholder.
    // Static IP mode: hitung ipconfig cloud-init per-VM dari static_ip_base.
    // List kosong = fallback "ip=dhcp" di main.tf (lihat conditional di sana).
    let (master_cfg, worker_cfg) = static_ipconfigs_for_cluster(c);
    // Prefix nama VM dari nama cluster + id pendek — tiap cluster/project
    // dapat nama VM unik, tidak tabrakan di Proxmox yang sama.
    let prefix = vm_prefix(c);
    let hcl_list = |items: &[String]| {
        format!(
            "[{}]",
            items
                .iter()
                .map(|s| format!("\"{}\"", hcl_escape(s)))
                .collect::<Vec<_>>()
                .join(", ")
        )
    };
    // Pasangan key=value diratakan seperti `terraform fmt` (lebar = key terpanjang).
    let pairs: Vec<(&str, String)> = vec![
        ("proxmox_api_url", format!("\"{}\"", hcl_escape(&c.proxmox_url))),
        (
            "proxmox_user",
            format!("\"{}:{}\"", hcl_escape(&c.proxmox_user), hcl_escape(&c.token_id)),
        ),
        ("proxmox_token", format!("\"{}\"", hcl_escape(&c.token_secret))),
        ("target_node", format!("\"{}\"", hcl_escape(&c.target_node))),
        ("clone_template", format!("\"{}\"", hcl_escape(&c.clone_template))),
        ("network_bridge", format!("\"{}\"", hcl_escape(&c.network_bridge))),
        ("gateway", format!("\"{}\"", hcl_escape(&c.gateway))),
        ("dns1", format!("\"{}\"", hcl_escape(&c.dns1))),
        ("ssh_user", format!("\"{}\"", hcl_escape(&c.ssh_user))),
        ("master_count", c.master_count.to_string()),
        ("worker_count", c.worker_count.to_string()),
        ("master_cpu", c.master_cpu.to_string()),
        ("master_ram", c.master_ram.to_string()),
        ("worker_cpu", c.worker_cpu.to_string()),
        ("worker_ram", c.worker_ram.to_string()),
        ("master_ipconfigs", hcl_list(&master_cfg)),
        ("worker_ipconfigs", hcl_list(&worker_cfg)),
        ("name_prefix", format!("\"{}\"", hcl_escape(&prefix))),
        ("ciuser", hcl_str_or_null(Some(vm_ssh_user(c)))),
        ("sshkeys", hcl_str_or_null(panel_key)),
        ("disk_size_gb", c.disk_size_gb.to_string()),
        ("disk_storage", format!("\"{}\"", hcl_escape(&c.disk_storage))),
        ("vlan_tag", c.vlan_tag.to_string()),
    ];
    let width = pairs.iter().map(|(k, _)| k.len()).max().unwrap_or(0);
    let mut vars = String::new();
    for (k, v) in &pairs {
        vars.push_str(&format!("{k:<width$} = {v}\n", width = width));
    }
    let tfvars = format!("{dir}/terraform.tfvars");
    std::fs::write(&tfvars, vars)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&tfvars, std::fs::Permissions::from_mode(0o600));
    }
    std::fs::write(format!("{dir}/main.tf"), terraform_main_tf())?;
    // Ansible inventory sized to the requested counts. Prioritas isi:
    // 1. IP statis (diketahui sebelum deploy — tanpa perlu guest-agent),
    // 2. IP simpanan DB (hasil deploy sebelumnya),
    // 3. placeholder (dioverwrite setelah `terraform output` sukses).
    let inv_cluster = match static_ips_for_cluster(c) {
        Some((m, w)) => {
            let mut cc = c.clone();
            cc.master_ips = m;
            cc.worker_ips = w;
            cc
        }
        None => c.clone(),
    };
    std::fs::write(format!("{dir}/inventory.ini"), build_inventory(&inv_cluster))?;
    // Manual fallback for remote apply (used automatically by remote mode,
    // runnable by hand when scp/ssh from the panel is unavailable).
    let remote_user = if c.ssh_remote_user.trim().is_empty() {
        "root".to_string()
    } else {
        c.ssh_remote_user.clone()
    };
    let script = format!(
        "#!/usr/bin/env bash\n# Manual remote apply for cluster '{}' ({})\n# Usage: ./deploy-remote.sh   (needs scp/ssh access to the Proxmox server)\nset -euo pipefail\nSRC=\"$(dirname \"$0\")\"\nREMOTE_DIR=\"/tmp/proxpilot/{}\"\nssh -p {} {}@{} \"mkdir -p '$REMOTE_DIR'\"\nscp -P {} -r \"$SRC/.\" \"{}@{}:$REMOTE_DIR/\"\nssh -p {} {}@{} \"cd '$REMOTE_DIR' && terraform init -input=false && terraform apply -auto-approve -input=false\"\n",
        c.name, c.id, c.id, c.ssh_port, remote_user, c.ssh_host,
        c.ssh_port, remote_user, c.ssh_host, c.ssh_port, remote_user, c.ssh_host,
    );
    let script_path = format!("{dir}/deploy-remote.sh");
    std::fs::write(&script_path, script)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&script_path, std::fs::Permissions::from_mode(0o755));
    }
    // run-ansible.sh: pengisi VM sesuai fitur cluster (menu Configure).
    // Dijalankan dari repo root (atau set ANSIBLE_DIR=<path folder ansible/>).
    // File ini ikut ter-sync ke server pada remote mode (scp -r seluruh dir).
    // Ditulis ulang juga tiap PUT /api/clusters/:id (ganti fitur tanpa deploy).
    write_run_ansible(&dir, c)?;
    Ok(())
}

/// Tulis (ulang) run-ansible.sh sesuai fitur cluster saat ini.
fn write_run_ansible(dir: &str, c: &Cluster) -> std::io::Result<()> {
    let mut run = format!(
        "#!/usr/bin/env bash\n# Generated by proxpilot for cluster '{}' ({})\n# Isi VM sesuai fitur: {}\n# Usage: ./run-ansible.sh   (dari repo root; butuh ansible terinstal)\nset -euo pipefail\nINV=\"$(dirname \"$0\")/inventory.ini\"\nANSIBLE_DIR=\"${{ANSIBLE_DIR:-ansible}}\"\n",
        c.name,
        c.id,
        if c.enabled_features.is_empty() { "common only".to_string() } else { c.enabled_features.join(",") },
    );
    for (pb, desc) in ansible_steps(c) {
        run.push_str(&format!("\n# {desc}\nansible-playbook -i \"$INV\" \"$ANSIBLE_DIR/{pb}\"\n"));
    }
    let run_path = format!("{dir}/run-ansible.sh");
    std::fs::write(&run_path, run)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&run_path, std::fs::Permissions::from_mode(0o755));
    }
    Ok(())
}

/// Hitung IP statis dari `static_ip_base` (mis. "192.168.1.50", /24).
/// Master mengambil N pertama, worker melanjutkan setelahnya.
/// None bila mode dhcp, base bukan IPv4 valid, atau blok melewati .254.
fn static_ips_for_cluster(c: &Cluster) -> Option<(Vec<String>, Vec<String>)> {
    if c.ip_mode.trim() != "static" {
        return None;
    }
    let parts: Vec<&str> = c.static_ip_base.trim().split('.').collect();
    if parts.len() != 4 {
        return None;
    }
    let mut oct = [0u8; 4];
    for (i, p) in parts.iter().enumerate() {
        oct[i] = p.parse::<u8>().ok()?;
    }
    if oct[3] == 0 {
        return None;
    }
    let m = c.master_count.max(1) as u16;
    let w = c.worker_count.max(0) as u16;
    if oct[3] as u16 + m + w - 1 > 254 {
        return None;
    }
    let ip = |last: u8| format!("{}.{}.{}.{last}", oct[0], oct[1], oct[2]);
    let masters = (0..m).map(|i| ip(oct[3] + i as u8)).collect();
    let workers = (0..w).map(|i| ip(oct[3] + m as u8 + i as u8)).collect();
    Some((masters, workers))
}

/// ipconfig cloud-init per-VM ("ip=.../24,gw=...") untuk terraform.tfvars.
/// Kosong = main.tf fallback ke "ip=dhcp".
fn static_ipconfigs_for_cluster(c: &Cluster) -> (Vec<String>, Vec<String>) {
    let (m, w) = match static_ips_for_cluster(c) {
        Some(v) => v,
        None => return (vec![], vec![]),
    };
    let gw = c.gateway.trim();
    let cfg = |ips: Vec<String>| {
        ips.into_iter()
            .map(|ip| {
                if gw.is_empty() {
                    format!("ip={ip}/24")
                } else {
                    format!("ip={ip}/24,gw={gw}")
                }
            })
            .collect()
    };
    (cfg(m), cfg(w))
}

/// Prefix nama VM per cluster: custom bila diisi, else slug(nama) — selalu +
/// 8 char id agar unik. Mis. prefix "web-1" -> "web-1-a1b2c3d4", VM jadi
/// "web-1-a1b2c3d4-master-0". Menjamin tiap project unik di Proxmox yang
/// sama (nama VM Proxmox tidak boleh kembar).
fn vm_prefix(c: &Cluster) -> String {
    let custom = slugify(&c.vm_name_prefix, 30);
    let base = if custom.is_empty() {
        let auto = slugify(&c.name, 20);
        if auto.is_empty() {
            "cluster".to_string()
        } else {
            auto
        }
    } else {
        custom
    };
    let short: String = c.id.chars().take(8).collect();
    format!("{base}-{short}")
}

/// Huruf/angka kecil + strip, runtuh strip ganda, potong max.
fn slugify(s: &str, max: usize) -> String {
    s.to_lowercase()
        .chars()
        .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '-' })
        .collect::<String>()
        .split('-')
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("-")
        .chars()
        .take(max)
        .collect()
}

/// Playbook ansible sesuai fitur cluster + jumlah worker.
/// Dipakai log deploy dan file `run-ansible.sh` per cluster, supaya "isi VM"
/// selalu konsisten dengan pilihan fitur (k8s / nginx / nodejs).
/// `common` selalu pertama (paket dasar). Docker tidak ada di daftar karena
/// role `common` sudah menginstalnya di semua VM.
fn ansible_steps(c: &Cluster) -> Vec<(&'static str, &'static str)> {
    let has = |f: &str| c.enabled_features.iter().any(|x| x == f);
    let mut steps = vec![("playbook-common.yml", "base: apt update + paket dasar di SEMUA vm")];
    if has("k8s") {
        steps.push(("playbook-master.yml", "master: init Kubernetes"));
        if c.worker_count > 0 {
            steps.push(("playbook-workers.yml", "worker: join cluster"));
        }
    }
    if has("nodejs") {
        steps.push(("playbook-nodejs.yml", "semua vm: Node.js LTS + npm + pm2"));
    }
    if has("nginx") {
        steps.push(("playbook-nginx.yml", "master-0: Nginx landing page"));
    }
    steps
}

/// Build inventory.ini: pakai IP asli dari DB kalau ada, else placeholder.
/// Ini yang bikin "IP tidak tersimpan masih default bawaan" tidak terjadi lagi
/// setelah deploy sukses — file selalu ditulis ulang dengan IP hasil
/// `terraform output`.
fn build_inventory(c: &Cluster) -> String {
    let has_ips = !c.master_ips.is_empty() || !c.worker_ips.is_empty();
    let mut inv = format!(
        "# Generated by proxpilot for cluster '{}' ({})\n# {}\n#   master-0 ansible_host=192.168.1.50 ansible_user={}\n\n[k8s_master]\n",
        c.name,
        c.id,
        if has_ips {
            "IPs below are REAL (from `terraform output`, stored in DB)."
        } else {
            "DHCP IPs unknown yet — placeholders. Deploy / Refresh IPs to fill from `terraform output`."
        },
        c.ssh_user,
    );
    for i in 0..c.master_count.max(1) {
        let host = c
            .master_ips
            .get(i as usize)
            .filter(|s| !s.trim().is_empty())
            .map(|s| s.trim().to_string())
            .unwrap_or_else(|| format!("master-{i}"));
        inv.push_str(&format!("master-{i} ansible_host={host} ansible_user={}\n", c.ssh_user));
    }
    inv.push_str("\n[k8s_workers]\n");
    for i in 0..c.worker_count.max(0) {
        let host = c
            .worker_ips
            .get(i as usize)
            .filter(|s| !s.trim().is_empty())
            .map(|s| s.trim().to_string())
            .unwrap_or_else(|| format!("worker-{i}"));
        inv.push_str(&format!("worker-{i} ansible_host={host} ansible_user={}\n", c.ssh_user));
    }
    inv.push_str("\n[nginx_group]\nmaster-0\n\n[k8s_cluster:children]\nk8s_master\nk8s_workers\n");
    inv
}

/// Simpan IP hasil terraform ke DB + tulis ulang inventory.ini.
/// Returns (masters, workers) yang tersimpan.
fn save_discovered_ips(state: &AppState, id: &str, masters: Vec<String>, workers: Vec<String>) -> (Vec<String>, Vec<String>) {
    if let Some(mut c) = state.db.get_cluster(id) {
        c.master_ips = masters.clone();
        c.worker_ips = workers.clone();
        c.updated_at = chrono::Utc::now();
        state.db.save_cluster(&c);
        let inv_path = format!("{}/terraform/{}/inventory.ini", state.infra_dir, c.id);
        if let Err(e) = std::fs::write(&inv_path, build_inventory(&c)) {
            state.push_log(id, "provision", &format!("WARNING: IPs saved to DB but inventory rewrite failed ({e})"), "warning");
        }
    }
    (masters, workers)
}

/// Ambil `terraform output -json` (lokal/WSL/remote) lalu simpan ke DB.
/// Dipanggil setelah apply sukses DAN oleh endpoint refresh-ips.
async fn fetch_and_save_ips(state: &AppState, id: &str, cluster: &Cluster) -> (Vec<String>, Vec<String>) {
    let use_remote = !cluster.ssh_host.trim().is_empty();
    let via_wsl = tokio::task::spawn_blocking(exec::wsl_available).await.unwrap_or(false);
    let raw: Option<String> = if use_remote {
        let (h, u, p, cid) = (
            cluster.ssh_host.clone(),
            cluster.ssh_remote_user_or_default(),
            cluster.ssh_port,
            cluster.id.clone(),
        );
        tokio::task::spawn_blocking(move || terraform_output_remote(&h, &u, p, &cid, via_wsl))
            .await
            .unwrap_or(None)
    } else {
        let dir = format!("{}/terraform/{}", state.infra_dir, cluster.id);
        tokio::task::spawn_blocking(move || terraform_output_local(&dir, via_wsl))
            .await
            .unwrap_or(None)
    };
    let raw = match raw {
        Some(s) if !s.trim().is_empty() => s,
        _ => return (vec![], vec![]),
    };
    let (masters, workers) = parse_terraform_outputs(&raw);
    if masters.is_empty() && workers.is_empty() {
        return (vec![], vec![]);
    }
    // Pad/truncate ke requested counts agar index inventory stabil.
    let mut m = masters;
    let mut w = workers;
    m.truncate(cluster.master_count.max(1) as usize);
    w.truncate(cluster.worker_count.max(0) as usize);
    save_discovered_ips(state, id, m.clone(), w.clone());
    (m, w)
}

/// POST /api/clusters/:id/refresh-ips — baca ulang `terraform output`
/// tanpa deploy ulang. Berguna saat DHCP/agent lambat: apply OK tapi IP
/// masih placeholder.
async fn refresh_ips(State(s): State<AppState>, Path(id): Path<String>) -> impl IntoResponse {
    let cluster = match s.db.get_cluster(&id) {
        Some(c) => c,
        None => return (StatusCode::NOT_FOUND, Json(json!({"error": "cluster not found"}))).into_response(),
    };
    let (masters, workers) = fetch_and_save_ips(&s, &id, &cluster).await;
    if masters.is_empty() && workers.is_empty() {
        return (
            StatusCode::OK,
            Json(json!({
                "ok": false,
                "master_ips": masters, "worker_ips": workers,
                "hint": "terraform output kosong. Pastikan apply pernah sukses + qemu-guest-agent aktif di VM (butuh 1-3 mnt setelah boot).",
            })),
        )
            .into_response();
    }
    s.push_log(&id, "provision", &format!("Refresh IPs — masters: [{}] workers: [{}]", masters.join(", "), workers.join(", ")), "info");
    (StatusCode::OK, Json(json!({"ok": true, "master_ips": masters, "worker_ips": workers}))).into_response()
}

// ---------- live VMs (Proxmox API, filter prefix nama cluster) ----------

/// GET /api/clusters/:id/vms — VM milik cluster ini (live dari Proxmox).
async fn cluster_vms(State(s): State<AppState>, Path(id): Path<String>) -> impl IntoResponse {
    let cluster = match s.db.get_cluster(&id) {
        Some(c) => c,
        None => return (StatusCode::NOT_FOUND, Json(json!({"error": "cluster not found"}))).into_response(),
    };
    if cluster.token_secret.trim().is_empty() {
        return (StatusCode::BAD_REQUEST, Json(json!({"error": "cluster has no API token"}))).into_response();
    }
    let creds = cluster.into_creds();
    let prefix = vm_prefix(&cluster);
    match proxmox::list_vms(&creds, &cluster.target_node).await {
        Ok(all) => {
            let mine: Vec<_> = all
                .into_iter()
                .filter(|v| !v.template && v.name.starts_with(&prefix))
                .collect();
            (StatusCode::OK, Json(json!({"ok": true, "vms": mine}))).into_response()
        }
        Err(e) => (
            StatusCode::BAD_GATEWAY,
            Json(json!({"ok": false, "error": e})),
        )
            .into_response(),
    }
}

/// POST /api/clusters/:id/vms/:vmid/:action — start|shutdown|reboot|stop.
/// VMID harus milik cluster ini (cek prefix nama) agar tidak salah sasaran.
async fn vm_action(
    State(s): State<AppState>,
    Path((id, vmid_raw, action)): Path<(String, String, String)>,
) -> impl IntoResponse {
    const ALLOWED: &[&str] = &["start", "shutdown", "reboot", "stop"];
    if !ALLOWED.contains(&action.as_str()) {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "action must be one of: start, shutdown, reboot, stop"})),
        )
            .into_response();
    }
    let vmid: u64 = match vmid_raw.parse() {
        Ok(v) => v,
        Err(_) => return (StatusCode::BAD_REQUEST, Json(json!({"error": "invalid vmid"}))).into_response(),
    };
    let cluster = match s.db.get_cluster(&id) {
        Some(c) => c,
        None => return (StatusCode::NOT_FOUND, Json(json!({"error": "cluster not found"}))).into_response(),
    };
    let creds = cluster.into_creds();
    let prefix = vm_prefix(&cluster);
    // Resolve nama dulu: pastikan VMID memang milik cluster ini.
    let name = match proxmox::list_vms(&creds, &cluster.target_node).await {
        Ok(all) => match all.into_iter().find(|v| v.vmid == vmid) {
            Some(v) if !v.template && v.name.starts_with(&prefix) => v.name,
            _ => {
                return (
                    StatusCode::FORBIDDEN,
                    Json(json!({"error": "vmid is not part of this cluster"})),
                )
                    .into_response()
            }
        },
        Err(e) => {
            return (StatusCode::BAD_GATEWAY, Json(json!({"error": e}))).into_response();
        }
    };
    match proxmox::vm_power(&creds, &cluster.target_node, vmid, &action).await {
        Ok(task) => {
            s.push_log(&id, "power", &format!("{action} {name} (vmid {vmid}): task {task}"), "info");
            (StatusCode::OK, Json(json!({"ok": true, "task": task}))).into_response()
        }
        Err(e) => (
            StatusCode::BAD_GATEWAY,
            Json(json!({"error": e})),
        )
            .into_response(),
    }
}

// ---------- setup: koneksi Proxmox+SSH tersimpan SEKALI (sumber utama alur VM) ----------

fn setup_val(db: &crate::db::Db, key: &str) -> String {
    db.get_setting(key).unwrap_or_default()
}

/// Kredensial Proxmox dari Setup. None = belum diisi (isi di menu Setup dulu).
fn setup_creds(db: &crate::db::Db) -> Option<ProxmoxCreds> {
    let url = setup_val(db, "setup_proxmox_url");
    let secret = setup_val(db, "setup_token_secret");
    if url.trim().is_empty() || secret.trim().is_empty() {
        return None;
    }
    let user = setup_val(db, "setup_proxmox_user");
    Some(ProxmoxCreds {
        base_url: url,
        user: if user.trim().is_empty() { "root@pam".to_string() } else { user },
        token_id: setup_val(db, "setup_token_id"),
        token_secret: secret,
        verify_tls: setup_val(db, "setup_verify_tls") == "1",
    })
}

fn setup_node(db: &crate::db::Db) -> String {
    let n = setup_val(db, "setup_target_node");
    if n.trim().is_empty() { "pve".to_string() } else { n }
}

/// GET /api/setup — koneksi tersimpan (secret dimask).
async fn get_setup(State(s): State<AppState>) -> impl IntoResponse {
    let secret = setup_val(&s.db, "setup_token_secret");
    let port: u16 = setup_val(&s.db, "setup_ssh_port").parse().unwrap_or(22);
    Json(json!({
        "saved": setup_creds(&s.db).is_some(),
        "proxmox_url": setup_val(&s.db, "setup_proxmox_url"),
        "proxmox_user": setup_val(&s.db, "setup_proxmox_user"),
        "token_id": setup_val(&s.db, "setup_token_id"),
        "token_secret": if secret.is_empty() { "".to_string() } else { "******".to_string() },
        "has_token": !secret.is_empty(),
        "verify_tls": setup_val(&s.db, "setup_verify_tls") == "1",
        "target_node": setup_node(&s.db),
        "ssh_host": setup_val(&s.db, "setup_ssh_host"),
        "ssh_user": setup_val(&s.db, "setup_ssh_user"),
        "ssh_port": port,
    }))
}

/// PUT /api/setup — simpan koneksi. Secret kosong/"******" = pakai lama
/// (wajib ada bila belum pernah tersimpan).
async fn put_setup(State(s): State<AppState>, Json(b): Json<crate::models::SetupBody>) -> impl IntoResponse {
    if b.proxmox_url.trim().is_empty() {
        return (StatusCode::BAD_REQUEST, Json(json!({"error": "proxmox_url wajib diisi"}))).into_response();
    }
    let mut secret = b.token_secret.trim().to_string();
    if secret.is_empty() || secret == "******" {
        secret = setup_val(&s.db, "setup_token_secret");
    }
    if secret.is_empty() {
        return (StatusCode::BAD_REQUEST, Json(json!({"error": "token_secret wajib diisi (belum ada yang tersimpan)"}))).into_response();
    }
    s.db.set_setting("setup_proxmox_url", b.proxmox_url.trim());
    s.db.set_setting("setup_proxmox_user", if b.proxmox_user.trim().is_empty() { "root@pam" } else { b.proxmox_user.trim() });
    s.db.set_setting("setup_token_id", b.token_id.trim().trim_start_matches('!'));
    s.db.set_setting("setup_token_secret", &secret);
    s.db.set_setting("setup_verify_tls", if b.verify_tls { "1" } else { "0" });
    s.db.set_setting("setup_target_node", if b.target_node.trim().is_empty() { "pve" } else { b.target_node.trim() });
    s.db.set_setting("setup_ssh_host", b.ssh_host.trim());
    s.db.set_setting("setup_ssh_user", if b.ssh_user.trim().is_empty() { "root" } else { b.ssh_user.trim() });
    s.db.set_setting("setup_ssh_port", &b.ssh_port.to_string());
    (StatusCode::OK, Json(json!({"ok": true, "message": "koneksi tersimpan"}))).into_response()
}

// ---------- VMs: clone langsung via API (nama + VMID sesuai permintaan) ----------

fn err_msg(status: StatusCode, e: impl Into<String>) -> axum::response::Response {
    (status, Json(json!({"error": e.into()}))).into_response()
}

fn valid_vm_name(s: &str) -> bool {
    let b = s.as_bytes();
    if b.is_empty() || b.len() > 63 {
        return false;
    }
    let ok = |c: u8| c.is_ascii_alphanumeric() || c == b'-';
    b.iter().all(|&c| ok(c)) && b[0] != b'-' && *b.last().unwrap() != b'-'
}

fn valid_ipv4(s: &str) -> bool {
    let p: Vec<&str> = s.split('.').collect();
    p.len() == 4 && p.iter().all(|x| x.parse::<u8>().is_ok() && !x.is_empty())
}

/// GET /api/vms — semua VM QEMU (?node= override).
/// Mencoba Setup lalu cluster, dan tiap kredensial mencoba beberapa node
/// (override -> setup/cluster node -> live nodes) agar nama node yang salah
/// tidak membuat daftar VM kosong.
async fn list_all_vms(State(s): State<AppState>, Query(q): Query<NodesQuery>) -> impl IntoResponse {
    if candidate_creds(&s).is_empty() {
        return err_msg(StatusCode::BAD_REQUEST, "isi Setup dulu (simpan koneksi di Health) atau buat cluster dengan token API (koneksi belum tersimpan)");
    }
    match fetch_vms_any(&s, q.node).await {
        Some((vms, node, ok_nodes)) => {
            let templates: Vec<Value> = vms.iter().filter(|v| v.template).cloned().map(|v| json!({"name": v.name, "vmid": v.vmid})).collect();
            (StatusCode::OK, Json(json!({"ok": true, "node": node, "nodes": ok_nodes, "vms": vms, "templates": templates}))).into_response()
        }
        None => err_msg(StatusCode::BAD_GATEWAY, "Proxmox terhubung tapi list VM gagal di semua node — cek token/node/Jaringan"),
    }
}

/// GET /api/templates — template LIVE dari Proxmox; fallback daftar statis
/// bila Setup/API belum bisa.
/// Bentuk respon SELALU objek: {templates, live, node, source}
/// (`live=false` + `source=fallback` = 5 template statis, bukan dari Proxmox).
async fn list_templates(State(s): State<AppState>) -> Json<Value> {
    if let Some((all, node, ok_nodes)) = fetch_vms_any(&s, None).await {
        let live: Vec<Value> = all
            .iter()
            .filter(|v| v.template)
            .map(|v| json!({"name": v.name, "vmid": v.vmid, "description": format!("live di node {} (vmid {})", node, v.vmid)}))
            .collect();
        if !live.is_empty() {
            return Json(json!({"templates": live, "live": true, "node": node, "nodes": ok_nodes, "source": "live"}));
        }
        // Terhubung tapi tidak ada template — sertakan diagnosa agar jelas.
        let sample: Vec<String> = all.iter().take(10).map(|v| format!("{} (vmid {}, template={})", v.name, v.vmid, v.template)).collect();
        let diag = if all.is_empty() {
            format!("API mengembalikan 0 VM dari nodes {:?} (node primer {}). Artinya token nyambung tapi tidak melihat VM apa pun. Penyebab umum: (1) token dibuat dengan Privilege Separation ON / role terbatas — buat ulang token TANPA centang Privilege Separation atau beri role PVEAuditor+VM.Audit di / (Datacenter → Permissions); (2) VM/template memang belum ada di cluster ini — cek di Proxmox UI atau Shell `qm list`; (3) template berupa LXC/ISO, bukan QEMU template.", ok_nodes, node)
        } else {
            format!("Terhubung ke node {} tapi tidak ada template di sana (cek {} VM, semua flag template=false). Kemungkinan: (1) belum ada template di Proxmox — buat dari VM via Proxmox UI (klik VM > More > Convert to template); (2) template ada di node lain — cek nodes {:?}; (3) token tanpa hak baca template.", node, all.len(), ok_nodes)
        };
        return Json(json!({
            "templates": [],
            "live": true,
            "node": node,
            "nodes": ok_nodes,
            "source": "live-empty",
            "total_vms": all.len(),
            "sample": sample,
            "warning": diag,
        }));
    }
    Json(json!({
        "templates": static_templates(),
        "live": false,
        "node": setup_node(&s.db),
        "source": "fallback",
        "warning": "Proxmox belum terhubung — 5 template di bawah ini daftar statis (bukan live dari node). Isi Setup / Test API di Health agar daftar live muncul.",
    }))
}

/// POST /api/vms/clone — clone template -> VM isi data (cloud-init) -> start -> IP.
/// VM lahir KOSONG sesuai template; isi (ansible) langkah terpisah di Configure.
async fn clone_vm(State(s): State<AppState>, Json(b): Json<crate::models::CloneBody>) -> impl IntoResponse {
    let creds = setup_creds(&s.db)
        .or_else(|| s.first_creds());
    if creds.is_none() {
        return err_msg(StatusCode::BAD_REQUEST, "isi Setup dulu atau buat cluster dengan token API (koneksi belum tersimpan)");
    }
    let creds = creds.unwrap();
    let name = b.name.trim().to_string();
    if !valid_vm_name(&name) {
        return err_msg(
            StatusCode::BAD_REQUEST,
            "nama VM invalid (huruf/angka/strip, maks 63, tidak boleh diawali/diakhiri strip)",
        );
    }
    // 1. Resolve template di SEMUA node (bukan cuma setup_node).
    let (tpl, node) = match resolve_template_any(&s, b.template.trim()).await {
        Some(x) => x,
        None => {
            return err_msg(
                StatusCode::BAD_REQUEST,
                format!("template '{}' tidak ditemukan di node mana pun (harus template, bukan VM biasa). Cek daftar template live di atas / buat template dulu di Proxmox", b.template.trim()),
            )
        }
    };
    // Daftar VM di node template (untuk cek vmid bentrok).
    let all = proxmox::list_vms(&creds, &node).await.unwrap_or_default();
    // 2. VMID: sesuai permintaan, atau nextid otomatis.
    let vmid = match b.vmid {
        Some(v) => {
            if v < 100 || v > 999999999 {
                return err_msg(StatusCode::BAD_REQUEST, "vmid harus 100..999999999");
            }
            if all.iter().any(|x| x.vmid == v) {
                return err_msg(StatusCode::CONFLICT, format!("vmid {v} sudah dipakai"));
            }
            v
        }
        None => match proxmox::next_vmid(&creds).await {
            Ok(v) => v,
            Err(e) => return err_msg(StatusCode::BAD_GATEWAY, e),
        },
    };
    // 3. Clone (full default) + tunggu selesai.
    let upid = match proxmox::clone_vm(&creds, &node, tpl, vmid, &name, b.full, non_empty(&b.storage)).await {
        Ok(u) => u,
        Err(e) => return err_msg(StatusCode::BAD_GATEWAY, format!("clone gagal: {e}")),
    };
    match proxmox::wait_task(&creds, &node, &upid).await {
        Ok(true) => {}
        Ok(false) => return err_msg(StatusCode::GATEWAY_TIMEOUT, format!("clone timeout (task {upid}); cek Proxmox manual")),
        Err(e) => return err_msg(StatusCode::BAD_GATEWAY, format!("clone gagal: {e} (task {upid})")),
    }
    // 4. Cloud-init: user + key panel + IP (DHCP default) + DNS.
    let ciuser = if b.ciuser.trim().is_empty() { "ubuntu".to_string() } else { b.ciuser.trim().to_string() };
    let key = panel_ssh_key().await;
    let ipconfig = if b.static_ip.trim().is_empty() {
        "ip=dhcp".to_string()
    } else {
        if !valid_ipv4(b.static_ip.trim()) {
            return err_msg(StatusCode::BAD_REQUEST, format!("static_ip invalid, VM {name} ({vmid}) sudah ter-clone — hapus/perbaiki manual"));
        }
        let gw = if b.gateway.trim().is_empty() {
            format!("{}.1", b.static_ip.trim().rsplit_once('.').map(|x| x.0).unwrap_or(""))
        } else {
            b.gateway.trim().to_string()
        };
        format!("ip={}/24,gw={}", b.static_ip.trim(), gw)
    };
    let ns = if b.nameserver.trim().is_empty() { "8.8.8.8".to_string() } else { b.nameserver.trim().to_string() };
    if let Err(e) = proxmox::set_vm_config(&creds, &node, vmid, &ciuser, key.as_deref(), &ipconfig, &ns).await {
        return err_msg(
            StatusCode::BAD_GATEWAY,
            format!("VM {name} ({vmid}) ter-clone TAPI config gagal: {e} — perbaiki via Proxmox UI"),
        );
    }
    // 5. Start (default) + deteksi IP via agent (best-effort, DHCP butuh waktu).
    if b.start {
        if let Err(e) = proxmox::vm_power(&creds, &node, vmid, "start").await {
            return err_msg(
                StatusCode::BAD_GATEWAY,
                format!("VM {name} ({vmid}) ter-clone TAPI start gagal: {e} — start manual di Proxmox"),
            );
        }
    }
    let mut ip: Option<String> = None;
    for _ in 0..8 {
        match proxmox::vm_ip(&creds, &node, vmid).await {
            Ok(Some(found)) => {
                ip = Some(found);
                break;
            }
            _ => tokio::time::sleep(std::time::Duration::from_secs(5)).await,
        }
    }
    (
        StatusCode::CREATED,
        Json(json!({
            "ok": true, "vmid": vmid, "name": name, "ip": ip,
            "hint": if ip.is_none() { "IP belum terbaca (agent/DHCP lambat) — cek GET /api/vms/:vmid/ip beberapa saat lagi" } else { "" },
        })),
    )
        .into_response()
}

fn non_empty(s: &str) -> Option<&str> {
    let t = s.trim();
    if t.is_empty() { None } else { Some(t) }
}

/// DELETE /api/vms/:vmid — hapus VM (harus stopped dulu).
async fn delete_vm(State(s): State<AppState>, Path(vmid_raw): Path<String>) -> impl IntoResponse {
    let creds = setup_creds(&s.db)
        .or_else(|| s.first_creds());
    if creds.is_none() {
        return err_msg(StatusCode::BAD_REQUEST, "isi Setup dulu atau buat cluster dengan token API (koneksi belum tersimpan)");
    }
    let creds = creds.unwrap();
    let node = setup_node(&s.db);
    let vmid: u64 = match vmid_raw.parse() {
        Ok(v) => v,
        Err(_) => return err_msg(StatusCode::BAD_REQUEST, "invalid vmid"),
    };
    let all = match proxmox::list_vms(&creds, &node).await {
        Ok(v) => v,
        Err(e) => return err_msg(StatusCode::BAD_GATEWAY, e),
    };
    match all.into_iter().find(|v| v.vmid == vmid) {
        None => return err_msg(StatusCode::NOT_FOUND, format!("vmid {vmid} tidak ada di node {node}")),
        Some(v) if v.template => return err_msg(StatusCode::BAD_REQUEST, "itu TEMPLATE — hapus manual di Proxmox bila yakin"),
        Some(v) if v.status == "running" => {
            return err_msg(StatusCode::BAD_REQUEST, "VM running — shutdown/stop dulu")
        }
        _ => {}
    }
    match proxmox::delete_vm(&creds, &node, vmid).await {
        Ok(()) => (StatusCode::OK, Json(json!({"ok": true}))).into_response(),
        Err(e) => err_msg(StatusCode::BAD_GATEWAY, e),
    }
}

/// GET /api/vms/:vmid/ip — IP via guest-agent (null bila belum ada).
async fn vm_ip_addr(State(s): State<AppState>, Path(vmid_raw): Path<String>) -> impl IntoResponse {
    let creds = setup_creds(&s.db)
        .or_else(|| s.first_creds());
    if creds.is_none() {
        return err_msg(StatusCode::BAD_REQUEST, "isi Setup dulu atau buat cluster dengan token API (koneksi belum tersimpan)");
    }
    let creds = creds.unwrap();
    let node = setup_node(&s.db);
    let vmid: u64 = match vmid_raw.parse() {
        Ok(v) => v,
        Err(_) => return err_msg(StatusCode::BAD_REQUEST, "invalid vmid"),
    };
    match proxmox::vm_ip(&creds, &node, vmid).await {
        Ok(ip) => (
            StatusCode::OK,
            Json(json!({"ok": true, "vmid": vmid, "ip": ip,
                "hint": if ip.is_none() { "agent belum jawab (VM mati / agent belum aktif / baru boot)" } else { "" }})),
        )
            .into_response(),
        Err(e) => err_msg(StatusCode::BAD_GATEWAY, format!("{e} (agent belum aktif?)")),
    }
}

/// POST /api/vms/:vmid/:action — power VM pilihan (start|shutdown|reboot|stop).
async fn vm_power(
    State(s): State<AppState>,
    Path((vmid_raw, action)): Path<(String, String)>,
) -> impl IntoResponse {
    const ALLOWED: &[&str] = &["start", "shutdown", "reboot", "stop"];
    if !ALLOWED.contains(&action.as_str()) {
        return err_msg(StatusCode::BAD_REQUEST, "action harus: start, shutdown, reboot, stop");
    }
    let creds = setup_creds(&s.db)
        .or_else(|| s.first_creds());
    if creds.is_none() {
        return err_msg(StatusCode::BAD_REQUEST, "isi Setup dulu atau buat cluster dengan token API (koneksi belum tersimpan)");
    }
    let creds = creds.unwrap();
    let node = setup_node(&s.db);
    let vmid: u64 = match vmid_raw.parse() {
        Ok(v) => v,
        Err(_) => return err_msg(StatusCode::BAD_REQUEST, "invalid vmid"),
    };
    let name = match proxmox::list_vms(&creds, &node).await {
        Ok(all) => match all.into_iter().find(|v| v.vmid == vmid && !v.template) {
            Some(v) => v.name,
            None => return err_msg(StatusCode::NOT_FOUND, format!("vmid {vmid} tidak ada (atau itu template)")),
        },
        Err(e) => return err_msg(StatusCode::BAD_GATEWAY, e),
    };
    match proxmox::vm_power(&creds, &node, vmid, &action).await {
        Ok(task) => (
            StatusCode::OK,
            Json(json!({"ok": true, "name": name, "task": task})),
        )
            .into_response(),
        Err(e) => err_msg(StatusCode::BAD_GATEWAY, e),
    }
}

// ---------- configure: pilih VM -> tanam template (ansible, async) ----------

/// Template konfigurasi: id -> (playbook, grup inventory, deskripsi).
/// Playbook k8s/nginx memakai grup bawaannya; redis/postgres/nodejs pakai
/// grup `configured` (lihat playbook masing-masing).
fn config_template(id: &str) -> Option<(&'static str, &'static str, &'static str)> {
    match id {
        "k8s-master" => Some(("playbook-master.yml", "k8s_master", "Kubernetes master (init + Calico)")),
        "k8s-worker" => Some(("playbook-workers.yml", "k8s_workers", "Kubernetes worker (join)")),
        "redis" => Some(("playbook-redis.yml", "configured", "Redis server")),
        "postgres" => Some(("playbook-postgres.yml", "configured", "PostgreSQL server")),
        "nodejs" => Some(("playbook-nodejs.yml", "configured", "Node.js LTS + pm2")),
        "nginx" => Some(("playbook-nginx.yml", "nginx_group", "Nginx landing page")),
        _ => None,
    }
}

/// GET /api/configure/templates — daftar template tanam.
async fn config_templates() -> Json<Value> {
    Json(json!([
        {"id": "k8s-master", "playbook": "playbook-master.yml", "description": "Kubernetes master (init + Calico)"},
        {"id": "k8s-worker", "playbook": "playbook-workers.yml", "description": "Kubernetes worker (join)"},
        {"id": "redis", "playbook": "playbook-redis.yml", "description": "Redis server"},
        {"id": "postgres", "playbook": "playbook-postgres.yml", "description": "PostgreSQL server"},
        {"id": "nodejs", "playbook": "playbook-nodejs.yml", "description": "Node.js LTS + pm2"},
        {"id": "nginx", "playbook": "playbook-nginx.yml", "description": "Nginx landing page"},
    ]))
}

/// Inventory untuk tanam: semua target di [configured] + grup template.
fn build_configure_inventory(run_id: &str, template: &str, hosts: &[(String, String, String)]) -> String {
    let group = config_template(template).map(|(_, g, _)| g).unwrap_or("configured");
    let mut inv = format!("# Generated by proxpilot configure {run_id} (template {template})\n[configured]\n");
    for (alias, ip, user) in hosts {
        inv.push_str(&format!("{alias} ansible_host={ip} ansible_user={user}\n"));
    }
    if group != "configured" {
        inv.push_str(&format!("\n[{group}]\n"));
        for (alias, _, _) in hosts {
            inv.push_str(&format!("{alias}\n"));
        }
    }
    inv.push_str("\n[all:vars]\nansible_ssh_common_args=-o StrictHostKeyChecking=accept-new -o ConnectTimeout=10\n");
    inv
}

/// POST /api/configure — tanam template ke VM terpilih (async, poll via runs/:id).
/// Syarat: ansible ada di host panel (WSL/native), folder ansible ketemu.
async fn configure_run(State(s): State<AppState>, Json(b): Json<crate::models::ConfigureBody>) -> impl IntoResponse {
    let (playbook, _group, _desc) = match config_template(b.template.trim()) {
        Some(t) => t,
        None => return err_msg(StatusCode::BAD_REQUEST, "template tidak dikenal"),
    };
    if b.vmids.is_empty() {
        return err_msg(StatusCode::BAD_REQUEST, "pilih minimal 1 VM");
    }
    let creds = setup_creds(&s.db)
        .or_else(|| s.first_creds());
    if creds.is_none() {
        return err_msg(StatusCode::BAD_REQUEST, "isi Setup dulu atau buat cluster dengan token API (koneksi belum tersimpan)");
    }
    let creds = creds.unwrap();
    let node = setup_node(&s.db);
    // ansible harus ada di host panel (dieksekusi lokal/WSL, bukan remote).
    let via_wsl = tokio::task::spawn_blocking(exec::wsl_available).await.unwrap_or(false);
    let ansible_ok = tokio::task::spawn_blocking(move || {
        if via_wsl { exec::tool_version_wsl("ansible") } else { exec::tool_version_local("ansible") }
    })
    .await
    .map(|r| r.ok)
    .unwrap_or(false);
    if !ansible_ok {
        return err_msg(
            StatusCode::BAD_REQUEST,
            "ansible tidak ada di host panel (install di WSL: sudo apt install ansible)",
        );
    }
    let pb_path = format!("{}/{playbook}", s.ansible_dir);
    if !std::path::Path::new(&pb_path).exists() {
        return err_msg(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("playbook tidak ketemu di {} (set PANEL_ANSIBLE bila repo di path lain)", s.ansible_dir),
        );
    }
    // Resolve VM -> (alias, ip, user). IP wajib ada (agent).
    let ssh_user = if b.ssh_user.trim().is_empty() { "ubuntu".to_string() } else { b.ssh_user.trim().to_string() };
    let all = match proxmox::list_vms(&creds, &node).await {
        Ok(v) => v,
        Err(e) => return err_msg(StatusCode::BAD_GATEWAY, e),
    };
    let mut hosts: Vec<(String, String, String)> = vec![];
    let mut names: Vec<String> = vec![];
    for vmid in &b.vmids {
        let vm = match all.iter().find(|v| v.vmid == *vmid && !v.template) {
            Some(v) => v,
            None => return err_msg(StatusCode::NOT_FOUND, format!("vmid {vmid} tidak ada (atau itu template)")),
        };
        if vm.status != "running" {
            return err_msg(StatusCode::BAD_REQUEST, format!("{} ({vmid}) sedang {} — start dulu", vm.name, vm.status));
        }
        let ip = match proxmox::vm_ip(&creds, &node, *vmid).await {
            Ok(Some(ip)) => ip,
            _ => {
                return err_msg(
                    StatusCode::BAD_REQUEST,
                    format!("{} ({vmid}) belum ada IP (tunggu guest-agent, lalu ulangi)", vm.name),
                )
            }
        };
        names.push(vm.name.clone());
        hosts.push((vm.name.clone(), ip, ssh_user.clone()));
    }
    // Tulis inventory run (folder = id run, konsisten).
    let rid = s.create_config_run(b.template.trim(), names);
    let dir = format!("{}/configure/{rid}", s.infra_dir);
    if std::fs::create_dir_all(&dir).is_err() {
        return err_msg(StatusCode::INTERNAL_SERVER_ERROR, "gagal siapkan folder configure");
    }
    let inv = build_configure_inventory(&rid, b.template.trim(), &hosts);
    let inv_path = format!("{dir}/inventory.ini");
    if std::fs::write(&inv_path, &inv).is_err() {
        return err_msg(StatusCode::INTERNAL_SERVER_ERROR, "gagal tulis inventory configure");
    }
    let bg = s.clone();
    let rid_clone = rid.clone();
    tokio::spawn(async move {
        bg.push_config_log(&rid_clone, &format!("target: {inv_path}\nplaybook: {pb_path}\n\n"));
        // Panel di Windows (tanpa ansible native) -> lewat WSL bridge.
        let (ok, txt): (bool, String) = tokio::task::spawn_blocking(move || {
            if via_wsl {
                let q = |p: &str| format!("\"{}\"", p.replace('\\', "\\\\").replace('"', "\\\""));
                let cmd = format!("ansible-playbook -i {} {} 2>&1", q(&win_to_wsl(&inv_path)), q(&win_to_wsl(&pb_path)));
                let r = exec::run_in_wsl(&cmd);
                (r.ok, r.output)
            } else {
                match std::process::Command::new("ansible-playbook").args(["-i", &inv_path, &pb_path]).output() {
                    Ok(o) => (
                        o.status.success(),
                        format!(
                            "{}\n[stderr]\n{}",
                            String::from_utf8_lossy(&o.stdout),
                            String::from_utf8_lossy(&o.stderr)
                        ),
                    ),
                    Err(e) => (false, format!("failed to spawn ansible-playbook: {e}")),
                }
            }
        })
        .await
        .unwrap_or((false, "task join failed".to_string()));
        bg.push_config_log(&rid_clone, &truncate(&txt, 20000));
        bg.finish_config_run(&rid_clone, if ok { "done" } else { "failed" });
    });
    (StatusCode::ACCEPTED, Json(json!({"ok": true, "run_id": rid}))).into_response()
}

/// GET /api/configure/runs/:id — status + output run (polling).
async fn configure_status(State(s): State<AppState>, Path(id): Path<String>) -> impl IntoResponse {
    match s.get_config_run(&id) {
        Some(r) => (
            StatusCode::OK,
            Json(json!({
                "ok": true, "id": r.id, "template": r.template, "vms": r.vms,
                "status": r.status, "output": r.output,
            })),
        )
            .into_response(),
        None => err_msg(StatusCode::NOT_FOUND, "run tidak ditemukan"),
    }
}

/// `terraform output -json` di host lokal (native) atau via WSL bridge.
fn terraform_output_local(dir: &str, via_wsl: bool) -> Option<String> {
    if via_wsl {
        let dir_wsl = win_to_wsl(dir);
        let cmd = format!("cd \"{}\" && terraform output -json 2>/dev/null", dir_wsl.replace('"', "\\\""));
        let r = exec::run_in_wsl(&cmd);
        let s = r.output.trim().to_string();
        if s.starts_with('{') {
            return Some(extract_json_object(&s));
        }
        return None;
    }
    let out = std::process::Command::new("terraform")
        .args(["output", "-json"])
        .current_dir(dir)
        .output()
        .ok()?;
    let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if s.starts_with('{') {
        Some(s)
    } else {
        None
    }
}

/// `terraform output -json` di server Proxmox via ssh (remote mode).
fn terraform_output_remote(host: &str, user: &str, port: u16, cluster_id: &str, via_wsl: bool) -> Option<String> {
    let remote_dir = format!("/tmp/proxpilot/{cluster_id}");
    let cmd = format!("cd '{remote_dir}' && terraform output -json 2>/dev/null");
    let r = exec::ssh_exec_auto(host, user, port, &cmd, via_wsl);
    let s = r.output.trim().to_string();
    if s.contains('{') {
        Some(extract_json_object(&s))
    } else {
        None
    }
}

/// Ambil substring objek JSON terluar (buang noise ssh/warning di sekitarnya).
fn extract_json_object(s: &str) -> String {
    match (s.find('{'), s.rfind('}')) {
        (Some(a), Some(b)) if b > a => s[a..=b].to_string(),
        _ => s.to_string(),
    }
}

/// Parse `terraform output -json` ke (master_ips, worker_ips).
/// Mendukung format baru (master_ips/worker_ips array) + legacy manual
/// (master_ip/worker1_ip/worker2_ip string).
fn parse_terraform_outputs(s: &str) -> (Vec<String>, Vec<String>) {
    let v: Value = match serde_json::from_str(&extract_json_object(s)) {
        Ok(v) => v,
        Err(_) => return (vec![], vec![]),
    };
    let obj = match v.as_object() {
        Some(o) => o,
        None => return (vec![], vec![]),
    };
    let mut masters = extract_ip_list(obj.get("master_ips"));
    let mut workers = extract_ip_list(obj.get("worker_ips"));
    // Legacy fallback: master_ip / worker1_ip / worker2_ip / worker_ips lama.
    if masters.is_empty() {
        masters = extract_ip_list(obj.get("master_ip"));
    }
    if workers.is_empty() {
        let mut w = vec![];
        for key in ["worker1_ip", "worker2_ip", "worker3_ip", "worker4_ip"] {
            w.extend(extract_ip_list(obj.get(key)));
        }
        if !w.is_empty() {
            workers = w;
        }
    }
    // Bersihkan placeholder qemu-agent ("", "-", "unknown", "none").
    masters.retain(|ip| is_real_ip(ip));
    workers.retain(|ip| is_real_ip(ip));
    (masters, workers)
}

/// Ambil list IP dari satu output terraform: {value: [...]/"..." } atau langsung [...]/"...".
fn extract_ip_list(v: Option<&Value>) -> Vec<String> {
    let v = match v {
        Some(v) => v,
        None => return vec![],
    };
    let inner = v.get("value").unwrap_or(v);
    match inner {
        Value::Array(arr) => arr.iter().filter_map(|x| x.as_str().map(|s| s.trim().to_string())).filter(|s| !s.is_empty()).collect(),
        Value::String(s) => {
            let t = s.trim().to_string();
            if t.is_empty() { vec![] } else { vec![t] }
        }
        _ => vec![],
    }
}

fn is_real_ip(s: &str) -> bool {
    let t = s.trim().to_lowercase();
    !(t.is_empty() || t == "-" || t == "unknown" || t == "none" || t == "null" || t == "n/a" || t == "0.0.0.0")
}

fn hcl_escape(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

/// Nilai string HCL atau `null` (null = argumen dianggap tidak diset).
fn hcl_str_or_null(v: Option<&str>) -> String {
    match v.map(str::trim).filter(|s| !s.is_empty()) {
        Some(s) => format!("\"{}\"", hcl_escape(s)),
        None => "null".to_string(),
    }
}

/// User cloud-init VM (fallback "ubuntu" untuk DB lama yang kosong).
fn vm_ssh_user(c: &Cluster) -> &str {
    if c.ssh_user.trim().is_empty() {
        "ubuntu"
    } else {
        c.ssh_user.trim()
    }
}

fn terraform_main_tf() -> &'static str {
    r#"# Generated by proxpilot (Rust). Credentials come from the
# generated terraform.tfvars (mode 0600) next to this file.
terraform {
  required_providers {
    proxmox = {
      source  = "telmate/proxmox"
      version = ">= 2.9.0"
    }
  }
}
variable "proxmox_api_url" { type = string }
variable "proxmox_user" { type = string }
variable "proxmox_token" {
  type      = string
  sensitive = true
}
variable "target_node" { type = string }
variable "clone_template" { type = string }
variable "network_bridge" { type = string }
variable "gateway" { type = string }
variable "dns1" { type = string }
variable "ssh_user" { type = string }
variable "master_count" { type = number }
variable "worker_count" { type = number }
variable "master_cpu" { type = number }
variable "master_ram" { type = number }
variable "worker_cpu" { type = number }
variable "worker_ram" { type = number }
variable "name_prefix" { type = string }
variable "ciuser" {
  type    = string
  default = null
}
variable "sshkeys" {
  type    = string
  default = null
}
variable "disk_size_gb" {
  type    = number
  default = 0
}
variable "disk_storage" {
  type    = string
  default = "local-lvm"
}
variable "vlan_tag" {
  type    = number
  default = -1
}
variable "master_ipconfigs" {
  type    = list(string)
  default = []
}
variable "worker_ipconfigs" {
  type    = list(string)
  default = []
}

provider "proxmox" {
  pm_api_url          = var.proxmox_api_url
  pm_user             = split(":", var.proxmox_user)[0]
  pm_api_token_id     = split(":", var.proxmox_user)[1]
  pm_api_token_secret = var.proxmox_token
  pm_tls_insecure     = true
}

resource "proxmox_vm_qemu" "master" {
  count       = var.master_count
  name        = "${var.name_prefix}-master-${count.index}"
  target_node = var.target_node
  clone       = var.clone_template
  cores       = var.master_cpu
  sockets     = 1
  cpu         = "host"
  memory      = var.master_ram
  agent       = 1
  os_type     = "cloud-init"
  scsihw      = "virtio-scsi-single"
  network {
    bridge   = var.network_bridge
    firewall = false
    model    = "virtio"
    tag      = var.vlan_tag
  }
  ipconfig0  = length(var.master_ipconfigs) > count.index ? var.master_ipconfigs[count.index] : "ip=dhcp"
  nameserver = var.dns1
  dynamic "disk" {
    for_each = var.disk_size_gb > 0 ? [1] : []
    content {
      type    = "scsi"
      storage = var.disk_storage
      size    = "${var.disk_size_gb}G"
    }
  }
  ciuser   = var.ciuser
  sshkeys  = var.sshkeys
  ssh_user = var.ssh_user
  oncreate = true
  onboot   = true
  lifecycle { ignore_changes = [network] }
}

resource "proxmox_vm_qemu" "worker" {
  count       = var.worker_count
  name        = "${var.name_prefix}-worker-${count.index}"
  target_node = var.target_node
  clone       = var.clone_template
  cores       = var.worker_cpu
  sockets     = 1
  cpu         = "host"
  memory      = var.worker_ram
  agent       = 1
  os_type     = "cloud-init"
  scsihw      = "virtio-scsi-single"
  network {
    bridge   = var.network_bridge
    firewall = false
    model    = "virtio"
    tag      = var.vlan_tag
  }
  ipconfig0  = length(var.worker_ipconfigs) > count.index ? var.worker_ipconfigs[count.index] : "ip=dhcp"
  nameserver = var.dns1
  dynamic "disk" {
    for_each = var.disk_size_gb > 0 ? [1] : []
    content {
      type    = "scsi"
      storage = var.disk_storage
      size    = "${var.disk_size_gb}G"
    }
  }
  ciuser   = var.ciuser
  sshkeys  = var.sshkeys
  ssh_user = var.ssh_user
  oncreate = true
  onboot   = true
  lifecycle { ignore_changes = [network] }
}

output "master_ips" {
  description = "IPs of master VMs (static when ipconfigs given, else DHCP via guest-agent)"
  value       = proxmox_vm_qemu.master[*].default_ipv4_address
}

output "worker_ips" {
  description = "DHCP IPs of worker VMs (fill into inventory.ini)"
  value       = proxmox_vm_qemu.worker[*].default_ipv4_address
}
"#
}

fn win_to_wsl(p: &str) -> String {
    // D:\x\y -> /mnt/d/x/y ; already-unix paths pass through.
    let t = p.replace('\\', "/");
    if t.len() >= 2 && t.chars().nth(1) == Some(':') {
        let drive = t.chars().next().unwrap().to_ascii_lowercase();
        return format!("/mnt/{}/{}", drive, t[2..].trim_start_matches('/'));
    }
    t
}

fn truncate(s: &str, n: usize) -> String {
    if s.len() <= n {
        s.to_string()
    } else {
        format!("{}...[truncated]", &s[..n])
    }
}

// helper trait for default remote user
trait ClusterExt {
    fn ssh_remote_user_or_default(&self) -> String;
}
impl ClusterExt for Cluster {
    fn ssh_remote_user_or_default(&self) -> String {
        if self.ssh_remote_user.trim().is_empty() {
            "root".to_string()
        } else {
            self.ssh_remote_user.clone()
        }
    }
}

// Shim for unwrap_or on JoinHandle error path (never actually constructed).
#[allow(dead_code)]
fn _unused(_m: &HashMap<String, String>) {
    let _t = TemplateOption {
        name: String::new(),
        storage: String::new(),
        size: String::new(),
        description: String::new(),
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    /// tfvars ditulis rata ala `terraform fmt` — normalisasi whitespace
    /// sebelum assert agar tidak bergantung jumlah spasi.
    fn norm_ws(s: &str) -> String {
        s.split_whitespace().collect::<Vec<_>>().join(" ")
    }

    #[test]
    fn wsl_path_conversion() {
        assert_eq!(win_to_wsl("D:\\a\\b"), "/mnt/d/a/b");
        assert_eq!(win_to_wsl("/tmp/x"), "/tmp/x");
    }

    #[test]
    fn hcl_escapes_quotes_and_backslashes() {
        assert_eq!(hcl_escape("a\"b\\c"), "a\\\"b\\\\c");
    }

    #[test]
    fn filegen_writes_real_secret_and_sized_inventory() {
        let base = std::env::temp_dir().join(format!("pp-test-{}", uuid::Uuid::new_v4()));
        let infra = base.to_string_lossy().to_string();
        let payload: Cluster = serde_json::from_value(serde_json::json!({
            "name": "t",
            "token_secret": "s3cr\"et",
            "master_count": 2,
            "worker_count": 3,
        }))
        .unwrap();
        let c = Cluster::new(payload);
        write_terraform(&infra, &c, None).unwrap();

        let tfvars = std::fs::read_to_string(format!("{infra}/terraform/{}/terraform.tfvars", c.id)).unwrap();
        let tfvars = norm_ws(&tfvars);
        assert!(tfvars.contains("proxmox_token = \"s3cr\\\"et\""), "{tfvars}");
        assert!(!tfvars.contains("redacted"));

        let inv = std::fs::read_to_string(format!("{infra}/terraform/{}/inventory.ini", c.id)).unwrap();
        let entries = inv
            .lines()
            .filter(|l| !l.trim_start().starts_with('#') && l.contains("ansible_host="))
            .count();
        assert_eq!(entries, 5, "{inv}");
        assert!(inv.contains("master-1"));
        assert!(inv.contains("worker-2"));

        let script = std::fs::read_to_string(format!("{infra}/terraform/{}/deploy-remote.sh", c.id)).unwrap();
        assert!(script.contains("terraform apply -auto-approve"));

        std::fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn parse_output_new_and_legacy_formats() {
        let (m, w) = parse_terraform_outputs(
            r#"{"master_ips":{"value":["10.0.0.1"],"type":["tuple",["string"]]},"worker_ips":{"value":["10.0.0.2","10.0.0.3"],"type":["tuple",["string","string"]]}}"#,
        );
        assert_eq!(m, vec!["10.0.0.1"]);
        assert_eq!(w, vec!["10.0.0.2", "10.0.0.3"]);

        let (m2, w2) = parse_terraform_outputs(
            r#"{"master_ip":{"value":"192.168.1.10","type":"string"},"worker1_ip":{"value":"192.168.1.11"},"worker2_ip":{"value":"192.168.1.12"}}"#,
        );
        assert_eq!(m2, vec!["192.168.1.10"]);
        assert_eq!(w2, vec!["192.168.1.11", "192.168.1.12"]);

        // placeholder agent kosong harus dibuang
        let (m3, w3) = parse_terraform_outputs(r#"{"master_ips":{"value":[""],"type":"tuple"},"worker_ips":{"value":["unknown"]}}"#);
        assert!(m3.is_empty());
        assert!(w3.is_empty());
    }

    #[test]
    fn inventory_uses_real_ips_when_known() {
        let mut payload: Cluster = serde_json::from_value(serde_json::json!({
            "name": "t",
            "token_secret": "x",
            "master_count": 1,
            "worker_count": 2,
        }))
        .unwrap();
        payload = Cluster::new(payload);
        // tanpa IP -> placeholder
        let inv0 = build_inventory(&payload);
        assert!(inv0.contains("ansible_host=master-0"));
        assert!(inv0.contains("placeholders"));

        // dengan IP -> ansible pakai IP asli, bukan default bawaan
        payload.master_ips = vec!["192.168.1.50".to_string()];
        payload.worker_ips = vec!["192.168.1.51".to_string(), "192.168.1.52".to_string()];
        let inv1 = build_inventory(&payload);
        assert!(inv1.contains("ansible_host=192.168.1.50"), "{inv1}");
        assert!(inv1.contains("ansible_host=192.168.1.51"), "{inv1}");
        assert!(inv1.contains("ansible_host=192.168.1.52"), "{inv1}");
        assert!(!inv1.contains("ansible_host=master-0"));
    }

    #[test]
    fn cluster_deser_backward_compatible_without_ip_fields() {
        // DB lama (sebelum ada master_ips/worker_ips) harus tetap bisa dibaca.
        let c: Cluster = serde_json::from_value(serde_json::json!({
            "id": "abc", "name": "old", "token_secret": "s"
        }))
        .unwrap();
        assert!(c.master_ips.is_empty());
        assert!(c.worker_ips.is_empty());
    }

    fn static_cluster(base: &str, masters: i32, workers: i32) -> Cluster {
        let mut payload: Cluster = serde_json::from_value(serde_json::json!({
            "name": "s", "token_secret": "x",
            "ip_mode": "static", "static_ip_base": base,
            "gateway": "192.168.1.1",
            "master_count": masters, "worker_count": workers,
        }))
        .unwrap();
        payload = Cluster::new(payload);
        assert_eq!(payload.ip_mode, "static");
        payload
    }

    #[test]
    fn static_ips_sequential_masters_then_workers() {
        let c = static_cluster("192.168.1.50", 1, 2);
        let (m, w) = static_ips_for_cluster(&c).unwrap();
        assert_eq!(m, vec!["192.168.1.50"]);
        assert_eq!(w, vec!["192.168.1.51", "192.168.1.52"]);

        let (mcfg, wcfg) = static_ipconfigs_for_cluster(&c);
        assert_eq!(mcfg, vec!["ip=192.168.1.50/24,gw=192.168.1.1"]);
        assert_eq!(
            wcfg,
            vec!["ip=192.168.1.51/24,gw=192.168.1.1", "ip=192.168.1.52/24,gw=192.168.1.1"]
        );

        // inventory langsung pakai IP statis, tanpa perlu terraform output
        let inv = build_inventory(&{
            let mut cc = c.clone();
            cc.master_ips = m;
            cc.worker_ips = w;
            cc
        });
        assert!(inv.contains("ansible_host=192.168.1.50"), "{inv}");
        assert!(!inv.contains("ansible_host=master-0"));
    }

    #[test]
    fn static_ips_reject_bad_base() {
        for bad in ["", "abc", "192.168.1", "192.168.1.300", "192.168.1.0", "192.168.1.254"] {
            // .254 + 1 master + 2 worker = lewat .254 -> None; sisanya format salah.
            let c = static_cluster(bad, 1, 2);
            assert!(static_ips_for_cluster(&c).is_none(), "{bad}");
        }
        // dhcp mode selalu None (tidak mengganggu perilaku lama)
        let dhcp: Cluster = serde_json::from_value(serde_json::json!({
            "name": "d", "token_secret": "x", "static_ip_base": "192.168.1.50",
        }))
        .map(Cluster::new)
        .unwrap();
        assert_eq!(dhcp.ip_mode, "dhcp");
        assert!(static_ips_for_cluster(&dhcp).is_none());
        let (m, w) = static_ipconfigs_for_cluster(&dhcp);
        assert!(m.is_empty() && w.is_empty());
    }

    #[test]
    fn filegen_static_writes_ipconfigs_and_real_inventory() {
        let base = std::env::temp_dir().join(format!("pp-test-{}", uuid::Uuid::new_v4()));
        let infra = base.to_string_lossy().to_string();
        let c = static_cluster("10.0.0.20", 1, 1);
        write_terraform(&infra, &c, None).unwrap();

        let tfvars = std::fs::read_to_string(format!("{infra}/terraform/{}/terraform.tfvars", c.id)).unwrap();
        let tfvars = norm_ws(&tfvars);
        assert!(tfvars.contains("master_ipconfigs = [\"ip=10.0.0.20/24,gw=192.168.1.1\"]"), "{tfvars}");
        assert!(tfvars.contains("worker_ipconfigs = [\"ip=10.0.0.21/24,gw=192.168.1.1\"]"), "{tfvars}");

        let main = std::fs::read_to_string(format!("{infra}/terraform/{}/main.tf", c.id)).unwrap();
        assert!(main.contains("var.master_ipconfigs[count.index]"), "{main}");

        let inv = std::fs::read_to_string(format!("{infra}/terraform/{}/inventory.ini", c.id)).unwrap();
        assert!(inv.contains("ansible_host=10.0.0.20"), "{inv}");
        assert!(inv.contains("ansible_host=10.0.0.21"), "{inv}");

        std::fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn vm_names_unique_per_cluster() {
        let mk = |name: &str| {
            let p: Cluster = serde_json::from_value(serde_json::json!({
                "name": name, "token_secret": "x",
            }))
            .unwrap();
            Cluster::new(p)
        };
        let a = mk("Toko Online");
        let b = mk("Toko Online");
        let pa = vm_prefix(&a);
        let pb = vm_prefix(&b);
        // nama sama tapi id beda -> prefix beda (tidak tabrakan)
        assert_ne!(pa, pb);
        assert!(pa.starts_with("toko-online-"), "{pa}");

        // filegen menulis name_prefix + main.tf memakainya
        let base = std::env::temp_dir().join(format!("pp-test-{}", uuid::Uuid::new_v4()));
        let infra = base.to_string_lossy().to_string();
        write_terraform(&infra, &a, None).unwrap();
        let tfvars = std::fs::read_to_string(format!("{infra}/terraform/{}/terraform.tfvars", a.id)).unwrap();
        let tfvars = norm_ws(&tfvars);
        assert!(tfvars.contains(&format!("name_prefix = \"{pa}\"")), "{tfvars}");
        let main = std::fs::read_to_string(format!("{infra}/terraform/{}/main.tf", a.id)).unwrap();
        assert!(main.contains("${var.name_prefix}-master-${count.index}"), "{main}");
        assert!(main.contains("${var.name_prefix}-worker-${count.index}"), "{main}");
        assert!(!main.contains("k8s-master-${count.index}"));
        std::fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn single_vm_zero_workers_generates_no_worker_entries() {
        let c: Cluster = serde_json::from_value(serde_json::json!({
            "name": "solo", "token_secret": "x",
            "master_count": 1, "worker_count": 0,
        }))
        .map(Cluster::new)
        .unwrap();
        let inv = build_inventory(&c);
        let entries: Vec<&str> = inv
            .lines()
            .filter(|l| !l.trim_start().starts_with('#') && l.contains("ansible_host="))
            .collect();
        assert_eq!(entries.len(), 1, "{inv}");
        assert!(entries[0].starts_with("master-0 "));
    }

    fn feat_cluster(features: &[&str], workers: i32) -> Cluster {
        let f: Vec<String> = features.iter().map(|s| s.to_string()).collect();
        let p: Cluster = serde_json::from_value(serde_json::json!({
            "name": "f", "token_secret": "x",
            "master_count": 1, "worker_count": workers,
            "enabled_features": f,
        }))
        .unwrap();
        Cluster::new(p)
    }

    #[test]
    fn ansible_steps_follow_features() {
        // Node.js only, 1 VM: common + nodejs, tanpa k8s/nginx/workers.
        let node = feat_cluster(&["nodejs"], 0);
        let steps: Vec<&str> = ansible_steps(&node).iter().map(|(pb, _)| *pb).collect();
        assert_eq!(steps, vec!["playbook-common.yml", "playbook-nodejs.yml"]);

        // Default k8s+nginx dengan worker: urutan common, master, workers, nginx.
        let k8s = feat_cluster(&["k8s", "nginx"], 2);
        let steps: Vec<&str> = ansible_steps(&k8s).iter().map(|(pb, _)| *pb).collect();
        assert_eq!(
            steps,
            vec!["playbook-common.yml", "playbook-master.yml", "playbook-workers.yml", "playbook-nginx.yml"]
        );

        // Fitur kosong = hanya common (VM polos).
        let bare = feat_cluster(&[], 1);
        let steps: Vec<&str> = ansible_steps(&bare).iter().map(|(pb, _)| *pb).collect();
        assert_eq!(steps, vec!["playbook-common.yml"]);
    }

        #[test]
    fn filegen_writes_run_ansible_runner() {

        let base = std::env::temp_dir().join(format!("pp-test-{}", uuid::Uuid::new_v4()));
        let infra = base.to_string_lossy().to_string();
        let c = feat_cluster(&["nodejs"], 0);
        write_terraform(&infra, &c, None).unwrap();

        let run = std::fs::read_to_string(format!("{infra}/terraform/{}/run-ansible.sh", c.id)).unwrap();
        assert!(run.contains("playbook-common.yml"), "{run}");
        assert!(run.contains("playbook-nodejs.yml"), "{run}");
        assert!(!run.contains("playbook-master.yml"), "{run}");
        assert!(run.contains("ANSIBLE_DIR"), "{run}");

        std::fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn vm_prefix_uses_custom_sanitized_or_auto() {
        let mut p: Cluster = serde_json::from_value(serde_json::json!({
            "name": "Toko Online", "token_secret": "x", "vm_name_prefix": "Web-1 Utama!",
        }))
        .map(Cluster::new)
        .unwrap();
        assert!(vm_prefix(&p).starts_with("web-1-utama-"), "{}", vm_prefix(&p));

        // prefix sampah -> fallback otomatis dari nama cluster
        p.vm_name_prefix = "!!!".to_string();
        assert!(vm_prefix(&p).starts_with("toko-online-"), "{}", vm_prefix(&p));

        // kosong -> otomatis
        p.vm_name_prefix.clear();
        assert!(vm_prefix(&p).starts_with("toko-online-"), "{}", vm_prefix(&p));
    }

    #[test]
    fn filegen_injects_ssh_key_or_null() {
        let base = std::env::temp_dir().join(format!("pp-test-{}", uuid::Uuid::new_v4()));
        let infra = base.to_string_lossy().to_string();
        let c: Cluster = serde_json::from_value(serde_json::json!({
            "name": "k", "token_secret": "x", "ssh_user": "rocky",
        }))
        .map(Cluster::new)
        .unwrap();

        // Dengan key: ciuser + sshkeys terisi
        write_terraform(&infra, &c, Some("ssh-ed25519 AAAAtest panel")).unwrap();
        let tfvars = std::fs::read_to_string(format!("{infra}/terraform/{}/terraform.tfvars", c.id)).unwrap();
        let tfvars = norm_ws(&tfvars);
        assert!(tfvars.contains("ciuser = \"rocky\""), "{tfvars}");
        assert!(tfvars.contains("sshkeys = \"ssh-ed25519 AAAAtest panel\""), "{tfvars}");

        // Tanpa key: null (argumen dianggap tidak diset, template fallback berlaku)
        write_terraform(&infra, &c, None).unwrap();
        let tfvars = std::fs::read_to_string(format!("{infra}/terraform/{}/terraform.tfvars", c.id)).unwrap();
        let tfvars = norm_ws(&tfvars);
        assert!(tfvars.contains("sshkeys = null"), "{tfvars}");

        let main = std::fs::read_to_string(format!("{infra}/terraform/{}/main.tf", c.id)).unwrap();
        assert!(main.contains("ciuser   = var.ciuser"), "{main}");
        assert!(main.contains("sshkeys  = var.sshkeys"), "{main}");

        std::fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn disk_vlan_normalized() {
        let c: Cluster = serde_json::from_value(serde_json::json!({
            "name": "d", "token_secret": "x",
            "disk_size_gb": -5, "disk_storage": "", "vlan_tag": 9999,
        }))
        .map(Cluster::new)
        .unwrap();
        assert_eq!(c.disk_size_gb, 0);
        assert_eq!(c.disk_storage, "local-lvm");
        assert_eq!(c.vlan_tag, -1);
    }

        #[test]
    fn filegen_disk_vlan_nameserver() {

        let base = std::env::temp_dir().join(format!("pp-test-{}", uuid::Uuid::new_v4()));
        let infra = base.to_string_lossy().to_string();
        let c: Cluster = serde_json::from_value(serde_json::json!({
            "name": "d", "token_secret": "x",
            "disk_size_gb": 50, "disk_storage": "local-lvm",
            "vlan_tag": 20, "dns1": "1.1.1.1",
        }))
        .map(Cluster::new)
        .unwrap();
        write_terraform(&infra, &c, None).unwrap();

        let tfvars = std::fs::read_to_string(format!("{infra}/terraform/{}/terraform.tfvars", c.id)).unwrap();
        let tfvars = norm_ws(&tfvars);
        assert!(tfvars.contains("disk_size_gb = 50"), "{tfvars}");
        assert!(tfvars.contains("disk_storage = \"local-lvm\""), "{tfvars}");
        assert!(tfvars.contains("vlan_tag = 20"), "{tfvars}");

        let main = std::fs::read_to_string(format!("{infra}/terraform/{}/main.tf", c.id)).unwrap();
        assert!(main.contains("dynamic \"disk\""), "{main}");
        assert!(main.contains("size    = \"${var.disk_size_gb}G\""), "{main}");
        assert!(main.contains("tag      = var.vlan_tag"), "{main}");
        assert!(main.contains("nameserver = var.dns1"), "{main}");

        // Default: inherit disk (0), tanpa tag (-1).
        let d: Cluster = serde_json::from_value(serde_json::json!({
            "name": "e", "token_secret": "x",
        }))
        .map(Cluster::new)
        .unwrap();
        write_terraform(&infra, &d, None).unwrap();
        let tfvars = std::fs::read_to_string(format!("{infra}/terraform/{}/terraform.tfvars", d.id)).unwrap();
        let tfvars = norm_ws(&tfvars);
        assert!(tfvars.contains("disk_size_gb = 0"), "{tfvars}");
        assert!(tfvars.contains("vlan_tag = -1"), "{tfvars}");

        std::fs::remove_dir_all(&base).ok();
    }

    /// Dump filegen ke temp dir TANPA cleanup, untuk `terraform validate`
    /// manual di WSL: `cargo test dump_for_terraform_validate -- --ignored --nocapture`
    /// lalu di WSL: `terraform init -backend=false && terraform validate`.
    #[test]
    #[ignore]
    fn dump_for_terraform_validate() {
        let dir = std::env::temp_dir().join("pp-validate");
        std::fs::remove_dir_all(&dir).ok();
        let infra = dir.to_string_lossy().to_string();
        let c: Cluster = serde_json::from_value(serde_json::json!({
            "name": "Validate Me", "token_secret": "dummy",
            "proxmox_url": "https://192.168.1.100:8006/api2/json",
            "master_count": 1, "worker_count": 1,
            "disk_size_gb": 50, "disk_storage": "local-lvm", "vlan_tag": 20,
            "ip_mode": "static", "static_ip_base": "192.168.1.50",
            "vm_name_prefix": "web-1",
        }))
        .map(Cluster::new)
        .unwrap();
        write_terraform(&infra, &c, Some("ssh-ed25519 AAAAdummy panel")).unwrap();
        println!("DUMPED to {infra}/terraform/{}", c.id);
    }

    #[test]
    fn plan_summary_extracts_plan_line() {
        let out = "Refreshing state...\nPlan: 3 to add, 0 to change, 1 to destroy.\n\nSaved the plan.";
        assert_eq!(
            plan_summary(out).as_deref(),
            Some("Plan: 3 to add, 0 to change, 1 to destroy.")
        );
        assert!(plan_summary("No changes. Your infrastructure matches the configuration.").is_none());
        assert!(plan_summary("random output").is_none());
    }
}

// Shim for unwrap_or on JoinHandle error path (never actually constructed).
