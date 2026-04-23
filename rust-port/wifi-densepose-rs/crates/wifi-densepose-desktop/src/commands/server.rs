use std::collections::VecDeque;
use std::io::{BufRead, BufReader, Read};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::thread;

use serde::{Deserialize, Serialize};
use sysinfo::{Pid, ProcessesToUpdate, System};
use tauri::{AppHandle, Manager, State};

use crate::state::{AppState, ServerLogBuffer};

/// Default binary name for the sensing server.
const DEFAULT_SERVER_BIN: &str = "sensing-server";
const MAX_SERVER_LOG_LINES: usize = 500;

fn push_bounded_line(lines: &mut VecDeque<String>, line: String) {
    lines.push_back(line);
    while lines.len() > MAX_SERVER_LOG_LINES {
        lines.pop_front();
    }
}

fn spawn_log_reader<R>(reader: R, logs: Arc<Mutex<ServerLogBuffer>>, stdout: bool)
where
    R: Read + Send + 'static,
{
    thread::spawn(move || {
        let reader = BufReader::new(reader);
        for line in reader.lines() {
            let Ok(line) = line else { break };
            if let Ok(mut buffer) = logs.lock() {
                if stdout {
                    push_bounded_line(&mut buffer.stdout, line);
                } else {
                    push_bounded_line(&mut buffer.stderr, line);
                }
            }
        }
    });
}

/// Find the sensing server binary path.
///
/// Search order:
/// 1. Custom path from config.server_path
/// 2. Bundled in app resources (macOS: Contents/Resources/bin/)
/// 3. Next to the app executable
/// 4. System PATH
fn find_server_binary(app: &AppHandle, custom_path: Option<&str>) -> Result<String, String> {
    // 1. Custom path from settings
    if let Some(path) = custom_path {
        if std::path::Path::new(path).exists() {
            return Ok(path.to_string());
        }
    }

    // 2. Bundled in resources (Tauri bundles to Contents/Resources/)
    if let Ok(resource_dir) = app.path().resource_dir() {
        let bundled = resource_dir.join("bin").join(DEFAULT_SERVER_BIN);
        if bundled.exists() {
            return Ok(bundled.to_string_lossy().to_string());
        }
        // Also check directly in resources
        let direct = resource_dir.join(DEFAULT_SERVER_BIN);
        if direct.exists() {
            return Ok(direct.to_string_lossy().to_string());
        }
    }

    // 3. Next to the executable
    if let Ok(exe_path) = std::env::current_exe() {
        if let Some(exe_dir) = exe_path.parent() {
            let sibling = exe_dir.join(DEFAULT_SERVER_BIN);
            if sibling.exists() {
                return Ok(sibling.to_string_lossy().to_string());
            }
        }
    }

    // 4. Check if it's in PATH
    if let Ok(output) = Command::new("which").arg(DEFAULT_SERVER_BIN).output() {
        if output.status.success() {
            let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !path.is_empty() {
                return Ok(path);
            }
        }
    }

    Err(format!(
        "Sensing server binary '{}' not found. Please build it with: cargo build --release -p wifi-densepose-sensing-server",
        DEFAULT_SERVER_BIN
    ))
}

