use crate::exec;
use crate::models::{Cluster, ProxmoxTestRequest, SshCopyIdRequest, SshTestRequest, TemplateOption};
use crate::proxmox::{self, ProxmoxCreds};
use crate::store::AppState;
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
        .route("/api/clusters", get(list_clusters).post(create_cluster))
        .route("/api/clusters/:id", delete(delete_cluster).put(update_cluster))
        .route("/api/clusters/:id/status", get(cluster_status))
        .route("/api/clusters/:id/deploy", post(deploy_cluster))
        .route("/api/nodes", get(list_nodes))
        .route("/api/templates", get(list_templates))
        .route("/api/config", get(get_config))
        .route("/api/tools", get(tools))
        .route("/api/realtime/summary", get(summary))
        .route("/api/health/proxmox-test", post(proxmox_test))
        .route("/api/ssh/test", post(ssh_test))
        .route("/api/ssh/key", get(ssh_key))
        .route("/api/ssh/keygen", post(ssh_keygen))
        .route("/api/ssh/copy-id", post(ssh_copy_id))
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
    if !payload.gateway.trim().is_empty() {
        cur.gateway = payload.gateway.trim().to_string();
    }
    if !payload.dns1.trim().is_empty() {
        cur.dns1 = payload.dns1.trim().to_string();
    }
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
    if !payload.enabled_features.is_empty() {
        cur.enabled_features = payload.enabled_features.clone();
    }
    cur.updated_at = chrono::Utc::now();
    s.db.save_cluster(&cur);
    s.push_log(&id, "provision", "Connection settings updated. Re-deploy to apply.", "info");
    (StatusCode::OK, Json(cur.masked())).into_response()
}

async fn deploy_cluster(State(s): State<AppState>, Path(id): Path<String>) -> impl IntoResponse {
    if !s.cluster_exists(&id) {
        return (StatusCode::NOT_FOUND, Json(json!({"error": "cluster not found"})));
    }
    let bg = s.clone();
    let id2 = id.clone();
    tokio::spawn(async move {
        run_deployment(bg, &id2).await;
    });
    (StatusCode::OK, Json(json!({"message": "deployment started", "id": id})))
}

// ---------- nodes / templates / config ----------

#[derive(Debug, Deserialize)]
struct NodesQuery {
    cluster_id: Option<String>,
}

async fn list_nodes(State(s): State<AppState>, Query(q): Query<NodesQuery>) -> Json<Value> {
    // Try live Proxmox if a cluster with token exists.
    let creds = q
        .cluster_id
        .as_deref()
        .and_then(|id| s.creds_of(id))
        .or_else(|| s.first_creds());

    if let Some(c) = creds {
        if !c.token_secret.is_empty() {
            if let Ok(nodes) = proxmox::list_nodes(&c).await {
                return Json(json!(nodes));
            }
        }
    }
    Json(json!(proxmox::mock_nodes()))
}

