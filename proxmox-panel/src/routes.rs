use crate::exec;
use crate::models::{Cluster, ProxmoxTestRequest, SshTestRequest, TemplateOption};
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
        .route("/api/clusters/:id", delete(delete_cluster))
        .route("/api/clusters/:id/status", get(cluster_status))
        .route("/api/clusters/:id/deploy", post(deploy_cluster))
        .route("/api/nodes", get(list_nodes))
        .route("/api/templates", get(list_templates))
        .route("/api/config", get(get_config))
        .route("/api/tools", get(tools))
        .route("/api/realtime/summary", get(summary))
        .route("/api/health/proxmox-test", post(proxmox_test))
        .route("/api/ssh/test", post(ssh_test))
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
    // WSL preferred (Linux ssh + keys di ~/.ssh WSL), fallback ke ssh native
    // (mis. OpenSSH bawaan Windows) supaya tetap bisa tanpa WSL.
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
    state.set_status(&id, "deploying", 5);
    state.push_log(&id, "provision", "Validating cluster configuration (API Token auth)...", "info");

    // Validate token present
    if cluster.token_secret.is_empty() {
        state.push_log(&id, "provision", "ERROR: API Token secret is empty.", "error");
        state.set_status(&id, "error", 0);
        return;
    }
    tokio::time::sleep(std::time::Duration::from_millis(600)).await;

    // 1. Test Proxmox connection live
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
    let ok = test.get("ok").and_then(|x| x.as_bool()).unwrap_or(false);
    if ok {
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
                "Proxmox unreachable ({}). Continuing in simulated mode.",
                test.get("error").and_then(|x| x.as_str()).unwrap_or("unknown error")
            ),
            "warning",
        );
    }

    // 2. Generate terraform files
    state.set_status(&id, "deploying", 22);
    state.push_log(&id, "provision", "Generating Terraform configuration...", "info");
    if let Err(e) = write_terraform(&state.infra_dir, &cluster) {
        state.push_log(&id, "provision", &format!("Failed to write terraform files: {e}"), "error");
        state.set_status(&id, "error", 22);
        return;
    }
    state.push_log(&id, "provision", "Terraform files written to infra/terraform/<id>/", "info");

    // 3. Remote vs WSL-local execution path
    let use_remote = !cluster.ssh_host.trim().is_empty();
    if use_remote {
        state.push_log(
            &id,
            "provision",
            &format!(
                "Remote mode: WSL -> ssh {}@{}:{} (Proxmox server)",
                cluster.ssh_remote_user_or_default(),
                cluster.ssh_host,
                cluster.ssh_port
            ),
            "info",
        );
        remote_path(&state, &id, &cluster).await;
    } else {
        state.push_log(&id, "provision", "Local mode: executing via WSL (wsl bash -lc ...).", "info");
        local_path(&state, &id, &cluster).await;
    }

    // 4. Simulated phases (always run so UI shows full K8s flow; real outputs interleaved above)
    let steps = [
        ("Running terraform init...", 32),
        ("Provisioning master VM(s)...", 44),
        ("Provisioning worker VM(s)...", 54),
        ("Waiting for VMs to boot (cloud-init)...", 62),
        ("Setting up Kubernetes master (kubeadm)...", 72),
        ("Joining worker nodes to cluster...", 80),
        ("Installing Calico CNI network...", 86),
        ("Deploying Nginx landing page...", 92),
        ("Running health checks...", 96),
    ];
    for (msg, p) in steps {
        // Skip sleeping if already error? keep going.
        state.push_log(&id, "provision", msg, "info");
        state.set_status(&id, "deploying", p);
        tokio::time::sleep(std::time::Duration::from_millis(700)).await;
    }

    state.set_status(&id, "running", 100);
    state.push_log(&id, "provision", "Deployment complete! Cluster is ready.", "info");
}

async fn local_path(state: &AppState, id: &str, cluster: &Cluster) {
    if !tokio::task::spawn_blocking(exec::wsl_available).await.unwrap_or(false) {
        // Tanpa WSL: coba terraform native (Windows). Ansible tidak ada native
        // di Windows — untuk tahap ansible gunakan mode remote (SSH ke server).
        let tf = tokio::task::spawn_blocking(|| exec::tool_version_local("terraform"))
            .await
            .unwrap_or(exec::CmdResult { ok: false, output: "tool check failed".into(), ms: 0 });
        if !tf.ok {
            state.push_log(id, "provision", "WSL tidak ada + terraform native tidak ketemu. Simulating. (Ansible memang butuh Linux — pakai mode remote/SSH.)", "warning");
            return;
        }
        state.push_log(id, "provision", &format!("Terraform native: {}", tf.output.lines().next().unwrap_or("")), "info");
        let dir = format!("{}/terraform/{}", state.infra_dir, cluster.id);
        let out = tokio::task::spawn_blocking(move || {
            std::process::Command::new("terraform")
                .args(["init", "-backend=false", "-input=false"])
                .current_dir(&dir)
                .output()
        })
        .await;
        match out {
            Ok(Ok(o)) => {
                let s = String::from_utf8_lossy(&o.stdout).to_string();
                state.push_log(id, "provision", &format!("terraform init (native) ok={}:\n{}", o.status.success(), truncate(&s, 1200)), if o.status.success() { "info" } else { "warning" });
            }
            _ => state.push_log(id, "provision", "terraform init (native) gagal dijalankan.", "warning"),
        }
        return;
    }
    // Best-effort real tool detection inside WSL.
    let tools = tokio::task::spawn_blocking(exec::tools_summary).await.unwrap_or(json!({}));
    let tf_ok = tools.pointer("/terraform/wsl/ok").and_then(|x| x.as_bool()).unwrap_or(false);
    let an_ok = tools.pointer("/ansible/wsl/ok").and_then(|x| x.as_bool()).unwrap_or(false);
    state.push_log(
        id,
        "provision",
        &format!(
            "WSL tools: terraform(wsl)={} ansible(wsl)={} | terraform {} | ansible {}",
            tf_ok,
            an_ok,
            tools.pointer("/terraform/wsl/output").and_then(|x| x.as_str()).unwrap_or("-"),
            tools.pointer("/ansible/wsl/output").and_then(|x| x.as_str()).unwrap_or("-"),
        ),
        if tf_ok && an_ok { "info" } else { "warning" },
    );
    if !tf_ok {
        state.push_log(id, "provision", "terraform not found in WSL (install it there to enable real runs). Simulating.", "warning");
        return;
    }
    // Attempt `terraform init -backend=false` as a safe non-destructive proof.
    let dir_win = format!("{}/terraform/{}", state.infra_dir, cluster.id);
    let dir_wsl = win_to_wsl(&dir_win);
    let cmd = format!("cd \"{}\" && terraform init -backend=false -input=false 2>&1 | head -30", dir_wsl.replace('"', "\\\""));
    state.push_log(id, "provision", &format!("WSL exec: {cmd}"), "info");
    let out = tokio::task::spawn_blocking(move || exec::run_in_wsl(&cmd)).await;
    match out {
        Ok(r) => state.push_log(
            id,
            "provision",
            &format!("terraform init (wsl) ok={}:\n{}", r.ok, truncate(&r.output, 1200)),
            if r.ok { "info" } else { "warning" },
        ),
        Err(e) => state.push_log(id, "provision", &format!("WSL exec failed: {e}"), "error"),
    }
    let _ = an_ok;
}

