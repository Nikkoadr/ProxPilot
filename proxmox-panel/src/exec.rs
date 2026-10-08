//! WSL + SSH executor.
//!
//! Lokal (Windows): semua perintah dijalankan lewat `wsl bash -lc "..."`.
//! Remote: `wsl ssh -o ConnectTimeout=5 -p PORT user@host "<cmd>"`.

use std::process::Command;
use std::time::{Duration, Instant};

#[derive(Debug, Clone)]
pub struct CmdResult {
    pub ok: bool,
    pub output: String,
    pub ms: u128,
}

fn decode_bytes(b: &[u8]) -> String {
    // wsl.exe outputs UTF-16LE; detect by NUL bytes.
    if b.contains(&0) {
        let u16v: Vec<u16> = b
            .chunks_exact(2)
            .map(|c| u16::from_le_bytes([c[0], c[1]]))
            .collect();
        let s = String::from_utf16_lossy(&u16v);
        // strip remaining NULs / CR
        return s.replace('\0', "").trim().to_string();
    }
    String::from_utf8_lossy(b).to_string()
}

fn run_cmd(program: &str, args: &[&str], timeout: Duration) -> CmdResult {
    let start = Instant::now();
    // Simple timeout: spawn and wait with kill_after via thread is overkill;
    // commands used here are fast (version checks, echo). Use default wait.
    let _ = timeout;
    let out = Command::new(program).args(args).output();
    let ms = start.elapsed().as_millis();
    match out {
        Ok(o) => {
            let mut s = decode_bytes(&o.stdout);
            let e = decode_bytes(&o.stderr);
            if !e.trim().is_empty() {
                s.push_str("\n[stderr]\n");
                s.push_str(&e);
            }
            CmdResult {
                ok: o.status.success(),
                output: s.trim().to_string(),
                ms,
            }
        }
        Err(e) => CmdResult {
            ok: false,
            output: format!("failed to spawn {program}: {e}"),
            ms,
        },
    }
}

/// Escape a command for `bash -lc "..."`.
fn bash_escape(cmd: &str) -> String {
    cmd.replace('\\', "\\\\").replace('"', "\\\"")
}

pub fn wsl_available() -> bool {
    let r = run_cmd("wsl", &["bash", "-lc", "echo wsl-ok"], Duration::from_secs(10));
    r.ok && r.output.contains("wsl-ok")
}

pub fn wsl_distros() -> String {
    let r = run_cmd("wsl", &["-l", "-v"], Duration::from_secs(10));
    r.output
}

pub fn run_in_wsl(bash_cmd: &str) -> CmdResult {
    let wrapped = bash_cmd.to_string();
    run_cmd("wsl", &["bash", "-lc", &wrapped], Duration::from_secs(60))
}

pub fn tool_version_local(tool: &str) -> CmdResult {
    let args: Vec<&str> = match tool {
        "terraform" => vec!["version"],
        "ansible" => vec!["--version"],
        "ssh" => vec!["-V"],
        _ => vec!["--version"],
    };
    // ssh -V writes to stderr, handled in run_cmd merge.
    let a: Vec<&str> = args;
    run_cmd(tool, &a, Duration::from_secs(10))
}

pub fn tool_version_wsl(tool: &str) -> CmdResult {
    let cmd = match tool {
        "terraform" => "terraform version 2>&1 | head -3",
        "ansible" => "ansible --version 2>&1 | head -3",
        "ssh" => "ssh -V 2>&1",
        _ => "echo unknown",
    };
    let r = run_in_wsl(cmd);
    CmdResult {
        ok: !r.output.is_empty()
            && !r.output.contains("command not found")
            && !r.output.contains("failed to spawn"),
        output: r.output,
        ms: r.ms,
    }
}