/// Start the sensing server as a managed child process.
///
/// The server binary is looked up in the following order:
/// 1. Settings `server_path` if set
/// 2. Bundled resource path
/// 3. Next to executable
/// 4. System PATH
#[tauri::command]
pub async fn start_server(
    app: AppHandle,
    config: ServerConfig,
    state: State<'_, AppState>,
) -> Result<ServerStartResult, String> {
    // Check if already running
    let logs = {
        let srv = state.server.lock().map_err(|e| e.to_string())?;
        if srv.running {
            return Err("Server is already running".into());
        }
        let logs = srv.logs.clone();
        if let Ok(mut buffer) = logs.lock() {
            buffer.clear();
        }
        logs
    };

    // Find server binary
    let server_path = find_server_binary(&app, config.server_path.as_deref())?;

    tracing::info!("Starting sensing server from: {}", server_path);

    // Build command with configuration
    let mut cmd = Command::new(&server_path);

    if let Some(port) = config.http_port {
        cmd.args(["--http-port", &port.to_string()]);
    }
    if let Some(port) = config.ws_port {
        cmd.args(["--ws-port", &port.to_string()]);
    }
    if let Some(port) = config.udp_port {
        cmd.args(["--udp-port", &port.to_string()]);
    }
    if let Some(port) = config.nexmon_port {
        cmd.args(["--nexmon-port", &port.to_string()]);
    }
    if let Some(ref ui_path) = config.ui_path {
        if !ui_path.trim().is_empty() {
            cmd.args(["--ui-path", ui_path]);
        }
    }
    if let Some(tick_ms) = config.tick_ms {
        cmd.args(["--tick-ms", &tick_ms.to_string()]);
    }
    if let Some(ref bind_addr) = config.bind_address {
        if !bind_addr.trim().is_empty() {
            cmd.args(["--bind-addr", bind_addr]);
        }
    }

    // Set data source (default to "auto" if not specified).
    let source = config.source.as_deref().unwrap_or("auto");
    cmd.args(["--source", source]);
    if config.pi_diag.unwrap_or(false) {
        cmd.arg("--pi-diag");
    }

    if let Some(ref path) = config.load_rvf {
        if !path.trim().is_empty() {
            cmd.args(["--load-rvf", path]);
        }
    }
    if let Some(ref path) = config.save_rvf {
        if !path.trim().is_empty() {
            cmd.args(["--save-rvf", path]);
        }
    }
    if let Some(ref path) = config.model {
        if !path.trim().is_empty() {
            cmd.args(["--model", path]);
        }
    }
    if config.progressive.unwrap_or(false) {
        cmd.arg("--progressive");
    }
    if let Some(ref path) = config.export_rvf {
        if !path.trim().is_empty() {
            cmd.args(["--export-rvf", path]);
        }
    }
    if config.train.unwrap_or(false) {
        cmd.arg("--train");
    }
    if let Some(ref dataset) = config.dataset {
        if !dataset.trim().is_empty() {
            cmd.args(["--dataset", dataset]);
        }
    }
    if let Some(ref dataset_type) = config.dataset_type {
        if !dataset_type.trim().is_empty() {
            cmd.args(["--dataset-type", dataset_type]);
        }
    }
    if let Some(epochs) = config.epochs {
        cmd.args(["--epochs", &epochs.to_string()]);
    }
    if let Some(ref checkpoint_dir) = config.checkpoint_dir {
        if !checkpoint_dir.trim().is_empty() {
            cmd.args(["--checkpoint-dir", checkpoint_dir]);
        }
    }
    if config.pretrain.unwrap_or(false) {
        cmd.arg("--pretrain");
    }
    if let Some(pretrain_epochs) = config.pretrain_epochs {
        cmd.args(["--pretrain-epochs", &pretrain_epochs.to_string()]);
    }
    if config.embed.unwrap_or(false) {
        cmd.arg("--embed");
    }
    if let Some(ref idx) = config.build_index {
        if !idx.trim().is_empty() {
            cmd.args(["--build-index", idx]);
        }
    }
    if let Some(ref node_positions) = config.node_positions {
        if !node_positions.trim().is_empty() {
            cmd.args(["--node-positions", node_positions]);
        }
    }
    if config.calibrate.unwrap_or(false) {
        cmd.arg("--calibrate");
    }
    if config.benchmark.unwrap_or(false) {
        cmd.arg("--benchmark");
    }

    // Redirect stdout/stderr to pipes for monitoring
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());

    // Spawn the child process
    let mut child = cmd.spawn()
        .map_err(|e| format!("Failed to start server: {}. Is '{}' installed?", e, server_path))?;

    let pid = child.id();

    if let Some(stdout) = child.stdout.take() {
        spawn_log_reader(stdout, logs.clone(), true);
    }
    if let Some(stderr) = child.stderr.take() {
        spawn_log_reader(stderr, logs.clone(), false);
    }

    // Store the child process in state
    {
        let mut srv = state.server.lock().map_err(|e| e.to_string())?;
        srv.running = true;
        srv.pid = Some(pid);
        srv.http_port = config.http_port;
        srv.ws_port = config.ws_port;
        srv.udp_port = config.udp_port;
        srv.nexmon_port = config.nexmon_port;
        srv.bind_address = config.bind_address.clone();
        srv.source = Some(source.to_string());
        srv.child = Some(child);
        srv.start_time = Some(std::time::Instant::now());
    }

    tracing::info!("Started sensing server with PID {}", pid);

    Ok(ServerStartResult {
        pid,
        http_port: config.http_port,
        ws_port: config.ws_port,
        udp_port: config.udp_port,
        nexmon_port: config.nexmon_port,
    })
}

