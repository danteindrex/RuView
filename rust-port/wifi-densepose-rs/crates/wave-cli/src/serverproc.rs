//! `wave-cli server start|stop|restart|logs` — manage the sensing-server child
//! process (backend "B"). Finds the bundled `sensing-server` binary next to
//! wave-cli (installer layout) or on PATH, spawns it detached with output to a
//! log file, and stops it by process name.

use anyhow::{Context, Result};
use std::path::PathBuf;
use std::process::Command;

fn bin_name() -> &'static str {
    if cfg!(windows) { "sensing-server.exe" } else { "sensing-server" }
}

/// Locate the sensing-server binary: next to wave-cli, then PATH.
fn server_bin() -> Option<PathBuf> {
    let name = bin_name();
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let sib = dir.join(name);
            if sib.exists() {
                return Some(sib);
            }
        }
    }
    let probe = if cfg!(windows) { "where" } else { "which" };
    if let Ok(o) = Command::new(probe).arg(name).output() {
        if o.status.success() {
            let p = String::from_utf8_lossy(&o.stdout)
                .lines()
                .next()
                .unwrap_or("")
                .trim()
                .to_string();
            if !p.is_empty() {
                return Some(PathBuf::from(p));
            }
        }
    }
    None
}

/// Log file the spawned server writes to (tailed by `server logs`).
pub fn log_path() -> PathBuf {
    let base = std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    let dir = base.join("RuView").join("wave-cli");
    let _ = std::fs::create_dir_all(&dir);
    dir.join("server.log")
}

/// Spawn the sensing-server detached; returns its PID.
pub fn start(http_port: u16, source: &str) -> Result<u32> {
    let bin = server_bin()
        .context("sensing-server binary not found (expected next to wave-cli or on PATH)")?;
    let out = std::fs::File::create(log_path())?;
    let err = out.try_clone()?;
    let mut cmd = Command::new(&bin);
    cmd.args(["--http-port", &http_port.to_string(), "--source", source])
        .stdout(out)
        .stderr(err);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0000_0008); // DETACHED_PROCESS — survives the CLI exiting.
    }
    let child = cmd.spawn().context("spawning sensing-server")?;
    Ok(child.id())
}

/// Stop any running sensing-server process(es).
pub fn stop() -> Result<()> {
    #[cfg(windows)]
    let r = Command::new("taskkill")
        .args(["/IM", "sensing-server.exe", "/F"])
        .output();
    #[cfg(not(windows))]
    let r = Command::new("pkill").args(["-f", "sensing-server"]).output();
    r.context("stopping sensing-server")?;
    Ok(())
}

/// Last `lines` of the server log.
pub fn logs(lines: usize) -> Result<String> {
    let content = std::fs::read_to_string(log_path()).unwrap_or_default();
    let all: Vec<&str> = content.lines().collect();
    let start = all.len().saturating_sub(lines);
    Ok(all[start..].join("\n"))
}