/// Test SSH ke server Proxmox *melalui WSL*.
/// Setara: `wsl ssh -o BatchMode=yes -o ConnectTimeout=8 -o StrictHostKeyChecking=accept-new -p PORT user@host "echo ssh-ok; hostname; pveversion | head -1"`.
pub fn ssh_test_via_wsl(host: &str, user: &str, port: u16) -> CmdResult {
    let remote = "echo ssh-ok; hostname; pveversion 2>/dev/null | head -1; echo ---; qm --version 2>/dev/null | head -1";
    let ssh_cmd = format!(
        "ssh -o BatchMode=yes -o ConnectTimeout=8 -o StrictHostKeyChecking=accept-new -p {} {}@{} \"{}\" 2>&1",
        port,
        shell_word(user),
        shell_word(host),
        bash_escape(remote)
    );
    let r = run_in_wsl(&ssh_cmd);
    let ok = r.ok && r.output.contains("ssh-ok");
    CmdResult {
        ok,
        output: r.output,
        ms: r.ms,
    }
}

/// Jalankan perintah remote di server Proxmox via WSL ssh. Mengembalikan output gabungan.
pub fn ssh_exec_via_wsl(host: &str, user: &str, port: u16, remote_cmd: &str) -> CmdResult {
    let ssh_cmd = format!(
        "ssh -o BatchMode=yes -o ConnectTimeout=15 -o StrictHostKeyChecking=accept-new -p {} {}@{} \"{}\" 2>&1",
        port,
        shell_word(user),
        shell_word(host),
        bash_escape(remote_cmd)
    );
    run_in_wsl(&ssh_cmd)
}

/// Test SSH langsung tanpa WSL (pakai ssh native, mis. OpenSSH bawaan Windows).
/// Setara: `ssh -o BatchMode=yes -o ConnectTimeout=8 -p PORT user@host "echo ssh-ok; ..."`.
pub fn ssh_test_native(host: &str, user: &str, port: u16) -> CmdResult {
    let remote = "echo ssh-ok; hostname; pveversion 2>/dev/null | head -1";
    let port_s = port.to_string();
    let target = format!("{user}@{host}");
    run_cmd(
        "ssh",
        &[
            "-o",
            "BatchMode=yes",
            "-o",
            "ConnectTimeout=8",
            "-o",
            "StrictHostKeyChecking=accept-new",
            "-p",
            port_s.as_str(),
            target.as_str(),
            remote,
        ],
        Duration::from_secs(30),
    )
    .map_ok_contains("ssh-ok")
}

/// Jalankan perintah remote via ssh native (tanpa WSL).
#[allow(dead_code)]
pub fn ssh_exec_native(host: &str, user: &str, port: u16, remote_cmd: &str) -> CmdResult {
    let port_s = port.to_string();
    let target = format!("{user}@{host}");
    run_cmd(
        "ssh",
        &[
            "-o",
            "BatchMode=yes",
            "-o",
            "ConnectTimeout=15",
            "-p",
            port_s.as_str(),
            target.as_str(),
            remote_cmd,
        ],
        Duration::from_secs(60),
    )
}

trait MapOk {
    fn map_ok_contains(self, needle: &str) -> Self;
}

impl MapOk for CmdResult {
    fn map_ok_contains(self, needle: &str) -> Self {
        CmdResult {
            ok: self.ok && self.output.contains(needle),
            ..self
        }
    }
}

/// Daftar key yang tersedia untuk SSH (membantu diagnosa "Permission denied").
/// via_wsl=true -> cek ~/.ssh di dalam WSL; false -> cek %USERPROFILE%\.ssh native.
pub fn ssh_keys_list(via_wsl: bool) -> String {
    if via_wsl {
        run_in_wsl("ls -la ~/.ssh/ 2>&1; echo ---; for f in ~/.ssh/id_rsa ~/.ssh/id_ed25519 ~/.ssh/id_ecdsa; do if [ -f \"$f\" ]; then echo \"== $f\"; ssh-keygen -l -f \"$f\" 2>&1; fi; done").output
    } else {
        let home = std::env::var("USERPROFILE")
            .or_else(|_| std::env::var("HOME"))
            .unwrap_or_default();
        let dir = std::path::Path::new(&home).join(".ssh");
        let mut out = format!("dir: {}\n", dir.display());
        match std::fs::read_dir(&dir) {
            Ok(entries) => {
                for e in entries.flatten() {
                    out.push_str(&format!("{}\n", e.file_name().to_string_lossy()));
                }
            }
            Err(e) => out.push_str(&format!("(cannot list: {e})")),
        }
        out
    }
}