/// Stop the managed sensing server process.
///
/// First attempts graceful termination (SIGTERM), then SIGKILL after timeout.
#[tauri::command]
pub async fn stop_server(state: State<'_, AppState>) -> Result<(), String> {
    // Extract child process and take ownership for killing
    let (child_id, mut child_process) = {
        let mut srv = state.server.lock().map_err(|e| e.to_string())?;
        if !srv.running {
            return Err("Server is not running".into());
        }
        let pid = srv.pid;
        let child = srv.child.take(); // Take ownership of child
        (pid, child)
    };

    let child_id = match child_id {
        Some(id) => id,
        None => return Err("No server process found".into()),
    };

    tracing::info!("Stopping sensing server with PID {}", child_id);

    // First try graceful termination via SIGTERM
    #[cfg(unix)]
    {
        unsafe {
            // Kill the process group (negative PID) to kill all children too
            let _ = libc::kill(-(child_id as i32), libc::SIGTERM);
            // Also kill the main process directly
            let _ = libc::kill(child_id as i32, libc::SIGTERM);
        }
    }

    // Wait briefly for graceful shutdown
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;

    // Check if still running
    let still_running = {
        let mut sys = System::new();
        let pid = Pid::from_u32(child_id);
        sys.refresh_processes(ProcessesToUpdate::Some(&[pid]), true);
        sys.process(pid).is_some()
    };

    // Force kill if still running
    if still_running {
        tracing::warn!("Server still running after SIGTERM, sending SIGKILL");

        #[cfg(unix)]
        {
            unsafe {
                // SIGKILL the process group and main process
                let _ = libc::kill(-(child_id as i32), libc::SIGKILL);
                let _ = libc::kill(child_id as i32, libc::SIGKILL);
            }
        }

        // Also use the child handle if available
        if let Some(ref mut child) = child_process {
            let _ = child.kill();
        }
    }

    // Wait for process to actually terminate
    if let Some(ref mut child) = child_process {
        let _ = child.wait();
    }

    // Final verification and cleanup
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    // Clear state
    {
        let mut srv = state.server.lock().map_err(|e| e.to_string())?;
        srv.running = false;
        srv.pid = None;
        srv.http_port = None;
        srv.ws_port = None;
        srv.udp_port = None;
        srv.nexmon_port = None;
        srv.bind_address = None;
        srv.source = None;
        srv.child = None;
        srv.start_time = None;
    }

    // Verify process is dead
    let still_alive = {
        let mut sys = System::new();
        let pid = Pid::from_u32(child_id);
        sys.refresh_processes(ProcessesToUpdate::Some(&[pid]), true);
        sys.process(pid).is_some()
    };

    if still_alive {
        tracing::error!("Failed to kill server process {}", child_id);
        return Err(format!("Failed to stop server process {}", child_id));
    }

    tracing::info!("Stopped sensing server");

    Ok(())
}

/// Get sensing server status including resource usage.
#[tauri::command]
pub async fn server_status(state: State<'_, AppState>) -> Result<ServerStatusResponse, String> {
    let srv = state.server.lock().map_err(|e| e.to_string())?;

    if !srv.running || srv.pid.is_none() {
        return Ok(ServerStatusResponse {
            running: false,
            pid: None,
            http_port: None,
            ws_port: None,
            udp_port: None,
            nexmon_port: None,
            memory_mb: None,
            cpu_percent: None,
            uptime_secs: None,
            source: None,
            bind_address: None,
        });
    }

    let pid = srv.pid.unwrap();
    let mut sys = System::new();
    let sysinfo_pid = Pid::from_u32(pid);
    sys.refresh_processes(ProcessesToUpdate::Some(&[sysinfo_pid]), true);

    let (memory_mb, cpu_percent) = sys.process(sysinfo_pid)
        .map(|proc| {
            let mem = proc.memory() as f64 / 1024.0 / 1024.0;
            let cpu = proc.cpu_usage();
            (Some(mem), Some(cpu))
        })
        .unwrap_or((None, None));

    // Calculate uptime if we have start time
    let uptime_secs = srv.start_time.map(|start| {
        std::time::Instant::now().duration_since(start).as_secs()
    });

    Ok(ServerStatusResponse {
        running: srv.running,
        pid: Some(pid),
        http_port: srv.http_port,
        ws_port: srv.ws_port,
        udp_port: srv.udp_port,
        nexmon_port: srv.nexmon_port,
        memory_mb,
        cpu_percent,
        uptime_secs,
        source: srv.source.clone(),
        bind_address: srv.bind_address.clone(),
    })
}

