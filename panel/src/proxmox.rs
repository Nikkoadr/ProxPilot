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

// ---------- VM inventory + power (dipakai kartu VMs per cluster) ----------

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct VmInfo {
    pub vmid: u64,
    pub name: String,
    pub status: String,
    #[serde(default)]
    pub cpus: u32,
    #[serde(default)]
    pub cpu: f64,
    #[serde(default)]
    pub mem: u64,
    #[serde(default)]
    pub maxmem: u64,
    #[serde(default)]
    pub uptime: u64,
    #[serde(default)]
    pub template: bool,
}

/// GET /nodes/{node}/qemu — semua VM node itu (panel memfilter per prefix nama).
pub async fn list_vms(c: &ProxmoxCreds, node: &str) -> Result<Vec<VmInfo>, String> {
    let cli = client(c.verify_tls);
    let url = api_url(&c.base_url, &format!("/nodes/{node}/qemu"));
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
    Ok(parse_vm_list(&v))
}

fn parse_vm_list(v: &serde_json::Value) -> Vec<VmInfo> {
    let mut out = vec![];
    if let Some(arr) = v.pointer("/data").and_then(|x| x.as_array()) {
        for n in arr {
            out.push(VmInfo {
                vmid: n.get("vmid").and_then(|x| x.as_u64()).unwrap_or(0),
                name: n.get("name").and_then(|x| x.as_str()).unwrap_or("?").to_string(),
                status: n.get("status").and_then(|x| x.as_str()).unwrap_or("unknown").to_string(),
                cpus: n.get("cpus").and_then(|x| x.as_u64()).unwrap_or(0) as u32,
                cpu: n.get("cpu").and_then(|x| x.as_f64()).unwrap_or(0.0),
                mem: n.get("mem").and_then(|x| x.as_u64()).unwrap_or(0),
                maxmem: n.get("maxmem").and_then(|x| x.as_u64()).unwrap_or(0),
                uptime: n.get("uptime").and_then(|x| x.as_u64()).unwrap_or(0),
                template: n.get("template").and_then(|x| x.as_u64()).unwrap_or(0) == 1,
            });
        }
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

/// POST /nodes/{node}/qemu/{vmid}/status/{action} — action: start|shutdown|reboot|stop.
/// Mengembalikan UPID task Proxmox (async di sisi server).
pub async fn vm_power(c: &ProxmoxCreds, node: &str, vmid: u64, action: &str) -> Result<String, String> {
    let cli = client(c.verify_tls);
    let url = api_url(&c.base_url, &format!("/nodes/{node}/qemu/{vmid}/status/{action}"));
    let auth = format!("PVEAPIToken={}!{}={}", c.user, c.token_id, c.token_secret);
    let r = cli
        .post(&url)
        .header("Authorization", auth)
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if !r.status().is_success() {
        let code = r.status();
        let body = r.text().await.unwrap_or_default();
        return Err(format!("HTTP {code}: {}", body.chars().take(300).collect::<String>()));
    }
    let v: serde_json::Value = r.json().await.map_err(|e| e.to_string())?;
    Ok(v
        .pointer("/data")
        .and_then(|x| x.as_str())
        .unwrap_or("")
        .to_string())
}

// ---------- VM lifecycle via API (clone langsung, tanpa terraform) ----------

fn auth_header(c: &ProxmoxCreds) -> String {
    format!("PVEAPIToken={}!{}={}", c.user, c.token_id, c.token_secret)
}

/// GET /cluster/nextid — VMID bebas berikutnya.
pub async fn next_vmid(c: &ProxmoxCreds) -> Result<u64, String> {
    let cli = client(c.verify_tls);
    let url = api_url(&c.base_url, "/cluster/nextid");
    let r = cli
        .get(&url)
        .header("Authorization", auth_header(c))
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if !r.status().is_success() {
        return Err(format!("HTTP {}", r.status()));
    }
    let v: serde_json::Value = r.json().await.map_err(|e| e.to_string())?;
    match v.pointer("/data") {
        Some(serde_json::Value::String(s)) => s.parse::<u64>().map_err(|e| e.to_string()),
        Some(serde_json::Value::Number(n)) => n.as_u64().ok_or_else(|| "bad nextid".to_string()),
        _ => Err("bad nextid response".to_string()),
    }
}

/// POST clone template -> VM baru. Returns UPID.
pub async fn clone_vm(
    c: &ProxmoxCreds,
    node: &str,
    template_vmid: u64,
    newid: u64,
    name: &str,
    full: bool,
    storage: Option<&str>,
) -> Result<String, String> {
    let cli = client(c.verify_tls);
    let url = api_url(&c.base_url, &format!("/nodes/{node}/qemu/{template_vmid}/clone"));
    let mut body = serde_json::json!({"newid": newid, "name": name, "full": if full { 1 } else { 0 }, "target": node});
    if let Some(s) = storage.filter(|s| !s.trim().is_empty()) {
        body["storage"] = serde_json::Value::String(s.to_string());
    }
    let r = cli
        .post(&url)
        .header("Authorization", auth_header(c))
        .json(&body)
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if !r.status().is_success() {
        let code = r.status();
        let txt = r.text().await.unwrap_or_default();
        return Err(format!("HTTP {code}: {}", txt.chars().take(300).collect::<String>()));
    }
    let v: serde_json::Value = r.json().await.map_err(|e| e.to_string())?;
    v.pointer("/data")
        .and_then(|x| x.as_str())
        .map(|s| s.to_string())
        .ok_or_else(|| "no UPID in clone response".to_string())
}

/// Status task: Ok((finished, ok, detail)).
pub async fn task_status(c: &ProxmoxCreds, node: &str, upid: &str) -> Result<(bool, bool, String), String> {
    let cli = client(c.verify_tls);
    let url = api_url(&c.base_url, &format!("/nodes/{node}/tasks/{upid}/status"));
    let r = cli
        .get(&url)
        .header("Authorization", auth_header(c))
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if !r.status().is_success() {
        return Err(format!("HTTP {}", r.status()));
    }
    let v: serde_json::Value = r.json().await.map_err(|e| e.to_string())?;
    let d = v.pointer("/data");
    let status = d.and_then(|x| x.get("status")).and_then(|x| x.as_str()).unwrap_or("");
    if status != "stopped" {
        return Ok((false, false, "running".to_string()));
    }
    let exit = d.and_then(|x| x.get("exitstatus")).and_then(|x| x.as_str()).unwrap_or("?").to_string();
    Ok((true, exit == "OK", exit))
}

/// Tunggu task selesai (poll 3s, maks ~5 mnt). Ok(false) = timeout.
pub async fn wait_task(c: &ProxmoxCreds, node: &str, upid: &str) -> Result<bool, String> {
    for _ in 0..100 {
        let (done, ok, detail) = task_status(c, node, upid).await?;
        if done {
            return if ok { Ok(true) } else { Err(format!("task gagal: {detail}")) };
        }
        tokio::time::sleep(std::time::Duration::from_secs(3)).await;
    }
    Ok(false)
}

/// PUT cloud-init config VM (ciuser/sshkeys/ipconfig/nameserver).
pub async fn set_vm_config(
    c: &ProxmoxCreds,
    node: &str,
    vmid: u64,
    ciuser: &str,
    sshkeys: Option<&str>,
    ipconfig: &str,
    nameserver: &str,
) -> Result<(), String> {
    let cli = client(c.verify_tls);
    let url = api_url(&c.base_url, &format!("/nodes/{node}/qemu/{vmid}/config"));
    let mut body = serde_json::json!({
        "ciuser": ciuser,
        "ipconfig0": ipconfig,
        "nameserver": nameserver,
    });
    if let Some(k) = sshkeys.filter(|k| !k.trim().is_empty()) {
        // API butuh URL-encoded agar newline aman.
        body["sshkeys"] = serde_json::Value::String(url_encode(k));
    }
    let r = cli
        .put(&url)
        .header("Authorization", auth_header(c))
        .json(&body)
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if !r.status().is_success() {
        let code = r.status();
        let txt = r.text().await.unwrap_or_default();
        return Err(format!("HTTP {code}: {}", txt.chars().take(300).collect::<String>()));
    }
    Ok(())
}

/// DELETE VM (harus stopped dulu — Proxmox menolak VM running).
pub async fn delete_vm(c: &ProxmoxCreds, node: &str, vmid: u64) -> Result<(), String> {
    let cli = client(c.verify_tls);
    let url = api_url(&c.base_url, &format!("/nodes/{node}/qemu/{vmid}"));
    let r = cli
        .delete(&url)
        .header("Authorization", auth_header(c))
        .query(&[("destroy-unreferenced-disks", "1"), ("purge", "1")])
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if !r.status().is_success() {
        let code = r.status();
        let txt = r.text().await.unwrap_or_default();
        return Err(format!("HTTP {code}: {}", txt.chars().take(300).collect::<String>()));
    }
    Ok(())
}

/// IP via qemu-guest-agent (network-get-interfaces). None bila agent
/// belum jawab / belum ada IPv4 non-loopback.
pub async fn vm_ip(c: &ProxmoxCreds, node: &str, vmid: u64) -> Result<Option<String>, String> {
    let cli = client(c.verify_tls);
    let url = api_url(&c.base_url, &format!("/nodes/{node}/qemu/{vmid}/agent/network-get-interfaces"));
    let r = cli
        .post(&url)
        .header("Authorization", auth_header(c))
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if !r.status().is_success() {
        return Err(format!("HTTP {}", r.status()));
    }
    let v: serde_json::Value = r.json().await.map_err(|e| e.to_string())?;
    Ok(parse_agent_ip(&v))
}

fn parse_agent_ip(v: &serde_json::Value) -> Option<String> {
    let arr = v.pointer("/data/result").and_then(|x| x.as_array())?;
    for iface in arr {
        let name = iface.get("name").and_then(|x| x.as_str()).unwrap_or("");
        if name == "lo" {
            continue;
        }
        if let Some(addrs) = iface.get("ip-addresses").and_then(|x| x.as_array()) {
            for a in addrs {
                if let Some(ip) = a.get("ip-address").and_then(|x| x.as_str()) {
                    if ip.contains(':') || ip.starts_with("127.") || ip.starts_with("169.254.") || ip.is_empty() {
                        continue;
                    }
                    return Some(ip.to_string());
                }
            }
        }
    }
    None
}

fn url_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(b as char),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vm_list_parses_and_sorts() {
        let v: serde_json::Value = serde_json::from_str(
            r#"{"data":[
                {"vmid":102,"name":"web-1-x-worker-0","status":"stopped","cpus":2,"cpu":0.0,"mem":0,"maxmem":4294967296,"uptime":0},
                {"vmid":101,"name":"web-1-x-master-0","status":"running","cpus":4,"cpu":0.05,"mem":1073741824,"maxmem":8589934592,"uptime":3600},
                {"vmid":9000,"name":"rocky-9-cloudinit","status":"stopped","template":1}
            ]}"#,
        )
        .unwrap();
        let list = parse_vm_list(&v);
        assert_eq!(list.len(), 3);
        assert_eq!(list[0].name, "rocky-9-cloudinit");
        assert!(list[0].template);
        assert_eq!(list[1].vmid, 101);
        assert_eq!(list[1].status, "running");
        assert_eq!(list[1].uptime, 3600);
    }

    #[test]
    fn agent_ip_picks_first_routable_ipv4() {
        let v: serde_json::Value = serde_json::from_str(
            r#"{"data":{"result":[
                {"name":"lo","ip-addresses":[{"ip-address":"127.0.0.1"},{"ip-address":"::1"}]},
                {"name":"eth0","ip-addresses":[{"ip-address":"fe80::1"},{"ip-address":"169.254.5.6"},{"ip-address":"192.168.1.50"}]}
            ]}}"#,
        )
        .unwrap();
        assert_eq!(parse_agent_ip(&v).as_deref(), Some("192.168.1.50"));
        let empty: serde_json::Value = serde_json::from_str(r#"{"data":{"result":[]}}"#).unwrap();
        assert!(parse_agent_ip(&empty).is_none());
    }

    #[test]
    fn url_encode_keeps_key_chars() {
        assert_eq!(url_encode("ssh-ed25519 AAAA+/=="), "ssh-ed25519%20AAAA%2B%2F%3D%3D");
        assert_eq!(url_encode("abc-_.~09"), "abc-_.~09");
    }
}
