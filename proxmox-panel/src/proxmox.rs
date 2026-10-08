//! Proxmox VE API client (API Token auth).
//!
//! Header: `Authorization: PVEAPIToken=<user>!<tokenid>=<secret>`
//! `verify_tls=false` (default di UI) => accept invalid certs (self-signed Proxmox).

use crate::models::ProxmoxNode;
use std::time::{Duration, Instant};

pub struct ProxmoxCreds {
    pub base_url: String,
    pub user: String,
    pub token_id: String,
    pub token_secret: String,
    pub verify_tls: bool,
}

fn client(verify_tls: bool) -> reqwest::Client {
    reqwest::Client::builder()
        .danger_accept_invalid_certs(!verify_tls)
        .timeout(Duration::from_secs(15))
        .build()
        .expect("reqwest client")
}

fn api_url(base: &str, path: &str) -> String {
    let b = base.trim_end_matches('/');
    // Accept both ".../api2/json" and host-only input.
    if b.ends_with("/api2/json") {
        format!("{b}{path}")
    } else {
        format!("{b}/api2/json{path}")
    }
}

pub async fn test_connection(c: &ProxmoxCreds) -> serde_json::Value {
    let start = Instant::now();
    let url = api_url(&c.base_url, "/version");
    let auth = format!("PVEAPIToken={}!{}={}", c.user, c.token_id, c.token_secret);

    // Preflight: TCP dial to host:port so we can tell
    // "network/firewall" apart from "TLS" and "HTTP auth" failures.
    let tcp_info = tcp_preflight(&url).await;

    let cli = client(c.verify_tls);
    let res = cli.get(&url).header("Authorization", auth).send().await;
    let ms = start.elapsed().as_millis();
    match res {
        Ok(r) => {
            let status = r.status();
            let body = r.text().await.unwrap_or_default();
            if status.is_success() {
                let v: serde_json::Value = serde_json::from_str(&body).unwrap_or_default();
                let version = v
                    .pointer("/data/version")
                    .and_then(|x| x.as_str())
                    .unwrap_or("unknown")
                    .to_string();
                let release = v
                    .pointer("/data/release")
                    .and_then(|x| x.as_str())
                    .unwrap_or("")
                    .to_string();
                serde_json::json!({
                    "ok": true, "version": version, "release": release,
                    "latency_ms": ms, "url": url, "raw": body.chars().take(500).collect::<String>()
                })
            } else if status.as_u16() == 401 {
                serde_json::json!({
                    "ok": false, "error": "HTTP 401 Unauthorized",
                    "hint": "Server reachable + TLS OK. Token salah / privilege separation on / user salah. Format user: root@pam, token_id: hanya bagian setelah '!'.",
                    "latency_ms": ms, "url": url, "tcp": tcp_info,
                    "raw": body.chars().take(500).collect::<String>()
                })
            } else {
                serde_json::json!({
                    "ok": false, "error": format!("HTTP {}", status),
                    "latency_ms": ms, "url": url, "tcp": tcp_info,
                    "raw": body.chars().take(500).collect::<String>()
                })
            }
        }
        Err(e) => {
            let detail = format!("{e:?}");
            serde_json::json!({
                "ok": false, "error": e.to_string(),
                "detail": detail.chars().take(800).collect::<String>(),
                "hint": classify_hint(&e, c.verify_tls),
                "latency_ms": ms, "url": url, "tcp": tcp_info
            })
        }
    }
}

/// Quick TCP dial (3s) to separate network vs TLS vs HTTP failures.
async fn tcp_preflight(url: &str) -> serde_json::Value {
    let (host, port) = parse_host_port(url);
    let addr = format!("{host}:{port}");
    let t0 = Instant::now();
    match tokio::time::timeout(
        Duration::from_secs(5),
        tokio::net::TcpStream::connect(addr.as_str()),
    )
    .await
    {
        Ok(Ok(_)) => serde_json::json!({"reachable": true, "host": host, "port": port, "ms": t0.elapsed().as_millis()}),
        Ok(Err(e)) => serde_json::json!({"reachable": false, "host": host, "port": port, "error": e.to_string()}),
        Err(_) => serde_json::json!({"reachable": false, "host": host, "port": port, "error": "TCP connect timeout (5s): firewall / IP salah / port 8006 tertutup"}),
    }
}