async fn list_templates() -> Json<Value> {
    Json(json!([
        {"name": "ubuntu-22-04-cloudinit", "storage": "local", "size": "4GB", "description": "Ubuntu 22.04 LTS Cloud-Init"},
        {"name": "ubuntu-24-04-cloudinit", "storage": "local", "size": "4GB", "description": "Ubuntu 24.04 LTS Cloud-Init"},
        {"name": "debian-12-cloudinit", "storage": "local", "size": "3GB", "description": "Debian 12 Cloud-Init"},
        {"name": "rocky-9-cloudinit", "storage": "local", "size": "4GB", "description": "Rocky Linux 9 Cloud-Init (user: rocky)"},
        {"name": "rocky-8-cloudinit", "storage": "local", "size": "4GB", "description": "Rocky Linux 8 Cloud-Init (user: rocky)"},
    ]))
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
        "features": ["k8s", "nginx", "monitoring"]
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
    if let Err(e) = write_terraform(&state.infra_dir, &cluster) {
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

    // 4. VMs exist now. Kubernetes install stays an explicit next step
    // (ansible playbooks below) instead of faked sleep phases.
    state.set_status(&id, "deploying", 90);
    state.push_log(&id, "provision", "Terraform apply succeeded — VMs are provisioned.", "info");
    let inv = format!("{}/terraform/{}/inventory.ini", state.infra_dir, cluster.id);
    state.push_log(
        &id,
        "k8s",
        &format!(
            "Next: install Kubernetes with ansible (from the repo root):\n  ansible-playbook -i \"{inv}\" ansible/playbook-master.yml\n  ansible-playbook -i \"{inv}\" ansible/playbook-workers.yml\n  ansible-playbook -i \"{inv}\" ansible/playbook-nginx.yml"
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

// ---------- terraform filegen ----------

fn write_terraform(infra_dir: &str, c: &Cluster) -> std::io::Result<()> {
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
    let vars = format!(
        "proxmox_api_url = \"{}\"\nproxmox_user = \"{}:{}\"\nproxmox_token = \"{}\"\ntarget_node = \"{}\"\nclone_template = \"{}\"\nnetwork_bridge = \"{}\"\ngateway = \"{}\"\ndns1 = \"{}\"\nssh_user = \"{}\"\nmaster_count = {}\nworker_count = {}\nmaster_cpu = {}\nmaster_ram = {}\nworker_cpu = {}\nworker_ram = {}\n",
        hcl_escape(&c.proxmox_url),
        hcl_escape(&c.proxmox_user),
        hcl_escape(&c.token_id),
        hcl_escape(&c.token_secret),
        hcl_escape(&c.target_node),
        hcl_escape(&c.clone_template),
        hcl_escape(&c.network_bridge),
        hcl_escape(&c.gateway),
        hcl_escape(&c.dns1),
        hcl_escape(&c.ssh_user),
        c.master_count,
        c.worker_count,
        c.master_cpu,
        c.master_ram,
        c.worker_cpu,
        c.worker_ram,
    );
    let tfvars = format!("{dir}/terraform.tfvars");
    std::fs::write(&tfvars, vars)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&tfvars, std::fs::Permissions::from_mode(0o600));
    }
    std::fs::write(format!("{dir}/main.tf"), terraform_main_tf())?;
    // Ansible inventory sized to the requested counts. IPs are DHCP-assigned
    // by Terraform — fill ansible_host from `terraform output` after apply.
    let mut inv = format!(
        "# Generated by proxpilot for cluster '{}' ({})\n# Fill ansible_host per node from `terraform output` (DHCP leases), e.g.\n#   master-0 ansible_host=192.168.1.50 ansible_user={}\n\n[k8s_master]\n",
        c.name, c.id, c.ssh_user
    );
    for i in 0..c.master_count.max(1) {
        inv.push_str(&format!("master-{i} ansible_host=master-{i} ansible_user={}\n", c.ssh_user));
    }
    inv.push_str("\n[k8s_workers]\n");
    for i in 0..c.worker_count.max(0) {
        inv.push_str(&format!("worker-{i} ansible_host=worker-{i} ansible_user={}\n", c.ssh_user));
    }
    inv.push_str("\n[nginx_group]\nmaster-0\n\n[k8s_cluster:children]\nk8s_master\nk8s_workers\n");
    std::fs::write(format!("{dir}/inventory.ini"), inv)?;
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
    Ok(())
}

fn hcl_escape(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

fn terraform_main_tf() -> &'static str {
    r#"# Generated by proxpilot (Rust). Credentials come from the
# generated terraform.tfvars (mode 0600) next to this file.
variable "proxmox_api_url" { type = string }
variable "proxmox_user" { type = string }
variable "proxmox_token" { type = string sensitive = true }
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

provider "proxmox" {
  pm_api_url      = var.proxmox_api_url
  pm_user         = split(":", var.proxmox_user)[0]
  pm_api_token_id = split(":", var.proxmox_user)[1]
  pm_api_token_secret = var.proxmox_token
  pm_tls_insecure = true
}

resource "proxmox_vm_qemu" "master" {
  count       = var.master_count
  name        = "k8s-master-${count.index}"
  target_node = var.target_node
  clone       = var.clone_template
  cores = var.master_cpu
  sockets = 1
  cpu = "host"
  memory = var.master_ram
  agent = 1
  os_type = "cloud-init"
  scsihw = "virtio-scsi-single"
  network { bridge = var.network_bridge firewall = false }
  ipconfig0 = "ip=dhcp"
  ssh_user = var.ssh_user
  start = true
  onboot = true
  lifecycle { ignore_changes = [network] }
}

resource "proxmox_vm_qemu" "worker" {
  count       = var.worker_count
  name        = "k8s-worker-${count.index}"
  target_node = var.target_node
  clone       = var.clone_template
  cores = var.worker_cpu
  sockets = 1
  cpu = "host"
  memory = var.worker_ram
  agent = 1
  os_type = "cloud-init"
  scsihw = "virtio-scsi-single"
  network { bridge = var.network_bridge firewall = false }
  ipconfig0 = "ip=dhcp"
  ssh_user = var.ssh_user
  start = true
  onboot = true
  lifecycle { ignore_changes = [network] }
}

output "master_ips" {
  description = "DHCP IPs of master VMs (fill into inventory.ini)"
  value = proxmox_vm_qemu.master[*].default_ipv4_address
}

output "worker_ips" {
  description = "DHCP IPs of worker VMs (fill into inventory.ini)"
  value = proxmox_vm_qemu.worker[*].default_ipv4_address
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
        write_terraform(&infra, &c).unwrap();

        let tfvars = std::fs::read_to_string(format!("{infra}/terraform/{}/terraform.tfvars", c.id)).unwrap();
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
}

// Shim for unwrap_or on JoinHandle error path (never actually constructed).