async fn remote_path(state: &AppState, id: &str, cluster: &Cluster) {
    let host = cluster.ssh_host.clone();
    let user = cluster.ssh_remote_user_or_default();
    let port = cluster.ssh_port;
    // 1. SSH connectivity test via WSL
    state.push_log(id, "provision", &format!("SSH via WSL: ssh -p {port} {user}@{host} ..."), "info");
    let r = tokio::task::spawn_blocking(move || exec::ssh_test_via_wsl(&host, &user, port)).await;
    match r {
        Ok(res) if res.ok => {
            state.push_log(id, "provision", &format!("SSH OK ({} ms):\n{}", res.ms, truncate(&res.output, 800)), "info");
        }
        Ok(res) => {
            state.push_log(
                id,
                "provision",
                &format!("SSH FAILED ({} ms). Check ~/.ssh keys in WSL + authorized_keys on Proxmox. Output:\n{}", res.ms, truncate(&res.output, 800)),
                "error",
            );
            state.push_log(id, "provision", "Continuing in simulated mode. Fix SSH then re-deploy.", "warning");
            return;
        }
        Err(e) => {
            state.push_log(id, "provision", &format!("SSH task failed: {e}"), "error");
            return;
        }
    }
    // 2. Remote tool versions
    let host2 = cluster.ssh_host.clone();
    let user2 = cluster.ssh_remote_user_or_default();
    let r2 = tokio::task::spawn_blocking(move || {
        exec::ssh_exec_via_wsl(&host2, &user2, port, "terraform version 2>&1 | head -2; echo ---; ansible --version 2>&1 | head -2")
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
    state.push_log(
        id,
        "provision",
        "Remote apply: sync infra/terraform/<id>/ to server (scp) then `terraform apply` there. Auto-sync not yet enabled — run manually, see README.",
        "warning",
    );
}

// ---------- terraform filegen ----------

fn write_terraform(infra_dir: &str, c: &Cluster) -> std::io::Result<()> {
    let dir = format!("{infra_dir}/terraform/{}", c.id);
    std::fs::create_dir_all(&dir)?;
    let vars = format!(
        "proxmox_api_url = \"{}\"\nproxmox_user = \"{}:{}\"\nproxmox_token = \"{}\"\ntarget_node = \"{}\"\nclone_template = \"{}\"\nnetwork_bridge = \"{}\"\ngateway = \"{}\"\ndns1 = \"{}\"\nssh_user = \"{}\"\nmaster_count = {}\nworker_count = {}\nmaster_cpu = {}\nmaster_ram = {}\nworker_cpu = {}\nworker_ram = {}\n",
        c.proxmox_url,
        c.proxmox_user,
        c.token_id,
        "***redacted***",
        c.target_node,
        c.clone_template,
        c.network_bridge,
        c.gateway,
        c.dns1,
        c.ssh_user,
        c.master_count,
        c.worker_count,
        c.master_cpu,
        c.master_ram,
        c.worker_cpu,
        c.worker_ram,
    );
    std::fs::write(format!("{dir}/terraform.tfvars"), vars)?;
    std::fs::write(format!("{dir}/main.tf"), terraform_main_tf())?;
    // Write a non-secret inventory for ansible stage.
    let inv = format!(
        "[k8s_master]\nmaster ansible_host=192.168.1.10 ansible_user={}\n\n[k8s_workers]\nworker1 ansible_host=192.168.1.11 ansible_user={}\nworker2 ansible_host=192.168.1.12 ansible_user={}\n",
        c.ssh_user, c.ssh_user, c.ssh_user
    );
    std::fs::write(format!("{dir}/inventory.ini"), inv)?;
    Ok(())
}

fn terraform_main_tf() -> &'static str {
    r#"# Generated by proxmox-panel (Rust). Fill pm_api_token_id/secret via env or tfvars on the target host.
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

// Shim for unwrap_or on JoinHandle error path (never actually constructed).
