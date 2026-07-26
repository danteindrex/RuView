//! Raspberry Pi node management over SSH (backend "C"). Shells out to the system
//! `ssh` client. Covers probe / service control / CSI health. The full Nexmon
//! install (a long, reboot-driven, 400-line embedded-script flow) stays in the
//! desktop wizard / `ruview_pi_files/nexmon_setup_auto.sh` — not duplicated here.

use anyhow::{Context, Result};
use serde_json::{json, Value};
use std::process::Command;

/// Run a command on `host` over SSH. Returns (combined output, exit code).
fn ssh(host: &str, cmd: &str) -> Result<(String, i32)> {
    let out = Command::new("ssh")
        .args([
            "-o", "BatchMode=yes",
            "-o", "ConnectTimeout=10",
            "-o", "StrictHostKeyChecking=accept-new",
            host, cmd,
        ])
        .output()
        .context("running ssh (is an ssh client installed and a key set up?)")?;
    let mut text = String::from_utf8_lossy(&out.stdout).to_string();
    text.push_str(&String::from_utf8_lossy(&out.stderr));
    Ok((text, out.status.code().unwrap_or(-1)))
}

pub fn probe(host: &str) -> Result<Value> {
    let (out, code) = ssh(
        host,
        "echo MODEL=$(cat /proc/device-tree/model 2>/dev/null | tr -d '\\0'); \
         echo KERNEL=$(uname -r); echo UP=$(uptime -p)",
    )?;
    if code != 0 {
        anyhow::bail!("ssh {host} failed: {}", out.trim());
    }
    let mut info = json!({ "host": host, "reachable": true });
    for line in out.lines() {
        if let Some((k, v)) = line.split_once('=') {
            info[k.to_lowercase()] = json!(v.trim());
        }
    }
    Ok(info)
}

pub fn service(host: &str, name: &str, action: &str) -> Result<Value> {
    let cmd = match action {
        "status" => format!("systemctl is-active {name}; systemctl is-enabled {name}"),
        a => format!("sudo systemctl {a} {name} && echo OK"),
    };
    let (out, code) = ssh(host, &cmd)?;
    Ok(json!({ "host": host, "service": name, "action": action, "ok": code == 0, "output": out.trim() }))
}

pub fn health(host: &str, service_name: &str) -> Result<Value> {
    // Is the agent running, and is a CSI UDP stream leaving the Pi?
    let cmd = format!(
        "echo SERVICE=$(systemctl is-active {service_name} 2>/dev/null); \
         echo MONITOR=$(iw dev 2>/dev/null | grep -c monitor)"
    );
    let (out, _) = ssh(host, &cmd)?;
    let mut info = json!({ "host": host });
    for line in out.lines() {
        if let Some((k, v)) = line.split_once('=') {
            info[k.to_lowercase()] = json!(v.trim());
        }
    }
    Ok(info)
}