/// Ringkasan `ssh -v` (BatchMode, tanpa password) — menunjukkan key apa yang
/// ditawarkan dan metode auth apa yang server terima. Dipakai saat test gagal.
pub fn ssh_auth_debug(host: &str, user: &str, port: u16, via_wsl: bool) -> String {
    let port_s = port.to_string();
    let target = format!("{user}@{host}");
    let raw = if via_wsl {
        let cmd = format!(
            "ssh -v -o BatchMode=yes -o ConnectTimeout=5 -o StrictHostKeyChecking=accept-new -p {} {}@{} \"exit\" 2>&1 | grep -Ei \"offering|authenticat|identity file|denied|timed out|refused|private key|Trying\" | head -20",
            port_s,
            shell_word(user),
            shell_word(host)
        );
        run_in_wsl(&cmd).output
    } else {
        run_cmd(
            "ssh",
            &[
                "-v",
                "-o",
                "BatchMode=yes",
                "-o",
                "ConnectTimeout=5",
                "-p",
                port_s.as_str(),
                target.as_str(),
                "exit",
            ],
            Duration::from_secs(20),
        )
        .output
    };
    raw.lines()
        .filter(|l| {
            let t = l.to_lowercase();
            t.contains("offering")
                || t.contains("authenticat")
                || t.contains("identity file")
                || t.contains("denied")
                || t.contains("timed out")
                || t.contains("refused")
                || t.contains("private key")
        })
        .take(20)
        .collect::<Vec<_>>()
        .join("\n")
}

fn shell_word(s: &str) -> String {
    if s.chars()
        .all(|c| c.is_alphanumeric() || c == '.' || c == '-' || c == '_' || c == '@')
    {
        s.to_string()
    } else {
        format!("'{}'", s.replace('\'', "'\\''"))
    }
}

/// Ringkasan tools untuk /api/tools.
pub fn tools_summary() -> serde_json::Value {
    let wsl = wsl_available();
    let t_local = tool_version_local("terraform");
    let a_local = tool_version_local("ansible");
    let s_local = tool_version_local("ssh");
    let (t_wsl, a_wsl, s_wsl) = if wsl {
        (
            tool_version_wsl("terraform"),
            tool_version_wsl("ansible"),
            tool_version_wsl("ssh"),
        )
    } else {
        (
            CmdResult {
                ok: false,
                output: "WSL unavailable".into(),
                ms: 0,
            },
            CmdResult {
                ok: false,
                output: "WSL unavailable".into(),
                ms: 0,
            },
            CmdResult {
                ok: false,
                output: "WSL unavailable".into(),
                ms: 0,
            },
        )
    };
    serde_json::json!({
        "wsl": { "available": wsl, "distros": wsl_distros() },
        "terraform": {
            "local": { "ok": t_local.ok, "output": first_line(&t_local.output), "ms": t_local.ms },
            "wsl": { "ok": t_wsl.ok, "output": first_line(&t_wsl.output), "ms": t_wsl.ms },
        },
        "ansible": {
            "local": { "ok": a_local.ok, "output": first_line(&a_local.output), "ms": a_local.ms },
            "wsl": { "ok": a_wsl.ok, "output": first_line(&a_wsl.output), "ms": a_wsl.ms },
        },
        "ssh": {
            "local": { "ok": true, "output": first_line(&s_local.output), "ms": s_local.ms },
            "wsl": { "ok": s_wsl.ok, "output": first_line(&s_wsl.output), "ms": s_wsl.ms },
        }
    })
}

fn first_line(s: &str) -> String {
    s.lines().next().unwrap_or("").chars().take(160).collect()
}