/// Restart the sensing server with the same or new configuration.
#[tauri::command]
pub async fn restart_server(
    app: AppHandle,
    config: Option<ServerConfig>,
    state: State<'_, AppState>,
) -> Result<ServerStartResult, String> {
    // Get current config if no new config provided
    let restart_config = if let Some(cfg) = config {
        cfg
    } else {
        let srv = state.server.lock().map_err(|e| e.to_string())?;
        ServerConfig {
            http_port: srv.http_port,
            ws_port: srv.ws_port,
            udp_port: srv.udp_port,
            nexmon_port: srv.nexmon_port,
            ui_path: None,
            tick_ms: None,
            bind_address: srv.bind_address.clone(),
            server_path: None,
            source: srv.source.clone(),
            pi_diag: None,
            benchmark: None,
            load_rvf: None,
            save_rvf: None,
            model: None,
            progressive: None,
            export_rvf: None,
            train: None,
            dataset: None,
            dataset_type: None,
            epochs: None,
            checkpoint_dir: None,
            pretrain: None,
            pretrain_epochs: None,
            embed: None,
            build_index: None,
            node_positions: None,
            calibrate: None,
        }
    };

    // Stop existing server
    let _ = stop_server(state.clone()).await;

    // Brief delay to ensure port is released
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;

    // Start with new config
    start_server(app, restart_config, state).await
}

/// Get server logs (last N lines from stdout/stderr).
#[tauri::command]
pub async fn server_logs(
    _lines: Option<usize>,
    state: State<'_, AppState>,
) -> Result<ServerLogsResponse, String> {
    let logs = {
        let srv = state.server.lock().map_err(|e| e.to_string())?;
        srv.logs.clone()
    };

    let lines = _lines.unwrap_or(200);
    let buffer = logs.lock().map_err(|e| e.to_string())?;
    let stdout = buffer
        .stdout
        .iter()
        .rev()
        .take(lines)
        .cloned()
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    let stderr = buffer
        .stderr
        .iter()
        .rev()
        .take(lines)
        .cloned()
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();

    Ok(ServerLogsResponse {
        truncated: buffer.stdout.len() > lines || buffer.stderr.len() > lines,
        stdout,
        stderr,
    })
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    pub http_port: Option<u16>,
    pub ws_port: Option<u16>,
    pub udp_port: Option<u16>,
    pub nexmon_port: Option<u16>,
    pub ui_path: Option<String>,
    pub tick_ms: Option<u64>,
    pub bind_address: Option<String>,
    pub server_path: Option<String>,
    /// Data source: "auto", "wifi", "esp32", "nexmon", "simulate"
    pub source: Option<String>,
    pub pi_diag: Option<bool>,
    pub benchmark: Option<bool>,
    pub load_rvf: Option<String>,
    pub save_rvf: Option<String>,
    pub model: Option<String>,
    pub progressive: Option<bool>,
    pub export_rvf: Option<String>,
    pub train: Option<bool>,
    pub dataset: Option<String>,
    pub dataset_type: Option<String>,
    pub epochs: Option<usize>,
    pub checkpoint_dir: Option<String>,
    pub pretrain: Option<bool>,
    pub pretrain_epochs: Option<usize>,
    pub embed: Option<bool>,
    pub build_index: Option<String>,
    pub node_positions: Option<String>,
    pub calibrate: Option<bool>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ServerStartResult {
    pub pid: u32,
    pub http_port: Option<u16>,
    pub ws_port: Option<u16>,
    pub udp_port: Option<u16>,
    pub nexmon_port: Option<u16>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ServerStatusResponse {
    pub running: bool,
    pub pid: Option<u32>,
    pub http_port: Option<u16>,
    pub ws_port: Option<u16>,
    pub udp_port: Option<u16>,
    pub nexmon_port: Option<u16>,
    pub memory_mb: Option<f64>,
    pub cpu_percent: Option<f32>,
    pub uptime_secs: Option<u64>,
    pub source: Option<String>,
    pub bind_address: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ServerLogsResponse {
    pub stdout: Vec<String>,
    pub stderr: Vec<String>,
    pub truncated: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_server_config_default() {
        let config = ServerConfig {
            http_port: Some(8080),
            ws_port: Some(8765),
            udp_port: Some(5005),
            nexmon_port: Some(5500),
            ui_path: None,
            tick_ms: None,
            bind_address: None,
            server_path: None,
            source: Some("simulate".to_string()),
            pi_diag: None,
            benchmark: None,
            load_rvf: None,
            save_rvf: None,
            model: None,
            progressive: None,
            export_rvf: None,
            train: None,
            dataset: None,
            dataset_type: None,
            epochs: None,
            checkpoint_dir: None,
            pretrain: None,
            pretrain_epochs: None,
            embed: None,
            build_index: None,
            node_positions: None,
            calibrate: None,
        };

        assert_eq!(config.http_port, Some(8080));
        assert_eq!(config.ws_port, Some(8765));
        assert_eq!(config.nexmon_port, Some(5500));
    }
}
