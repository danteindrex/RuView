use serde::{Deserialize, Serialize};
use std::process::Stdio;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::Command;
use tauri::{AppHandle, Emitter, State};

use crate::commands::auth::AuthManager;
use crate::commands::guard;

const DEFAULT_SERVICE_NAME: &str = "ruview-pi-node-agent";
const DEFAULT_BINARY_PATH: &str = "/usr/local/bin/wifi-densepose-pi-node-agent";
const DEFAULT_ENV_PATH: &str = "/etc/ruview/pi-node-agent.env";

/// The full Nexmon CSI installer + the monitor-mode boot restore script,
/// embedded so the desktop app is self-contained (no dependency on the repo
/// tree at runtime). Line endings are normalized to LF before upload — a
/// Windows checkout may carry CRLF, which bash on the Pi would choke on.
const NEXMON_SETUP_SH: &str = include_str!("../../../../../../ruview_pi_files/nexmon_setup_auto.sh");
const NEXMON_STARTUP_SH: &str = include_str!("../../../../../../ruview_pi_files/nexmon_startup.sh");

/// Marker the installer script prints immediately before `sudo reboot`. Used to
/// distinguish an intentional mid-install reboot (success) from an SSH failure.
const NEXMON_REBOOT_MARKER: &str = "RUVIEW_NEXMON_REBOOT";

