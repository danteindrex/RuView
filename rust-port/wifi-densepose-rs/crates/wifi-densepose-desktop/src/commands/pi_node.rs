use serde::{Deserialize, Serialize};
use std::process::Stdio;
use tokio::io::AsyncWriteExt;
use tokio::process::Command;

const DEFAULT_SERVICE_NAME: &str = "ruview-pi-node-agent";
const DEFAULT_BINARY_PATH: &str = "/usr/local/bin/wifi-densepose-pi-node-agent";
const DEFAULT_ENV_PATH: &str = "/etc/ruview/pi-node-agent.env";

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

#[tauri::command]
pub async fn pi_node_probe(target: PiNodeTarget) -> Result<PiNodeCommandResult, String> {
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
pub async fn pi_node_build_agent(request: PiNodeBuildRequest) -> Result<PiNodeCommandResult, String> {
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
pub async fn pi_node_deploy_binary(request: PiNodeDeployBinaryRequest) -> Result<PiNodeCommandResult, String> {
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
pub async fn pi_node_check_prereqs(request: PiNodePrereqRequest) -> Result<PiNodeCommandResult, String> {
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
pub async fn pi_node_csi_health(request: PiNodeHealthRequest) -> Result<PiNodeCommandResult, String> {
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
pub async fn pi_node_push_config(request: PiNodeConfigRequest) -> Result<PiNodeCommandResult, String> {
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
pub async fn pi_node_install_service(request: PiNodeInstallRequest) -> Result<PiNodeCommandResult, String> {
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
pub async fn pi_node_service(request: PiNodeServiceRequest) -> Result<PiNodeCommandResult, String> {
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
}