fn parse_host_port(url: &str) -> (String, u16) {
    let s = url
        .trim_start_matches("https://")
        .trim_start_matches("http://");
    let hostport = s.split('/').next().unwrap_or("");
    if let Some((h, p)) = hostport.rsplit_once(':') {
        if let Ok(port) = p.parse::<u16>() {
            return (h.to_string(), port);
        }
    }
    (hostport.to_string(), 8006)
}

fn classify_hint(e: &reqwest::Error, verify_tls: bool) -> &'static str {
    // NOTE: hyper wraps TLS failures as "Connect" errors, so check
    // cert keywords BEFORE is_connect().
    let s = format!("{e:?}").to_lowercase();
    if s.contains("cert") || s.contains("certificate") || s.contains("tls") || s.contains("ssl") {
        if verify_tls {
            return "Sertifikat Proxmox self-signed (UnknownIssuer). MATIKAN checkbox 'Verify TLS' lalu test lagi.";
        }
        return "TLS error walau Verify TLS off. Cek jam sistem / proxy / SNI.";
    }
    if e.is_timeout() {
        return "Timeout: server lambat / paket loss (server ini butuh ~4s). Coba lagi; timeout backend 15s.";
    }
    if e.is_connect() {
        return "TCP connect gagal: IP/port salah, firewall, atau Proxmox tidak listen 8006.";
    }
    if s.contains("proxy") {
        return "Proxy error: backend ikut env HTTP(S)_PROXY. Unset proxy untuk IP ini.";
    }
    "Unknown transport error — lihat field 'detail' dan 'tcp'. Kalau tcp.reachable=true tapi request gagal, kemungkinan TLS."
}

pub async fn list_nodes(c: &ProxmoxCreds) -> Result<Vec<ProxmoxNode>, String> {
    let cli = client(c.verify_tls);
    let url = api_url(&c.base_url, "/nodes");
    let auth = format!("PVEAPIToken={}!{}={}", c.user, c.token_id, c.token_secret);
    let r = cli
        .get(&url)
        .header("Authorization", auth)
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if !r.status().is_success() {
        return Err(format!("HTTP {}", r.status()));
    }
    let v: serde_json::Value = r.json().await.map_err(|e| e.to_string())?;
    let mut out = vec![];
    if let Some(arr) = v.pointer("/data").and_then(|x| x.as_array()) {
        for n in arr {
            let name = n.get("node").and_then(|x| x.as_str()).unwrap_or("?").to_string();
            let status = n.get("status").and_then(|x| x.as_str()).unwrap_or("unknown").to_string();
            out.push(ProxmoxNode {
                name,
                status,
                cpu: n.get("cpu").and_then(|x| x.as_f64()).unwrap_or(0.0),
                memory: n.get("maxmem").and_then(|x| x.as_u64()).unwrap_or(0),
                used_mem: n.get("mem").and_then(|x| x.as_u64()).unwrap_or(0),
                disk: n.get("maxdisk").and_then(|x| x.as_u64()).unwrap_or(0),
                used_disk: n.get("disk").and_then(|x| x.as_u64()).unwrap_or(0),
                live: true,
            });
        }
    }
    Ok(out)
}

pub fn mock_nodes() -> Vec<ProxmoxNode> {
    vec![ProxmoxNode {
        name: "pve".to_string(),
        status: "online (mock)".to_string(),
        cpu: 0.45,
        memory: 67108864,
        used_mem: 33554432,
        disk: 53687091200,
        used_disk: 21474836480,
        live: false,
    }]
}