/// systemd unit name for the boot-time monitor-mode restore.
const NEXMON_STARTUP_SERVICE: &str = "ruview-nexmon-startup";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PiNodeTarget {
    pub host: String,
    pub user: Option<String>,
    pub port: Option<u16>,
    pub identity_file: Option<String>,
    pub connect_timeout_secs: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PiAgentConfig {
    pub listen: String,
    pub aggregator: String,
    pub node_base: u8,
    pub tier: u8,
    pub default_rssi: i8,
    pub noise_floor: i8,
    pub mmwave_mock: bool,
    pub enable_wasm: bool,
    pub wasm_path: Option<String>,
    pub wasm_module_id: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PiServiceAction {
    Start,
    Stop,
    Restart,
    Status,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PiNodeInstallRequest {
    pub target: PiNodeTarget,
    pub config: PiAgentConfig,
    pub service_name: Option<String>,
    pub binary_path: Option<String>,
    pub env_path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PiNodeConfigRequest {
    pub target: PiNodeTarget,
    pub config: PiAgentConfig,
    pub env_path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PiNodeServiceRequest {
    pub target: PiNodeTarget,
    pub action: PiServiceAction,
    pub service_name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PiNodeBuildRequest {
    pub workspace_path: Option<String>,
    pub target_triple: Option<String>,
    pub release: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PiNodeDeployBinaryRequest {
    pub target: PiNodeTarget,
    pub local_binary_path: String,
    pub remote_binary_path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PiNodePrereqRequest {
    pub target: PiNodeTarget,
    pub install_packages: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PiNodeHealthRequest {
    pub target: PiNodeTarget,
    pub nexmon_port: Option<u16>,
    pub capture_seconds: Option<u64>,
    pub service_name: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PiNodeCommandResult {
    pub success: bool,
    pub command: String,
    pub stdout: String,
    pub stderr: String,
    pub exit_code: Option<i32>,
}

fn validate_target(target: &PiNodeTarget) -> Result<(), String> {
    if target.host.trim().is_empty() {
        return Err("Pi host is required".into());
    }
    if target.host.contains(['\n', '\r']) {
        return Err("Pi host cannot contain newlines".into());
    }
    if let Some(user) = &target.user {
        if user.contains(['\n', '\r', '@']) {
            return Err("Pi user cannot contain newlines or '@'".into());
        }
    }
    Ok(())
}

fn remote_addr(target: &PiNodeTarget) -> String {
    match target.user.as_deref().filter(|value| !value.trim().is_empty()) {
        Some(user) => format!("{}@{}", user.trim(), target.host.trim()),
        None => target.host.trim().to_string(),
    }
}

fn ssh_base(target: &PiNodeTarget) -> Command {
    let mut cmd = Command::new("ssh");
    cmd.arg("-o").arg("BatchMode=yes");
    cmd.arg("-o")
        .arg(format!("ConnectTimeout={}", target.connect_timeout_secs.unwrap_or(8)));
    if let Some(port) = target.port {
        cmd.arg("-p").arg(port.to_string());
    }
    if let Some(identity_file) = target.identity_file.as_deref().filter(|value| !value.trim().is_empty()) {
        cmd.arg("-i").arg(identity_file.trim());
    }
    cmd.arg(remote_addr(target));
    cmd
}

fn scp_base(target: &PiNodeTarget) -> Command {
    let mut cmd = Command::new("scp");
    cmd.arg("-o").arg("BatchMode=yes");
    cmd.arg("-o")
        .arg(format!("ConnectTimeout={}", target.connect_timeout_secs.unwrap_or(8)));
    if let Some(port) = target.port {
        cmd.arg("-P").arg(port.to_string());
    }
    if let Some(identity_file) = target.identity_file.as_deref().filter(|value| !value.trim().is_empty()) {
        cmd.arg("-i").arg(identity_file.trim());
    }
    cmd
}

async fn run_ssh(target: &PiNodeTarget, remote_command: &str) -> Result<PiNodeCommandResult, String> {
    validate_target(target)?;
    let mut cmd = ssh_base(target);
    cmd.arg(remote_command);
    let output = cmd
        .output()
        .await
        .map_err(|e| format!("Failed to run ssh. Is OpenSSH installed and available in PATH? {e}"))?;

    Ok(PiNodeCommandResult {
        success: output.status.success(),
        command: remote_command.to_string(),
        stdout: String::from_utf8_lossy(&output.stdout).to_string(),
        stderr: String::from_utf8_lossy(&output.stderr).to_string(),
        exit_code: output.status.code(),
    })
}

async fn run_local_command(mut cmd: Command, display_command: String) -> Result<PiNodeCommandResult, String> {
    let output = cmd
        .output()
        .await
        .map_err(|e| format!("Failed to run command: {e}"))?;

    Ok(PiNodeCommandResult {
        success: output.status.success(),
        command: display_command,
        stdout: String::from_utf8_lossy(&output.stdout).to_string(),
        stderr: String::from_utf8_lossy(&output.stderr).to_string(),
        exit_code: output.status.code(),
    })
}

async fn run_ssh_with_stdin(
    target: &PiNodeTarget,
    remote_command: &str,
    stdin_text: &str,
) -> Result<PiNodeCommandResult, String> {
    validate_target(target)?;
    let mut cmd = ssh_base(target);
    cmd.arg(remote_command);
    cmd.stdin(Stdio::piped());
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());

    let mut child = cmd
        .spawn()
        .map_err(|e| format!("Failed to run ssh. Is OpenSSH installed and available in PATH? {e}"))?;

    if let Some(mut stdin) = child.stdin.take() {
        stdin
            .write_all(stdin_text.as_bytes())
            .await
            .map_err(|e| format!("Failed to send config over ssh stdin: {e}"))?;
    }

    let output = child
        .wait_with_output()
        .await
        .map_err(|e| format!("Failed to collect ssh output: {e}"))?;

    Ok(PiNodeCommandResult {
        success: output.status.success(),
        command: remote_command.to_string(),
        stdout: String::from_utf8_lossy(&output.stdout).to_string(),
        stderr: String::from_utf8_lossy(&output.stderr).to_string(),
        exit_code: output.status.code(),
    })
}

fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\"'\"'"))
}

fn validate_path(value: &str, label: &str) -> Result<(), String> {
    if value.trim().is_empty() {
        return Err(format!("{label} is required"));
    }
    if value.contains(['\n', '\r']) {
        return Err(format!("{label} cannot contain newlines"));
    }
    Ok(())
}

fn env_line(key: &str, value: impl ToString) -> String {
    format!("{key}={}\n", shell_quote(&value.to_string()))
}

fn agent_env(config: &PiAgentConfig) -> String {
    let mut env = String::new();
    env.push_str(&env_line("RUVIEW_PI_AGENT_LISTEN", &config.listen));
    env.push_str(&env_line("RUVIEW_PI_AGENT_AGGREGATOR", &config.aggregator));
    env.push_str(&env_line("RUVIEW_PI_AGENT_NODE_BASE", config.node_base));
    env.push_str(&env_line("RUVIEW_PI_AGENT_TIER", config.tier));
    env.push_str(&env_line("RUVIEW_PI_AGENT_DEFAULT_RSSI", config.default_rssi));
    env.push_str(&env_line("RUVIEW_PI_AGENT_NOISE_FLOOR", config.noise_floor));
    env.push_str(&env_line("RUVIEW_PI_AGENT_MMWAVE_MOCK", config.mmwave_mock));
    env.push_str(&env_line("RUVIEW_PI_AGENT_ENABLE_WASM", config.enable_wasm));
    env.push_str(&env_line(
        "RUVIEW_PI_AGENT_WASM_PATH",
        config.wasm_path.as_deref().unwrap_or(""),
    ));
    env.push_str(&env_line("RUVIEW_PI_AGENT_WASM_MODULE_ID", config.wasm_module_id));
    env
}

fn service_unit(binary_path: &str, env_path: &str) -> String {
    format!(
        r#"[Unit]
Description=RuView Raspberry Pi Node Agent
After=network-online.target
Wants=network-online.target

[Service]
EnvironmentFile={}
ExecStart=/bin/sh -c 'args="--listen $RUVIEW_PI_AGENT_LISTEN --aggregator $RUVIEW_PI_AGENT_AGGREGATOR --node-base $RUVIEW_PI_AGENT_NODE_BASE --tier $RUVIEW_PI_AGENT_TIER --default-rssi $RUVIEW_PI_AGENT_DEFAULT_RSSI --noise-floor $RUVIEW_PI_AGENT_NOISE_FLOOR --wasm-module-id $RUVIEW_PI_AGENT_WASM_MODULE_ID"; if [ "$RUVIEW_PI_AGENT_MMWAVE_MOCK" = "true" ]; then args="$args --mmwave-mock"; fi; if [ "$RUVIEW_PI_AGENT_ENABLE_WASM" = "true" ]; then args="$args --enable-wasm"; fi; if [ -n "$RUVIEW_PI_AGENT_WASM_PATH" ]; then args="$args --wasm-path $RUVIEW_PI_AGENT_WASM_PATH"; fi; exec {} $args'
Restart=on-failure
RestartSec=2

[Install]
WantedBy=multi-user.target
"#,
        env_path, binary_path
    )
}

// ── Nexmon install: streaming, safety gate, orchestration ────────────────────

struct StreamOutcome {
    exit_code: Option<i32>,
    saw_marker: bool,
}

/// Run an SSH command and stream every stdout+stderr line to the frontend via
/// `app.emit(event, {line})`. Optionally watch for `marker`; `saw_marker` in the
/// outcome lets the caller tell an intentional reboot apart from a real failure.
async fn run_ssh_streaming(
    target: &PiNodeTarget,
    remote_command: &str,
    app: &AppHandle,
    event: &str,
    marker: Option<&str>,
) -> Result<StreamOutcome, String> {
    validate_target(target)?;
    let mut cmd = ssh_base(target);
    cmd.arg(remote_command);
    cmd.stdin(Stdio::null());
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());

    let mut child = cmd
        .spawn()
        .map_err(|e| format!("Failed to run ssh. Is OpenSSH installed and available in PATH? {e}"))?;
    let stdout = child.stdout.take().ok_or("Failed to capture ssh stdout")?;
    let stderr = child.stderr.take().ok_or("Failed to capture ssh stderr")?;

    let marker_owned = marker.map(|m| m.to_string());
    let saw = Arc::new(std::sync::atomic::AtomicBool::new(false));

    let spawn_reader = |reader: tokio::process::ChildStdout, is_out: bool| {
        let app = app.clone();
        let event = event.to_string();
        let saw = saw.clone();
        let marker = marker_owned.clone();
        let _ = is_out;
        tokio::spawn(async move {
            let mut lines = BufReader::new(reader).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                if let Some(ref m) = marker {
                    if line.contains(m.as_str()) {
                        saw.store(true, std::sync::atomic::Ordering::SeqCst);
                    }
                }
                let _ = app.emit(event.as_str(), serde_json::json!({ "line": line }));
            }
        })
    };
    let out_task = spawn_reader(stdout, true);

    // stderr uses the same body but a distinct type; inline it.
    let err_task = {
        let app = app.clone();
        let event = event.to_string();
        let saw = saw.clone();
        let marker = marker_owned.clone();
        tokio::spawn(async move {
            let mut lines = BufReader::new(stderr).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                if let Some(ref m) = marker {
                    if line.contains(m.as_str()) {
                        saw.store(true, std::sync::atomic::Ordering::SeqCst);
                    }
                }
                let _ = app.emit(event.as_str(), serde_json::json!({ "line": line }));
            }
        })
    };

    let status = child
        .wait()
        .await
        .map_err(|e| format!("Failed while awaiting ssh: {e}"))?;
    let _ = out_task.await;
    let _ = err_task.await;

    Ok(StreamOutcome {
        exit_code: status.code(),
        saw_marker: saw.load(std::sync::atomic::Ordering::SeqCst),
    })
}

/// Server-side IP of the SSH socket = field 3 of `$SSH_CONNECTION`
/// ("client_ip client_port server_ip server_port").
fn parse_ssh_server_ip(ssh_connection: &str) -> Option<String> {
    ssh_connection.split_whitespace().nth(2).map(str::to_string)
}

/// From `ip -o -4 addr show`, the interface that owns `server_ip`.
fn interface_owning_ip(ip_addr_output: &str, server_ip: &str) -> Option<String> {
    for line in ip_addr_output.lines() {
        let fields: Vec<&str> = line.split_whitespace().collect();
        if let Some(pos) = fields.iter().position(|&f| f == "inet") {
            if let Some(addr) = fields.get(pos + 1) {
                if addr.split('/').next() == Some(server_ip) {
                    return fields.get(1).map(|f| f.trim_end_matches(':').to_string());
                }
            }
        }
    }
    None
}

fn is_wifi_interface(name: &str) -> bool {
    let n = name.trim();
    n.starts_with("wlan") || n.starts_with("wlp") || n.starts_with("wlx")
}

/// Ethernet safety gate. Refuses if the SSH connection arrives on a Wi-Fi
/// interface (the install un-manages wlan0 and reboots — that would sever a
/// Wi-Fi SSH session and strand the Pi) or if the interface cannot be resolved
/// (fail-safe: we cannot confirm it is wired). Returns the wired interface name.
fn ethernet_gate(ssh_connection: &str, ip_addr_output: &str) -> Result<String, String> {
    let server_ip = parse_ssh_server_ip(ssh_connection)
        .filter(|s| !s.is_empty())
        .ok_or("Could not determine the SSH server IP ($SSH_CONNECTION was empty).")?;
    let iface = interface_owning_ip(ip_addr_output, &server_ip).ok_or_else(|| {
        format!(
            "Could not resolve which interface carries the SSH connection (server IP {server_ip}). \
             For safety, connect the Pi via Ethernet and retry."
        )
    })?;
    if is_wifi_interface(&iface) {
        return Err(format!(
            "Setup requires Ethernet: this Pi is reachable only over Wi-Fi ({iface}), which the Nexmon \
             install disables mid-process (it would sever this connection and strand the Pi). \
             Connect the Pi via Ethernet and retry."
        ));
    }
    Ok(iface)
}

fn normalize_lf(s: &str) -> String {
    s.replace("\r\n", "\n").replace('\r', "\n")
}

/// systemd oneshot that restores monitor mode on every boot. Runs as the SSH
/// user (not root) with an explicit HOME so `nexmon_startup.sh` finds the CSI
/// config saved under `$HOME/.config/nexmon`; `--no-tcpdump` avoids its blocking
/// demo.
fn nexmon_startup_unit(user: &str, home: &str) -> String {
    format!(
        r#"[Unit]
Description=RuView Nexmon monitor-mode restore
After=network.target

[Service]
Type=oneshot
RemainAfterExit=yes
User={user}
Environment=HOME={home}
ExecStart=/bin/bash {home}/.ruview/nexmon_startup.sh --no-tcpdump

[Install]
WantedBy=multi-user.target
"#
    )
}

/// Write `content` to `remote_path` on the Pi via `cat >` over SSH stdin.
async fn upload_text(target: &PiNodeTarget, content: &str, remote_path: &str) -> Result<(), String> {
    let cmd = format!("cat > {remote_path}");
    let res = run_ssh_with_stdin(target, &cmd, content).await?;
    if !res.success {
        return Err(format!("Failed to upload {remote_path}: {}", res.stderr.trim()));
    }
    Ok(())
}

#[derive(Debug, Clone, Deserialize)]
pub struct PiNexmonInstallRequest {
    pub target: PiNodeTarget,
}

/// Install Nexmon CSI on a Raspberry Pi end-to-end over SSH: safety-gate on
/// Ethernet, ship + run the setup script, wait through its reboot, verify, and
/// install the monitor-mode boot service. Streams progress on `pi-nexmon-progress`.
#[tauri::command]
pub async fn pi_node_install_nexmon(
    access_token: String,
    request: PiNexmonInstallRequest,
    app: AppHandle,
    auth: State<'_, Arc<AuthManager>>,
) -> Result<PiNodeCommandResult, String> {
    guard::require_auth(&access_token, &auth)?;
    let target = &request.target;
    validate_target(target)?;

    let emit = |line: &str| {
        let _ = app.emit("pi-nexmon-progress", serde_json::json!({ "line": line }));
    };

    // 1. Ethernet safety gate.
    emit("Checking that the Pi is reachable over Ethernet…");
    let sc = run_ssh(target, "echo \"$SSH_CONNECTION\"").await?;
    let addr = run_ssh(target, "ip -o -4 addr show").await?;
    let iface = match ethernet_gate(sc.stdout.trim(), &addr.stdout) {
        Ok(iface) => iface,
        Err(e) => {
            emit(&format!("BLOCKED: {e}"));
            return Err(e);
        }
    };
    emit(&format!("OK — SSH arrives on wired interface {iface}."));

    // 2. Passwordless sudo (the script issues many non-interactive sudo calls).
    emit("Checking passwordless sudo…");
    let sudo = run_ssh(target, "sudo -n true 2>/dev/null && echo RUVIEW_SUDO_OK || echo RUVIEW_SUDO_NO").await?;
    if !sudo.stdout.contains("RUVIEW_SUDO_OK") {
        let msg = "Passwordless sudo is required for the Nexmon install. On the Pi run: \
                   echo \"$USER ALL=(ALL) NOPASSWD:ALL\" | sudo tee /etc/sudoers.d/ruview && \
                   sudo chmod 440 /etc/sudoers.d/ruview"
            .to_string();
        emit(&format!("BLOCKED: {msg}"));
        return Err(msg);
    }

    let home_res = run_ssh(target, "echo $HOME").await?;
    let home = {
        let h = home_res.stdout.trim();
        if h.is_empty() { "/home/pi".to_string() } else { h.to_string() }
    };
    let user = target
        .user
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or("pi")
        .to_string();

    // 3. Upload the (LF-normalized) scripts.
    emit("Uploading Nexmon setup scripts…");
    run_ssh(target, "mkdir -p ~/.ruview").await?;
    upload_text(target, &normalize_lf(NEXMON_SETUP_SH), "~/.ruview/nexmon_setup_auto.sh").await?;
    upload_text(target, &normalize_lf(NEXMON_STARTUP_SH), "~/.ruview/nexmon_startup.sh").await?;
    run_ssh(target, "chmod +x ~/.ruview/nexmon_setup_auto.sh ~/.ruview/nexmon_startup.sh").await?;

    // 4. Stream the install (steps 1–14); it ends by rebooting.
    emit("Starting Nexmon install — this takes ~20–40 min and reboots the Pi partway.");
    let outcome = run_ssh_streaming(
        target,
        "bash ~/.ruview/nexmon_setup_auto.sh",
        &app,
        "pi-nexmon-progress",
        Some(NEXMON_REBOOT_MARKER),
    )
    .await?;
    if !outcome.saw_marker {
        let code = outcome
            .exit_code
            .map(|c| c.to_string())
            .unwrap_or_else(|| "unknown".into());
        let msg = format!(
            "The Nexmon install stopped before the reboot step (ssh exit {code}). Review the log above; \
             common causes are an unsupported kernel or a build failure."
        );
        emit(&format!("FAILED: {msg}"));
        return Err(msg);
    }

    // 5. Wait for the Pi to come back (up to 6 min).
    emit("Pi is rebooting — waiting for it to come back online…");
    let mut back = false;
    for _ in 0..36 {
        tokio::time::sleep(std::time::Duration::from_secs(10)).await;
        if let Ok(r) = run_ssh(target, "echo RUVIEW_UP").await {
            if r.success && r.stdout.contains("RUVIEW_UP") {
                back = true;
                break;
            }
        }
        emit("…still waiting for the Pi to reboot…");
    }
    if !back {
        let msg = "The Pi did not return within 6 minutes after the reboot.".to_string();
        emit(&format!("FAILED: {msg}"));
        return Err(msg);
    }
    emit("Pi is back online.");

    // 6. Post-reboot verification (step 15).
    emit("Running post-reboot verification…");
    let _ = run_ssh_streaming(
        target,
        "bash ~/.ruview/nexmon_setup_auto.sh --post-reboot",
        &app,
        "pi-nexmon-progress",
        None,
    )
    .await?;

    // 7. Install + enable the monitor-mode boot service, and run it once now.
    emit("Installing the monitor-mode boot service…");
    let unit = nexmon_startup_unit(&user, &home);
    let tmp = format!("/tmp/{NEXMON_STARTUP_SERVICE}.service");
    upload_text(target, &unit, &tmp).await?;
    let install_unit = format!(
        "sudo install -m 0644 {tmp} /etc/systemd/system/{svc}.service && rm -f {tmp} && \
         sudo systemctl daemon-reload && sudo systemctl enable {svc}.service",
        tmp = tmp,
        svc = NEXMON_STARTUP_SERVICE
    );
    let unit_res = run_ssh(target, &install_unit).await?;
    if !unit_res.success {
        let msg = format!("Failed to install the boot service: {}", unit_res.stderr.trim());
        emit(&format!("WARNING: {msg}"));
    }
    emit("Enabling monitor mode now…");
    let _ = run_ssh_streaming(
        target,
        "bash ~/.ruview/nexmon_startup.sh --no-tcpdump",
        &app,
        "pi-nexmon-progress",
        None,
    )
    .await?;

    emit("Nexmon is ready — CSI capture is live.");
    Ok(PiNodeCommandResult {
        success: true,
        command: "pi_node_install_nexmon".into(),
        stdout: "Nexmon installed; monitor mode enabled and set to restore on boot.".into(),
        stderr: String::new(),
        exit_code: Some(0),
    })
}

#[tauri::command]
pub async fn pi_node_probe(
    access_token: String,
    target: PiNodeTarget,
    auth: State<'_, Arc<AuthManager>>,
) -> Result<PiNodeCommandResult, String> {
    guard::require_auth(&access_token, &auth)?;
    let script = r#"set -u
echo "hostname=$(hostname 2>/dev/null || true)"
echo "user=$(id -un 2>/dev/null || true)"
echo "kernel=$(uname -a 2>/dev/null || true)"
echo "nexutil=$(command -v nexutil 2>/dev/null || echo missing)"
echo "agent_binary=$(command -v wifi-densepose-pi-node-agent 2>/dev/null || test -x /usr/local/bin/wifi-densepose-pi-node-agent && echo /usr/local/bin/wifi-densepose-pi-node-agent || echo missing)"
echo "service=$(systemctl is-active ruview-pi-node-agent 2>/dev/null || true)"
echo "wifi_ifaces=$(iw dev 2>/dev/null | awk '/Interface/ {print $2}' | tr '\n' ',' || true)"
echo "udp_5500=$(ss -lun 2>/dev/null | grep -c ':5500' || true)"
"#;
    run_ssh(&target, script).await
}

#[tauri::command]
pub async fn pi_node_build_agent(
    access_token: String,
    request: PiNodeBuildRequest,
    auth: State<'_, Arc<AuthManager>>,
) -> Result<PiNodeCommandResult, String> {
    guard::require_auth(&access_token, &auth)?;
    let release = request.release.unwrap_or(true);
    let mut cmd = Command::new("cargo");
    cmd.arg("build").arg("-p").arg("wifi-densepose-pi-node-agent");
    if let Some(workspace_path) = request.workspace_path.as_deref().filter(|value| !value.trim().is_empty()) {
        validate_path(workspace_path, "Workspace path")?;
        cmd.current_dir(workspace_path.trim());
    }
    if release {
        cmd.arg("--release");
    }
    if let Some(target) = request.target_triple.as_deref().filter(|value| !value.trim().is_empty()) {
        cmd.arg("--target").arg(target.trim());
    }

    let display = if let Some(target) = request.target_triple.as_deref().filter(|value| !value.trim().is_empty()) {
        format!("cargo build -p wifi-densepose-pi-node-agent{} --target {}", if release { " --release" } else { "" }, target)
    } else {
        format!("cargo build -p wifi-densepose-pi-node-agent{}", if release { " --release" } else { "" })
    };
    run_local_command(cmd, display).await
}

#[tauri::command]
pub async fn pi_node_deploy_binary(
    access_token: String,
    request: PiNodeDeployBinaryRequest,
    auth: State<'_, Arc<AuthManager>>,
) -> Result<PiNodeCommandResult, String> {
    guard::require_auth(&access_token, &auth)?;
    validate_target(&request.target)?;
    validate_path(&request.local_binary_path, "Local binary path")?;
    let remote_binary_path = request.remote_binary_path.as_deref().unwrap_or(DEFAULT_BINARY_PATH);
    validate_path(remote_binary_path, "Remote binary path")?;

    let local_path = std::path::Path::new(&request.local_binary_path);
    if !local_path.exists() {
        return Err(format!("Local binary does not exist: {}", request.local_binary_path));
    }

    let temp_path = "/tmp/wifi-densepose-pi-node-agent.upload";
    let remote_spec = format!("{}:{temp_path}", remote_addr(&request.target));
    let mut scp = scp_base(&request.target);
    scp.arg(&request.local_binary_path).arg(&remote_spec);
    let scp_result = run_local_command(
        scp,
        format!("scp {} {}", request.local_binary_path, remote_spec),
    )
    .await?;

    if !scp_result.success {
        return Ok(scp_result);
    }

    let install_cmd = format!(
        "sudo install -m 0755 {} {} && {} --version || true",
        shell_quote(temp_path),
        shell_quote(remote_binary_path),
        shell_quote(remote_binary_path)
    );
    let install_result = run_ssh(&request.target, &install_cmd).await?;
    Ok(PiNodeCommandResult {
        success: install_result.success,
        command: format!("{}\n{}", scp_result.command, install_result.command),
        stdout: format!("{}\n{}", scp_result.stdout, install_result.stdout),
        stderr: format!("{}\n{}", scp_result.stderr, install_result.stderr),
        exit_code: install_result.exit_code,
    })
}

#[tauri::command]
pub async fn pi_node_check_prereqs(
    access_token: String,
    request: PiNodePrereqRequest,
    auth: State<'_, Arc<AuthManager>>,
) -> Result<PiNodeCommandResult, String> {
    guard::require_auth(&access_token, &auth)?;
    let install_packages = request.install_packages.unwrap_or(false);
    let install_script = if install_packages {
        r#"
if command -v apt-get >/dev/null 2>&1; then
  sudo apt-get update
  sudo apt-get install -y iw tcpdump wireless-tools net-tools iproute2
else
  echo "apt_get=missing"
fi
"#
    } else {
        ""
    };
    let script = format!(
        r#"set -u
{install_script}
echo "os=$(grep PRETTY_NAME /etc/os-release 2>/dev/null | cut -d= -f2- | tr -d '"' || true)"
echo "model=$(tr -d '\0' </proc/device-tree/model 2>/dev/null || true)"
echo "arch=$(uname -m 2>/dev/null || true)"
echo "kernel=$(uname -r 2>/dev/null || true)"
echo "iw=$(command -v iw 2>/dev/null || echo missing)"
echo "tcpdump=$(command -v tcpdump 2>/dev/null || echo missing)"
echo "nexutil=$(command -v nexutil 2>/dev/null || echo missing)"
echo "brcmfmac=$(lsmod 2>/dev/null | awk '/brcmfmac/ {{print $1}}' | head -1 || true)"
echo "interfaces=$(iw dev 2>/dev/null | awk '/Interface/ {{print $2}}' | tr '\n' ',' || true)"
echo "monitor_ifaces=$(iw dev 2>/dev/null | awk 'prev == \"Interface\" {{iface=$1}} /type monitor/ {{print iface}} {{prev=$1}}' | tr '\n' ',' || true)"
if command -v nexutil >/dev/null 2>&1; then
  echo "nexutil_status=present"
else
  echo "nexutil_status=missing_install_nexmon_csi_first"
fi
"#
    );
    run_ssh(&request.target, &script).await
}

#[tauri::command]
pub async fn pi_node_csi_health(
    access_token: String,
    request: PiNodeHealthRequest,
    auth: State<'_, Arc<AuthManager>>,
) -> Result<PiNodeCommandResult, String> {
    guard::require_auth(&access_token, &auth)?;
    let port = request.nexmon_port.unwrap_or(5500);
    let seconds = request.capture_seconds.unwrap_or(6).clamp(1, 30);
    let service_name = request.service_name.as_deref().unwrap_or(DEFAULT_SERVICE_NAME);
    let script = format!(
        r#"set -u
echo "service=$(systemctl is-active {service}.service 2>/dev/null || true)"
echo "udp_listeners=$(ss -lun 2>/dev/null | grep -c ':{port}' || true)"
echo "journal_recent_begin"
journalctl -u {service}.service --no-pager -n 40 2>/dev/null || true
echo "journal_recent_end"
if command -v timeout >/dev/null 2>&1 && command -v tcpdump >/dev/null 2>&1; then
  echo "capture=starting_{seconds}s_udp_{port}"
  sudo timeout {seconds} tcpdump -i any -n udp port {port} -c 1 2>&1
  rc=$?
  echo "capture_exit=$rc"
  if [ "$rc" = "0" ]; then
    echo "csi_udp_seen=true"
  else
    echo "csi_udp_seen=false"
  fi
else
  echo "capture=unavailable_missing_timeout_or_tcpdump"
  echo "csi_udp_seen=unknown"
fi
"#,
        service = shell_quote(service_name),
        port = port,
        seconds = seconds
    );
    run_ssh(&request.target, &script).await
}

#[tauri::command]
pub async fn pi_node_push_config(
    access_token: String,
    request: PiNodeConfigRequest,
    auth: State<'_, Arc<AuthManager>>,
) -> Result<PiNodeCommandResult, String> {
    guard::require_auth(&access_token, &auth)?;
    let env_path = request.env_path.as_deref().unwrap_or(DEFAULT_ENV_PATH);
    let remote_command = format!(
        "sudo mkdir -p {} && sudo tee {} >/dev/null",
        shell_quote(
            std::path::Path::new(env_path)
                .parent()
                .and_then(|path| path.to_str())
                .unwrap_or("/etc/ruview")
        ),
        shell_quote(env_path)
    );
    run_ssh_with_stdin(&request.target, &remote_command, &agent_env(&request.config)).await
}

#[tauri::command]
pub async fn pi_node_install_service(
    access_token: String,
    request: PiNodeInstallRequest,
    auth: State<'_, Arc<AuthManager>>,
) -> Result<PiNodeCommandResult, String> {
    guard::require_auth(&access_token, &auth)?;
    let service_name = request.service_name.as_deref().unwrap_or(DEFAULT_SERVICE_NAME);
    let binary_path = request.binary_path.as_deref().unwrap_or(DEFAULT_BINARY_PATH);
    let env_path = request.env_path.as_deref().unwrap_or(DEFAULT_ENV_PATH);
    let unit_path = format!("/etc/systemd/system/{service_name}.service");
    let payload = format!("###ENV###\n{}###UNIT###\n{}", agent_env(&request.config), service_unit(binary_path, env_path));
    let script = format!(
        r#"set -eu
cat > /tmp/ruview-pi-node-agent.payload
sudo mkdir -p {env_dir}
awk 'BEGIN{{mode=0}} /^###ENV###$/{{mode=1; next}} /^###UNIT###$/{{mode=2; next}} mode==1{{print}}' /tmp/ruview-pi-node-agent.payload | sudo tee {env_path} >/dev/null
if ! test -x {binary_path}; then
  echo "missing_binary={binary_path}"
fi
awk 'BEGIN{{mode=0}} /^###UNIT###$/{{mode=1; next}} mode==1{{print}}' /tmp/ruview-pi-node-agent.payload > /tmp/ruview-pi-node-agent.service
sudo mv /tmp/ruview-pi-node-agent.service {unit_path}
sudo systemctl daemon-reload
sudo systemctl enable {service_name}.service
sudo systemctl status {service_name}.service --no-pager --lines=20 || true
"#,
        env_dir = shell_quote(
            std::path::Path::new(env_path)
                .parent()
                .and_then(|path| path.to_str())
                .unwrap_or("/etc/ruview")
        ),
        env_path = shell_quote(env_path),
        binary_path = shell_quote(binary_path),
        unit_path = shell_quote(&unit_path),
        service_name = shell_quote(service_name),
    );

    run_ssh_with_stdin(&request.target, &script, &payload).await
}

#[tauri::command]
pub async fn pi_node_service(
    access_token: String,
    request: PiNodeServiceRequest,
    auth: State<'_, Arc<AuthManager>>,
) -> Result<PiNodeCommandResult, String> {
    guard::require_auth(&access_token, &auth)?;
    let service_name = request.service_name.as_deref().unwrap_or(DEFAULT_SERVICE_NAME);
    let action = match request.action {
        PiServiceAction::Start => "start",
        PiServiceAction::Stop => "stop",
        PiServiceAction::Restart => "restart",
        PiServiceAction::Status => "status",
    };
    let script = if matches!(request.action, PiServiceAction::Status) {
        format!(
            "systemctl status {}.service --no-pager --lines=40 || true",
            shell_quote(service_name)
        )
    } else {
        format!(
            "sudo systemctl {} {}.service && systemctl status {}.service --no-pager --lines=20 || true",
            action,
            shell_quote(service_name),
            shell_quote(service_name)
        )
    };
    run_ssh(&request.target, &script).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn env_contains_agent_arguments() {
        let env = agent_env(&PiAgentConfig {
            listen: "0.0.0.0:5500".into(),
            aggregator: "192.168.1.5:5005".into(),
            node_base: 10,
            tier: 2,
            default_rssi: -55,
            noise_floor: -92,
            mmwave_mock: false,
            enable_wasm: true,
            wasm_path: Some("/opt/agent/filter.wasm".into()),
            wasm_module_id: 1,
        });

        assert!(env.contains("RUVIEW_PI_AGENT_LISTEN='0.0.0.0:5500'"));
        assert!(env.contains("RUVIEW_PI_AGENT_ENABLE_WASM='true'"));
    }

    // The `ip -o -4 addr show` sample lines below are representative of real
    // output (interface in field 2, "inet <addr>/<prefix>" mid-line).
    const ADDR_MIXED: &str = "1: lo    inet 127.0.0.1/8 scope host lo\n\
2: eth0    inet 192.168.1.60/24 brd 192.168.1.255 scope global eth0\n\
3: wlan0    inet 192.168.1.50/24 brd 192.168.1.255 scope global wlan0";

    #[test]
    fn ethernet_gate_refuses_when_ssh_is_on_wifi() {
        // Server IP .50 belongs to wlan0 → refuse.
        let sc = "192.168.1.10 54000 192.168.1.50 22";
        assert!(ethernet_gate(sc, ADDR_MIXED).is_err());
    }

    #[test]
    fn ethernet_gate_allows_wired() {
        // Server IP .60 belongs to eth0 → allow.
        let sc = "192.168.1.10 54000 192.168.1.60 22";
        assert_eq!(ethernet_gate(sc, ADDR_MIXED).unwrap(), "eth0");
    }

    #[test]
    fn ethernet_gate_same_subnet_uses_owning_iface_not_route() {
        // eth0 and wlan0 share the /24; the SSH socket bound to wlan0's IP must
        // still be recognized as Wi-Fi (the failure mode a route lookup misses).
        let addr = "2: eth0    inet 10.0.0.60/24 scope global eth0\n\
3: wlan0    inet 10.0.0.51/24 scope global wlan0";
        let sc = "10.0.0.9 5 10.0.0.51 22";
        assert!(ethernet_gate(sc, addr).is_err());
    }

    #[test]
    fn ethernet_gate_fails_safe_when_unresolvable() {
        let sc = "192.168.1.10 54000 10.9.9.9 22"; // .9.9 not in the addr table
        assert!(ethernet_gate(sc, ADDR_MIXED).is_err());
    }

    #[test]
    fn ethernet_gate_fails_safe_on_empty_ssh_connection() {
        assert!(ethernet_gate("", ADDR_MIXED).is_err());
    }

    #[test]
    fn wifi_interface_detection() {
        assert!(is_wifi_interface("wlan0"));
        assert!(is_wifi_interface("wlp2s0"));
        assert!(is_wifi_interface("wlx001122"));
        assert!(!is_wifi_interface("eth0"));
        assert!(!is_wifi_interface("enxdca6")); // USB Ethernet dongle
        assert!(!is_wifi_interface("eth1"));
    }

    #[test]
    fn embedded_scripts_present_and_guarded() {
        assert!(!NEXMON_SETUP_SH.is_empty());
        assert!(!NEXMON_STARTUP_SH.is_empty());
        // Reboot marker + non-interactive tcpdump guard must be in the script we ship.
        assert!(NEXMON_SETUP_SH.contains(NEXMON_REBOOT_MARKER));
        assert!(NEXMON_SETUP_SH.contains("if [ -t 0 ]"));
    }

    #[test]
    fn normalize_lf_strips_cr() {
        assert_eq!(normalize_lf("a\r\nb\rc\n"), "a\nb\nc\n");
        // A Windows checkout may carry CRLF; the uploaded script must be LF-only.
        assert!(!normalize_lf(NEXMON_SETUP_SH).contains('\r'));
        assert!(!normalize_lf(NEXMON_STARTUP_SH).contains('\r'));
    }

    #[test]
    fn startup_unit_runs_as_user_with_home_and_no_tcpdump() {
        let unit = nexmon_startup_unit("pi", "/home/pi");
        assert!(unit.contains("User=pi"));
        assert!(unit.contains("Environment=HOME=/home/pi"));
        assert!(unit.contains("/home/pi/.ruview/nexmon_startup.sh --no-tcpdump"));
    }
}
