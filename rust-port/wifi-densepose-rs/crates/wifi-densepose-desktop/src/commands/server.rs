use std::collections::VecDeque;
use std::io::{BufRead, BufReader, Read};
use std::net::TcpStream;
use std::process::{Command, ExitStatus, Stdio};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use sysinfo::{Pid, ProcessesToUpdate, System};
use tauri::{AppHandle, Manager, State};

use crate::commands::auth::AuthManager;
use crate::commands::guard;
use crate::commands::settings::AppSettings;
use crate::state::{AppState, ServerLogBuffer, ServerState};

/// Default binary name for the sensing server.
#[cfg(windows)]
const DEFAULT_SERVER_BIN: &str = "sensing-server.exe";
#[cfg(not(windows))]
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

/// Human-readable exit status for surfacing crashes in the UI.
fn format_exit_status(status: &ExitStatus) -> String {
    match status.code() {
        Some(code) => format!("exit code {}", code),
        None => "terminated by signal".to_string(),
    }
}

/// Clear runtime fields after a confirmed process death.
/// Keeps `last_config` (for restart) and `last_exit` (for the UI).
fn clear_runtime_fields(srv: &mut ServerState) {
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
    srv.external = false;
}

/// Detect a dead managed child and reconcile `ServerState`.
///
/// Uses `Child::try_wait()` when the handle is available, falling back to a
/// sysinfo PID probe otherwise. When the child is gone the state is reset
/// (`running = false`, `child = None`) and the exit status is recorded in
/// `last_exit` so the UI can show a "crashed" indicator.
fn reconcile_liveness(srv: &mut ServerState) {
    if !srv.running || srv.external {
        return;
    }

    let exit = match srv.child.as_mut() {
        Some(child) => match child.try_wait() {
            Ok(Some(status)) => Some(format_exit_status(&status)),
            Ok(None) => None,
            Err(e) => Some(format!("unknown (try_wait failed: {})", e)),
        },
        // Running with no child handle — fall back to a PID probe.
        None => match srv.pid {
            Some(pid) => {
                let mut sys = System::new();
                let sysinfo_pid = Pid::from_u32(pid);
                sys.refresh_processes(ProcessesToUpdate::Some(&[sysinfo_pid]), true);
                if sys.process(sysinfo_pid).is_some() {
                    None
                } else {
                    Some("unknown (process no longer exists)".to_string())
                }
            }
            None => Some("unknown (no pid recorded)".to_string()),
        },
    };

    if let Some(exit) = exit {
        tracing::warn!(
            "Sensing server (pid {:?}) died unexpectedly: {}",
            srv.pid,
            exit
        );
        srv.last_exit = Some(exit);
        clear_runtime_fields(srv);
    }
}

/// Lightweight watcher that flips `ServerState` when the managed child exits,
/// so the UI sees "crashed" without waiting for the next status poll.
fn spawn_child_watcher(app: AppHandle, watched_pid: u32) {
    thread::spawn(move || loop {
        thread::sleep(Duration::from_secs(1));
        let state = app.state::<AppState>();
        let Ok(mut srv) = state.server.lock() else {
            break;
        };
        // Stopped, restarted with a different process, or replaced by an
        // adopted external server — this watcher is obsolete.
        if !srv.running || srv.external || srv.pid != Some(watched_pid) {
            break;
        }
        match srv.child.as_mut() {
            Some(child) => match child.try_wait() {
                Ok(Some(status)) => {
                    let exit = format_exit_status(&status);
                    tracing::warn!(
                        "Sensing server (pid {}) exited unexpectedly: {}",
                        watched_pid,
                        exit
                    );
                    srv.last_exit = Some(exit);
                    clear_runtime_fields(&mut srv);
                    break;
                }
                Ok(None) => {}
                Err(e) => {
                    tracing::warn!(
                        "Server watcher try_wait failed for pid {}: {}",
                        watched_pid,
                        e
                    );
                    break;
                }
            },
            // Handle taken elsewhere (e.g. stop in progress) — stand down.
            None => break,
        }
    });
}

/// Check whether something already listens on the configured HTTP address.
/// Used before auto-start to avoid spawning a duplicate server.
pub fn tcp_port_in_use(bind_address: Option<&str>, port: u16) -> bool {
    use std::net::ToSocketAddrs;
    let host = match bind_address.map(str::trim) {
        Some(h) if !h.is_empty() && h != "0.0.0.0" => h.to_string(),
        _ => "127.0.0.1".to_string(),
    };
    let addr = format!("{}:{}", host, port);
    match addr.to_socket_addrs() {
        Ok(mut addrs) => addrs.next().is_some_and(|a| {
            TcpStream::connect_timeout(&a, Duration::from_millis(300)).is_ok()
        }),
        Err(_) => false,
    }
}

/// Record an already-listening external sensing server as adopted.
/// We did not spawn it, so we must never kill it — stop merely releases
/// the adoption.
pub fn adopt_external_server(state: &AppState, config: &ServerConfig) {
    if let Ok(mut srv) = state.server.lock() {
        srv.running = true;
        srv.external = true;
        srv.pid = None;
        srv.child = None;
        srv.http_port = config.http_port;
        srv.ws_port = config.ws_port;
        srv.udp_port = config.udp_port;
        srv.nexmon_port = config.nexmon_port;
        srv.bind_address = config.bind_address.clone();
        srv.source = config.source.clone();
        srv.start_time = Some(std::time::Instant::now());
        srv.last_config = Some(config.clone());
        srv.last_exit = None;
    }
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
    #[cfg(windows)]
    let path_lookup = "where";
    #[cfg(not(windows))]
    let path_lookup = "which";
    if let Ok(output) = Command::new(path_lookup).arg(DEFAULT_SERVER_BIN).output() {
        if output.status.success() {
            let path = String::from_utf8_lossy(&output.stdout)
                .lines()
                .next()
                .unwrap_or_default()
                .trim()
                .to_string();
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
    access_token: String,
    app: AppHandle,
    config: ServerConfig,
    state: State<'_, AppState>,
    auth: State<'_, Arc<AuthManager>>,
) -> Result<ServerStartResult, String> {
    guard::require_auth(&access_token, &auth)?;
    start_server_impl(&app, &config, state.inner()).await
}

/// Start the sensing server process. Shared by the authenticated
/// `start_server` command and the application-startup auto-start.
pub async fn start_server_impl(
    app: &AppHandle,
    config: &ServerConfig,
    state: &AppState,
) -> Result<ServerStartResult, String> {
    // Check if already running (reconciling first so a crashed child does
    // not permanently block restarts).
    let logs = {
        let mut srv = state.server.lock().map_err(|e| e.to_string())?;
        reconcile_liveness(&mut srv);
        if srv.running {
            return Err(if srv.external {
                "An external sensing server is already running (adopted)".into()
            } else {
                "Server is already running".into()
            });
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
        srv.last_config = Some(config.clone());
        srv.last_exit = None;
        srv.external = false;
    }

    // Watch for unexpected child exit so the state flips promptly.
    spawn_child_watcher(app.clone(), pid);

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
pub async fn stop_server(
    access_token: String,
    state: State<'_, AppState>,
    auth: State<'_, Arc<AuthManager>>,
) -> Result<(), String> {
    guard::require_auth(&access_token, &auth)?;
    stop_server_impl(state.inner()).await
}

/// Stop the sensing server process. Shared by the authenticated
/// `stop_server` command and internal restart handling.
pub async fn stop_server_impl(state: &AppState) -> Result<(), String> {
    // Extract child process and take ownership for killing
    let (child_id, mut child_process) = {
        let mut srv = state.server.lock().map_err(|e| e.to_string())?;
        reconcile_liveness(&mut srv);
        if !srv.running {
            return Err("Server is not running".into());
        }
        if srv.external {
            // Adopted external server — never kill it; just release adoption.
            tracing::info!("Releasing adopted external sensing server (not killing it)");
            clear_runtime_fields(&mut srv);
            srv.last_exit = None;
            return Ok(());
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

    // Final verification
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    // Verify process is dead BEFORE clearing state
    let still_alive = {
        let mut sys = System::new();
        let pid = Pid::from_u32(child_id);
        sys.refresh_processes(ProcessesToUpdate::Some(&[pid]), true);
        sys.process(pid).is_some()
    };

    if still_alive {
        tracing::error!("Failed to kill server process {}", child_id);
        // Restore the child handle so a later stop/reconcile can retry;
        // running/pid were never cleared.
        if let Ok(mut srv) = state.server.lock() {
            srv.child = child_process;
        }
        return Err(format!("Failed to stop server process {}", child_id));
    }

    // Clear state only after confirmed process death
    {
        let mut srv = state.server.lock().map_err(|e| e.to_string())?;
        clear_runtime_fields(&mut srv);
        // Clean user-requested stop — not a crash.
        srv.last_exit = None;
    }

    tracing::info!("Stopped sensing server");

    Ok(())
}

/// Get sensing server status including resource usage.
#[tauri::command]
pub async fn server_status(
    access_token: String,
    state: State<'_, AppState>,
    auth: State<'_, Arc<AuthManager>>,
) -> Result<ServerStatusResponse, String> {
    guard::require_auth(&access_token, &auth)?;
    let mut srv = state.server.lock().map_err(|e| e.to_string())?;

    // Detect a crashed child before reporting — resets state when dead and
    // records the exit status so the UI can show "crashed".
    reconcile_liveness(&mut srv);

    if !srv.running {
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
            external: false,
            last_exit_status: srv.last_exit.clone(),
        });
    }

    // Resource usage only applies to a managed child with a known PID.
    let (memory_mb, cpu_percent) = match srv.pid {
        Some(pid) => {
            let mut sys = System::new();
            let sysinfo_pid = Pid::from_u32(pid);
            sys.refresh_processes(ProcessesToUpdate::Some(&[sysinfo_pid]), true);
            sys.process(sysinfo_pid)
                .map(|proc| {
                    let mem = proc.memory() as f64 / 1024.0 / 1024.0;
                    let cpu = proc.cpu_usage();
                    (Some(mem), Some(cpu))
                })
                .unwrap_or((None, None))
        }
        None => (None, None),
    };

    // Calculate uptime if we have start time
    let uptime_secs = srv.start_time.map(|start| {
        std::time::Instant::now().duration_since(start).as_secs()
    });

    Ok(ServerStatusResponse {
        running: srv.running,
        pid: srv.pid,
        http_port: srv.http_port,
        ws_port: srv.ws_port,
        udp_port: srv.udp_port,
        nexmon_port: srv.nexmon_port,
        memory_mb,
        cpu_percent,
        uptime_secs,
        source: srv.source.clone(),
        bind_address: srv.bind_address.clone(),
        external: srv.external,
        last_exit_status: srv.last_exit.clone(),
    })
}

/// Restart the sensing server with the same or new configuration.
#[tauri::command]
pub async fn restart_server(
    access_token: String,
    app: AppHandle,
    config: Option<ServerConfig>,
    state: State<'_, AppState>,
    auth: State<'_, Arc<AuthManager>>,
) -> Result<ServerStartResult, String> {
    guard::require_auth(&access_token, &auth)?;
    // Baseline is the full config used for the last start (so no fields are
    // lost across restarts); fall back to the current runtime fields for
    // state predating last_config tracking.
    let baseline = {
        let srv = state.server.lock().map_err(|e| e.to_string())?;
        srv.last_config.clone().unwrap_or(ServerConfig {
            http_port: srv.http_port,
            ws_port: srv.ws_port,
            udp_port: srv.udp_port,
            nexmon_port: srv.nexmon_port,
            bind_address: srv.bind_address.clone(),
            source: srv.source.clone(),
            ..Default::default()
        })
    };

    // Any explicit overrides win field-by-field over the baseline.
    let restart_config = match config {
        Some(overrides) => overrides.merged_over(baseline),
        None => baseline,
    };

    // Stop if running; propagate real failures (e.g. unkillable process),
    // only tolerating "not running".
    match stop_server_impl(state.inner()).await {
        Ok(()) => {}
        Err(e) if e == "Server is not running" => {}
        Err(e) => return Err(format!("Restart aborted, failed to stop server: {}", e)),
    }

    // Wait a moment for port to release (async — don't block the runtime)
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;

    // Start with config
    start_server_impl(&app, &restart_config, state.inner()).await
}

/// Get server logs (last N lines from stdout/stderr).
#[tauri::command]
pub async fn server_logs(
    access_token: String,
    _lines: Option<usize>,
    state: State<'_, AppState>,
    auth: State<'_, Arc<AuthManager>>,
) -> Result<ServerLogsResponse, String> {
    guard::require_auth(&access_token, &auth)?;
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

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
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

/// Convert a persisted settings string to an optional CLI value
/// (empty / whitespace-only means "not set").
fn non_empty(s: &str) -> Option<String> {
    let trimmed = s.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

impl ServerConfig {
    /// Fallback config mirroring the sensing-server binary's own defaults
    /// (HTTP 4000, WS 4001, UDP 5005, Nexmon 5500). TCP ports live in the 4000
    /// range to avoid colliding with common local dev stacks on 8080. UDP ports
    /// stay at 5005/5500 — the ESP32/Pi firmware is provisioned to stream there.
    /// Ports must be explicit so the frontend can read them back from status.
    pub fn with_default_ports() -> Self {
        ServerConfig {
            http_port: Some(4000),
            ws_port: Some(4001),
            udp_port: Some(5005),
            nexmon_port: Some(5500),
            ..Default::default()
        }
    }

    /// Build a runtime config from persisted `AppSettings`.
    ///
    /// Only fields safe for unattended auto-start are mapped: ports, source,
    /// bind address, tick, UI path, and model/RVF/node-position paths.
    /// One-shot operations (train, pretrain, benchmark, embed, export,
    /// build-index) are intentionally left off — they must be explicitly
    /// requested through the Sensing page.
    pub fn from_settings(s: &AppSettings) -> Self {
        ServerConfig {
            http_port: Some(s.server_http_port),
            ws_port: Some(s.server_ws_port),
            udp_port: Some(s.server_udp_port),
            nexmon_port: Some(s.server_nexmon_port),
            ui_path: non_empty(&s.ui_path),
            tick_ms: Some(s.server_tick_ms),
            bind_address: non_empty(&s.bind_address),
            source: non_empty(&s.server_source),
            pi_diag: Some(s.server_pi_diag),
            model: non_empty(&s.server_model_path),
            load_rvf: non_empty(&s.server_load_rvf_path),
            save_rvf: non_empty(&s.server_save_rvf_path),
            progressive: Some(s.server_progressive),
            node_positions: non_empty(&s.server_node_positions),
            calibrate: Some(s.server_calibrate_on_boot),
            ..Default::default()
        }
    }

    /// Field-wise merge: values set in `self` win; `None` fields fall back
    /// to `base`. Used by restart so partial overrides don't lose the rest
    /// of the previous configuration.
    pub fn merged_over(self, base: ServerConfig) -> ServerConfig {
        ServerConfig {
            http_port: self.http_port.or(base.http_port),
            ws_port: self.ws_port.or(base.ws_port),
            udp_port: self.udp_port.or(base.udp_port),
            nexmon_port: self.nexmon_port.or(base.nexmon_port),
            ui_path: self.ui_path.or(base.ui_path),
            tick_ms: self.tick_ms.or(base.tick_ms),
            bind_address: self.bind_address.or(base.bind_address),
            server_path: self.server_path.or(base.server_path),
            source: self.source.or(base.source),
            pi_diag: self.pi_diag.or(base.pi_diag),
            benchmark: self.benchmark.or(base.benchmark),
            load_rvf: self.load_rvf.or(base.load_rvf),
            save_rvf: self.save_rvf.or(base.save_rvf),
            model: self.model.or(base.model),
            progressive: self.progressive.or(base.progressive),
            export_rvf: self.export_rvf.or(base.export_rvf),
            train: self.train.or(base.train),
            dataset: self.dataset.or(base.dataset),
            dataset_type: self.dataset_type.or(base.dataset_type),
            epochs: self.epochs.or(base.epochs),
            checkpoint_dir: self.checkpoint_dir.or(base.checkpoint_dir),
            pretrain: self.pretrain.or(base.pretrain),
            pretrain_epochs: self.pretrain_epochs.or(base.pretrain_epochs),
            embed: self.embed.or(base.embed),
            build_index: self.build_index.or(base.build_index),
            node_positions: self.node_positions.or(base.node_positions),
            calibrate: self.calibrate.or(base.calibrate),
        }
    }
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
    /// True when the server was adopted (already listening at startup)
    /// rather than spawned by the desktop app.
    pub external: bool,
    /// Exit status of the last managed child that died unexpectedly
    /// (None while running or after a clean stop).
    pub last_exit_status: Option<String>,
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

    #[test]
    fn test_with_default_ports_mirrors_server_defaults() {
        let config = ServerConfig::with_default_ports();
        assert_eq!(config.http_port, Some(4000));
        assert_eq!(config.ws_port, Some(4001));
        assert_eq!(config.udp_port, Some(5005));
        assert_eq!(config.nexmon_port, Some(5500));
        assert_eq!(config.source, None);
        assert_eq!(config.train, None);
    }

    #[test]
    fn test_from_settings_maps_ports_paths_and_source() {
        let settings = AppSettings {
            server_http_port: 9090,
            server_ws_port: 9765,
            server_udp_port: 6005,
            server_nexmon_port: 6500,
            bind_address: "0.0.0.0".into(),
            server_source: "esp32".into(),
            server_tick_ms: 50,
            server_model_path: "C:/models/pose.onnx".into(),
            server_node_positions: "C:/cfg/positions.json".into(),
            ..Default::default()
        };
        let config = ServerConfig::from_settings(&settings);
        assert_eq!(config.http_port, Some(9090));
        assert_eq!(config.ws_port, Some(9765));
        assert_eq!(config.udp_port, Some(6005));
        assert_eq!(config.nexmon_port, Some(6500));
        assert_eq!(config.bind_address.as_deref(), Some("0.0.0.0"));
        assert_eq!(config.source.as_deref(), Some("esp32"));
        assert_eq!(config.tick_ms, Some(50));
        assert_eq!(config.model.as_deref(), Some("C:/models/pose.onnx"));
        assert_eq!(
            config.node_positions.as_deref(),
            Some("C:/cfg/positions.json")
        );
        // One-shot operations must never be enabled by auto-start.
        assert_eq!(config.train, None);
        assert_eq!(config.benchmark, None);
        assert_eq!(config.export_rvf, None);
    }

    #[test]
    fn test_from_settings_empty_strings_become_none() {
        let settings = AppSettings::default();
        let config = ServerConfig::from_settings(&settings);
        assert_eq!(config.model, None);
        assert_eq!(config.load_rvf, None);
        assert_eq!(config.save_rvf, None);
        assert_eq!(config.node_positions, None);
        assert_eq!(config.ui_path, None);
        // Non-empty defaults survive.
        assert_eq!(config.bind_address.as_deref(), Some("127.0.0.1"));
        assert_eq!(config.source.as_deref(), Some("auto"));
    }

    #[test]
    fn test_merged_over_prefers_overrides_and_keeps_baseline() {
        let baseline = ServerConfig {
            http_port: Some(8080),
            ws_port: Some(8765),
            source: Some("esp32".into()),
            model: Some("old.onnx".into()),
            node_positions: Some("positions.json".into()),
            ..Default::default()
        };
        let overrides = ServerConfig {
            source: Some("nexmon".into()),
            http_port: Some(9090),
            ..Default::default()
        };
        let merged = overrides.merged_over(baseline);
        // Overrides win.
        assert_eq!(merged.http_port, Some(9090));
        assert_eq!(merged.source.as_deref(), Some("nexmon"));
        // Baseline fields are not lost.
        assert_eq!(merged.ws_port, Some(8765));
        assert_eq!(merged.model.as_deref(), Some("old.onnx"));
        assert_eq!(merged.node_positions.as_deref(), Some("positions.json"));
    }

    #[test]
    fn test_non_empty_trims_and_filters() {
        assert_eq!(non_empty(""), None);
        assert_eq!(non_empty("   "), None);
        assert_eq!(non_empty(" x "), Some("x".to_string()));
    }

    #[test]
    fn test_tcp_port_in_use_detects_listener() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        assert!(tcp_port_in_use(None, port));
        assert!(tcp_port_in_use(Some("127.0.0.1"), port));
        // "0.0.0.0" bind address probes loopback.
        assert!(tcp_port_in_use(Some("0.0.0.0"), port));
        drop(listener);
    }

    #[test]
    fn test_tcp_port_in_use_invalid_host_is_false() {
        assert!(!tcp_port_in_use(Some("not a hostname !!"), 8080));
    }

    #[test]
    fn test_reconcile_liveness_flags_dead_child() {
        let mut srv = ServerState::default();
        // Spawn a short-lived process and wait for it to exit.
        #[cfg(windows)]
        let mut cmd = {
            let mut c = Command::new("cmd");
            c.args(["/C", "exit", "3"]);
            c
        };
        #[cfg(not(windows))]
        let mut cmd = {
            let mut c = Command::new("sh");
            c.args(["-c", "exit 3"]);
            c
        };
        let mut child = cmd.spawn().expect("spawn test child");
        let status = child.wait().expect("wait test child");
        assert_eq!(status.code(), Some(3));

        srv.running = true;
        srv.pid = Some(child.id());
        srv.child = Some(child);
        reconcile_liveness(&mut srv);

        assert!(!srv.running);
        assert!(srv.child.is_none());
        assert!(srv.pid.is_none());
        assert_eq!(srv.last_exit.as_deref(), Some("exit code 3"));
    }

    #[test]
    fn test_reconcile_liveness_skips_external() {
        let mut srv = ServerState::default();
        srv.running = true;
        srv.external = true;
        reconcile_liveness(&mut srv);
        assert!(srv.running);
        assert!(srv.external);
    }
}
