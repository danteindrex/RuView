use std::net::{SocketAddr, UdpSocket};
use std::time::Duration;
use std::sync::Arc;

use mdns_sd::{ServiceDaemon, ServiceEvent};
use serde::Serialize;
use tauri::State;
use tokio::time::timeout;
use tokio_serial::available_ports;
use flume::RecvTimeoutError;

use crate::commands::auth::AuthManager;
use crate::commands::guard;
use crate::domain::node::{
    Chip, DiscoveredNode, DiscoveryMethod, HealthStatus, MacAddress, MeshRole,
    NodeCapabilities, NodeRegistry,
};
use crate::state::AppState;

/// Service type for Wave ESP32 nodes using mDNS.
const MDNS_SERVICE_TYPE: &str = "_wave._udp.local.";

/// UDP broadcast port for node discovery.
const UDP_DISCOVERY_PORT: u16 = 5006;

/// Discovery beacon magic bytes.
const BEACON_MAGIC: &[u8] = b"WAVE_BEACON";

/// Discover ESP32 CSI nodes on the local network via mDNS + UDP broadcast.
///
/// Discovery strategy:
/// 1. Start mDNS browser for `_wave._udp.local.`
/// 2. Send UDP broadcast on port 5006
/// 3. Collect responses for `timeout_ms` milliseconds
/// 4. Deduplicate by MAC address and return merged results
#[tauri::command]
pub async fn discover_nodes(
    access_token: String,
    timeout_ms: Option<u64>,
    state: State<'_, AppState>,
    auth: State<'_, Arc<AuthManager>>,
) -> Result<Vec<DiscoveredNode>, String> {
    guard::require_auth(&access_token, &auth)?;
    let timeout_duration = Duration::from_millis(timeout_ms.unwrap_or(3000));

    // Run mDNS and UDP discovery concurrently
    let (mdns_nodes, udp_nodes) = tokio::join!(
        discover_via_mdns(timeout_duration),
        discover_via_udp(timeout_duration),
    );

    // Merge results, deduplicating by MAC address
    let mut registry = NodeRegistry::new();

    for node in mdns_nodes.unwrap_or_default() {
        if let Some(ref mac) = node.mac {
            registry.upsert(MacAddress::new(mac), node);
        }
    }

    for node in udp_nodes.unwrap_or_default() {
        if let Some(ref mac) = node.mac {
            registry.upsert(MacAddress::new(mac), node);
        }
    }

    let nodes: Vec<DiscoveredNode> = registry.all().into_iter().cloned().collect();

    // Update global state
    {
        let mut discovery = state.discovery.lock().map_err(|e| e.to_string())?;
        discovery.nodes = nodes.clone();
    }

    Ok(nodes)
}

/// Discover nodes via mDNS (Bonjour/Avahi).
async fn discover_via_mdns(timeout_duration: Duration) -> Result<Vec<DiscoveredNode>, String> {
    let discovery_task = tokio::task::spawn_blocking(move || {
        let mdns = match ServiceDaemon::new() {
            Ok(daemon) => daemon,
            Err(e) => {
                tracing::warn!("Failed to create mDNS daemon: {}", e);
                return Vec::new();
            }
        };

        let receiver = match mdns.browse(MDNS_SERVICE_TYPE) {
            Ok(rx) => rx,
            Err(e) => {
                tracing::warn!("Failed to browse mDNS services: {}", e);
                return Vec::new();
            }
        };

        let mut discovered = Vec::new();
        let start = std::time::Instant::now();

        while start.elapsed() < timeout_duration {
            match receiver.recv_timeout(Duration::from_millis(100)) {
                Ok(ServiceEvent::ServiceResolved(info)) => {
                    let props = info.get_properties();
                    let chip_str = props.get("chip").map(|v| v.val_str());
                    let chip = match chip_str {
                        Some("esp32s2") => Chip::Esp32s2,
                        Some("esp32s3") => Chip::Esp32s3,
                        Some("esp32c3") => Chip::Esp32c3,
                        Some("esp32c6") => Chip::Esp32c6,
                        _ => Chip::Esp32,
                    };
                    let role_str = props.get("role").map(|v| v.val_str());
                    let mesh_role = match role_str {
                        Some("coordinator") => MeshRole::Coordinator,
                        Some("aggregator") => MeshRole::Aggregator,
                        _ => MeshRole::Node,
                    };
                    let node = DiscoveredNode {
                        ip: info.get_addresses()
                            .iter()
                            .next()
                            .map(|a| a.to_string())
                            .unwrap_or_default(),
                        mac: props.get("mac").map(|v| v.val_str().to_string()),
                        hostname: Some(info.get_hostname().to_string()),
                        node_id: props.get("node_id")
                            .and_then(|v| v.val_str().parse().ok())
                            .unwrap_or(0),
                        firmware_version: props.get("version").map(|v| v.val_str().to_string()),
                        health: HealthStatus::Online,
                        last_seen: chrono::Utc::now().to_rfc3339(),
                        chip,
                        mesh_role,
                        discovery_method: DiscoveryMethod::Mdns,
                        tdm_slot: props.get("tdm_slot").and_then(|v| v.val_str().parse().ok()),
                        tdm_total: props.get("tdm_total").and_then(|v| v.val_str().parse().ok()),
                        edge_tier: props.get("edge_tier").and_then(|v| v.val_str().parse().ok()),
                        uptime_secs: props.get("uptime").and_then(|v| v.val_str().parse().ok()),
                        capabilities: Some(NodeCapabilities {
                            wasm: props.get("wasm").map(|v| v.val_str() == "1").unwrap_or(false),
                            ota: props.get("ota").map(|v| v.val_str() == "1").unwrap_or(true),
                            csi: props.get("csi").map(|v| v.val_str() == "1").unwrap_or(true),
                        }),
                        friendly_name: props.get("name").map(|v| v.val_str().to_string()),
                        notes: None,
                    };
                    discovered.push(node);
                }
                Ok(ServiceEvent::SearchStarted(_)) => {}
                Ok(_) => {}
                Err(RecvTimeoutError::Timeout) => continue,
                Err(RecvTimeoutError::Disconnected) => break,
            }
        }

        // Stop browsing
        let _ = mdns.stop_browse(MDNS_SERVICE_TYPE);

        discovered
    });

    match timeout(timeout_duration + Duration::from_millis(500), discovery_task).await {
        Ok(Ok(nodes)) => Ok(nodes),
        Ok(Err(e)) => Err(format!("mDNS discovery task failed: {}", e)),
        Err(_) => Ok(Vec::new()), // Timeout, return empty
    }
}

/// Discover nodes via UDP broadcast beacon.
async fn discover_via_udp(timeout_duration: Duration) -> Result<Vec<DiscoveredNode>, String> {
    let discovery_task = tokio::task::spawn_blocking(move || -> Vec<DiscoveredNode> {
        let socket = match UdpSocket::bind("0.0.0.0:0") {
            Ok(s) => s,
            Err(e) => {
                tracing::warn!("Failed to bind UDP socket: {}", e);
                return Vec::new();
            }
        };

        if let Err(e) = socket.set_broadcast(true) {
            tracing::warn!("Failed to enable broadcast: {}", e);
            return Vec::new();
        }

        if let Err(e) = socket.set_read_timeout(Some(Duration::from_millis(100))) {
            tracing::warn!("Failed to set read timeout: {}", e);
            return Vec::new();
        }

        // Send discovery beacon
        let broadcast_addr = format!("255.255.255.255:{}", UDP_DISCOVERY_PORT);
        if let Err(e) = socket.send_to(b"WAVE_DISCOVER", &broadcast_addr) {
            tracing::warn!("Failed to send discovery beacon: {}", e);
        }

        let mut discovered = Vec::new();
        let mut buf = [0u8; 256];
        let start = std::time::Instant::now();

        while start.elapsed() < timeout_duration {
            match socket.recv_from(&mut buf) {
                Ok((len, addr)) => {
                    if len >= BEACON_MAGIC.len() && &buf[..BEACON_MAGIC.len()] == BEACON_MAGIC {
                        // Parse beacon response: WAVE_BEACON|mac|node_id|version
                        if let Some(node) = parse_beacon_response(&buf[..len], addr) {
                            discovered.push(node);
                        }
                    }
                }
                Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => continue,
                Err(ref e) if e.kind() == std::io::ErrorKind::TimedOut => continue,
                Err(_) => break,
            }
        }

        discovered
    });

    match timeout(timeout_duration + Duration::from_millis(500), discovery_task).await {
        Ok(Ok(nodes)) => Ok(nodes),
        Ok(Err(e)) => Err(format!("UDP discovery task failed: {}", e)),
        Err(_) => Ok(Vec::new()),
    }
}

/// Parse a UDP beacon response into a DiscoveredNode.
/// Format: WAVE_BEACON|<mac>|<node_id>|<version>|<chip>|<role>|<tdm_slot>|<tdm_total>
fn parse_beacon_response(data: &[u8], addr: SocketAddr) -> Option<DiscoveredNode> {
    let text = std::str::from_utf8(data).ok()?;
    let parts: Vec<&str> = text.split('|').collect();

    if parts.len() < 2 || parts[0] != "WAVE_BEACON" {
        return None;
    }

    let mac = parts.get(1).map(|s| s.to_string());
    let node_id = parts.get(2).and_then(|s| s.parse().ok()).unwrap_or(0);
    let version = parts.get(3).map(|s| s.to_string());
    let chip_str = parts.get(4).copied();
    let chip = match chip_str {
        Some("esp32s2") => Chip::Esp32s2,
        Some("esp32s3") => Chip::Esp32s3,
        Some("esp32c3") => Chip::Esp32c3,
        Some("esp32c6") => Chip::Esp32c6,
        _ => Chip::Esp32,
    };
    let role_str = parts.get(5).copied();
    let mesh_role = match role_str {
        Some("coordinator") => MeshRole::Coordinator,
        Some("aggregator") => MeshRole::Aggregator,
        _ => MeshRole::Node,
    };
    let tdm_slot = parts.get(6).and_then(|s| s.parse().ok());
    let tdm_total = parts.get(7).and_then(|s| s.parse().ok());

    Some(DiscoveredNode {
        ip: addr.ip().to_string(),
        mac,
        hostname: None,
        node_id,
        firmware_version: version,
        health: HealthStatus::Online,
        last_seen: chrono::Utc::now().to_rfc3339(),
        chip,
        mesh_role,
        discovery_method: DiscoveryMethod::UdpProbe,
        tdm_slot,
        tdm_total,
        edge_tier: None,
        uptime_secs: None,
        capabilities: Some(NodeCapabilities {
            wasm: false,
            ota: true,
            csi: true,
        }),
        friendly_name: None,
        notes: None,
    })
}

/// List available serial ports on this machine.
/// Filters for known ESP32 USB-to-serial chips (CP2102, CH340, FTDI).
#[tauri::command]
pub async fn list_serial_ports(
    access_token: String,
    auth: State<'_, Arc<AuthManager>>,
) -> Result<Vec<SerialPortInfo>, String> {
    guard::require_auth(&access_token, &auth)?;
    tracing::info!("list_serial_ports called");

    // On Windows the `serialport`/`tokio_serial` crate misses native-USB
    // composite devices (e.g. an ESP32-S3 on its built-in USB, VID 303A, which
    // enumerates as a `...&MI_00` composite interface) and can return nothing.
    // Enumerate via Win32_PnPEntity first — it reliably lists every COM port
    // with its USB VID/PID (the same source Device Manager and pyserial use).
    #[cfg(windows)]
    {
        match list_serial_ports_windows() {
            Ok(ports) if !ports.is_empty() => return Ok(ports),
            Ok(_) => tracing::warn!("Win32 enumeration found no COM ports; trying tokio_serial"),
            Err(e) => tracing::warn!("Win32 serial enumeration failed ({e}); trying tokio_serial"),
        }
    }

    let ports = match available_ports() {
        Ok(p) => {
            tracing::info!("Found {} ports from tokio_serial", p.len());
            p
        }
        Err(e) => {
            tracing::error!("Failed to enumerate ports: {}", e);
            // Fallback: try to list /dev/cu.usb* manually on macOS
            return list_serial_ports_fallback();
        }
    };

    let mut result = Vec::new();

    for port in ports {
        tracing::debug!("Processing port: {}", port.port_name);
        let info = match port.port_type {
            tokio_serial::SerialPortType::UsbPort(usb_info) => {
                SerialPortInfo {
                    name: port.port_name,
                    vid: Some(usb_info.vid),
                    pid: Some(usb_info.pid),
                    manufacturer: usb_info.manufacturer,
                    serial_number: usb_info.serial_number,
                    is_esp32_compatible: is_esp32_compatible(usb_info.vid, usb_info.pid),
                }
            }
            _ => {
                SerialPortInfo {
                    name: port.port_name.clone(),
                    vid: None,
                    pid: None,
                    manufacturer: None,
                    serial_number: None,
                    // Mark /dev/cu.usb* ports as potentially compatible
                    is_esp32_compatible: port.port_name.contains("usb"),
                }
            }
        };

        result.push(info);
    }

    // If no ports found via tokio_serial, try fallback
    if result.is_empty() {
        tracing::warn!("No ports from tokio_serial, trying fallback");
        return list_serial_ports_fallback();
    }

    // Sort ESP32-compatible ports first
    result.sort_by(|a, b| b.is_esp32_compatible.cmp(&a.is_esp32_compatible));

    tracing::info!("Returning {} serial ports", result.len());
    Ok(result)
}

/// Fallback serial port listing for macOS when tokio_serial fails
fn list_serial_ports_fallback() -> Result<Vec<SerialPortInfo>, String> {
    tracing::info!("Using fallback serial port listing");

    let result = Vec::new();

    // List /dev/cu.usb* devices on macOS
    #[cfg(target_os = "macos")]
    {
        use std::fs;
        if let Ok(entries) = fs::read_dir("/dev") {
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().to_string();
                if name.starts_with("cu.usb") {
                    let path = format!("/dev/{}", name);
                    tracing::info!("Fallback found port: {}", path);
                    result.push(SerialPortInfo {
                        name: path,
                        vid: None,
                        pid: None,
                        manufacturer: Some("USB Serial".to_string()),
                        serial_number: None,
                        is_esp32_compatible: true, // Assume USB serial is ESP32
                    });
                }
            }
        }
    }

    // Linux fallback
    #[cfg(target_os = "linux")]
    {
        use std::fs;
        if let Ok(entries) = fs::read_dir("/dev") {
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().to_string();
                if name.starts_with("ttyUSB") || name.starts_with("ttyACM") {
                    let path = format!("/dev/{}", name);
                    tracing::info!("Fallback found port: {}", path);
                    result.push(SerialPortInfo {
                        name: path,
                        vid: None,
                        pid: None,
                        manufacturer: Some("USB Serial".to_string()),
                        serial_number: None,
                        is_esp32_compatible: true,
                    });
                }
            }
        }
    }

    tracing::info!("Fallback found {} ports", result.len());
    Ok(result)
}

/// Check if a USB VID/PID is from a known ESP32 USB-to-serial chip.
fn is_esp32_compatible(vid: u16, pid: u16) -> bool {
    // CP210x (Silicon Labs)
    if vid == 0x10C4 && (pid == 0xEA60 || pid == 0xEA70) {
        return true;
    }
    // CH340/CH341 (QinHeng)
    if vid == 0x1A86 && (pid == 0x7523 || pid == 0x5523) {
        return true;
    }
    // FTDI
    if vid == 0x0403 && (pid == 0x6001 || pid == 0x6010 || pid == 0x6011 || pid == 0x6014 || pid == 0x6015) {
        return true;
    }
    // ESP32-S2/S3 native USB
    if vid == 0x303A {
        return true;
    }
    false
}

/// Extract a `COM<n>` port name from a Win32_PnPEntity friendly name such as
/// "USB Serial Device (COM7)" or "Silicon Labs CP210x ... (COM3)".
fn extract_com_name(name: &str) -> Option<String> {
    let idx = name.find("COM")?;
    let digits: String = name[idx + 3..].chars().take_while(|c| c.is_ascii_digit()).collect();
    if digits.is_empty() {
        None
    } else {
        Some(format!("COM{digits}"))
    }
}

/// Extract the 4 hex digits after `marker` (e.g. "VID_" / "PID_") from a device
/// instance id like `USB\VID_303A&PID_1001&MI_00\...`.
fn extract_hex_after(s: &str, marker: &str) -> Option<u16> {
    let start = s.find(marker)? + marker.len();
    let hex: String = s[start..].chars().take(4).collect();
    u16::from_str_radix(&hex, 16).ok()
}

/// Windows serial-port enumeration via Win32_PnPEntity (PowerShell). Reliable
/// for native-USB composite devices the `serialport` crate misses.
#[cfg(windows)]
fn list_serial_ports_windows() -> Result<Vec<SerialPortInfo>, String> {
    use std::process::Command as StdCommand;
    let output = StdCommand::new("powershell")
        .args([
            "-NoProfile",
            "-Command",
            "Get-CimInstance Win32_PnPEntity | Where-Object { $_.Name -match 'COM\\d+' } | \
             ForEach-Object { [pscustomobject]@{ Name = $_.Name; DeviceID = $_.DeviceID } } | \
             ConvertTo-Json -Compress",
        ])
        .output()
        .map_err(|e| format!("Failed to run powershell for serial enumeration: {e}"))?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let trimmed = stdout.trim();
    if trimmed.is_empty() {
        return Ok(Vec::new());
    }
    let value: serde_json::Value =
        serde_json::from_str(trimmed).map_err(|e| format!("Failed to parse serial JSON: {e}"))?;
    // ConvertTo-Json emits a bare object for a single match, an array otherwise.
    let items = match value {
        serde_json::Value::Array(a) => a,
        other => vec![other],
    };

    let mut result: Vec<SerialPortInfo> = items
        .into_iter()
        .filter_map(|item| {
            let name_full = item.get("Name").and_then(|v| v.as_str()).unwrap_or("");
            let device_id = item.get("DeviceID").and_then(|v| v.as_str()).unwrap_or("");
            let com = extract_com_name(name_full)?;
            let vid = extract_hex_after(device_id, "VID_");
            let pid = extract_hex_after(device_id, "PID_");
            let is_esp32 = matches!((vid, pid), (Some(v), Some(p)) if is_esp32_compatible(v, p));
            Some(SerialPortInfo {
                name: com,
                vid,
                pid,
                manufacturer: None,
                serial_number: None,
                is_esp32_compatible: is_esp32,
            })
        })
        .collect();

    result.sort_by(|a, b| b.is_esp32_compatible.cmp(&a.is_esp32_compatible));
    Ok(result)
}

/// The hub PC's current WiFi network and LAN IP, used to prefill the ESP32
/// provisioning form (the node joins the same WiFi and streams to this PC).
#[derive(Debug, Clone, Serialize)]
pub struct HostNetworkInfo {
    pub ssid: Option<String>,
    pub lan_ip: Option<String>,
}

#[tauri::command]
pub async fn host_network_info(
    access_token: String,
    auth: State<'_, Arc<AuthManager>>,
) -> Result<HostNetworkInfo, String> {
    guard::require_auth(&access_token, &auth)?;
    Ok(host_network_info_impl())
}

#[cfg(windows)]
fn host_network_info_impl() -> HostNetworkInfo {
    use std::process::Command as StdCommand;
    // One PowerShell call: current WiFi SSID (netsh) + the Wi-Fi adapter's
    // non-APIPA IPv4, emitted as JSON.
    let script = "$ssid = (netsh wlan show interfaces | Select-String '^\\s*SSID\\s*:\\s*(.+)$' | \
        Select-Object -First 1).Matches.Groups[1].Value; \
        $ip = (Get-NetIPAddress -AddressFamily IPv4 | Where-Object { $_.InterfaceAlias -eq 'Wi-Fi' -and \
        $_.IPAddress -notmatch '^169\\.254' } | Select-Object -First 1).IPAddress; \
        [pscustomobject]@{ ssid = $ssid; lan_ip = $ip } | ConvertTo-Json -Compress";
    let output = StdCommand::new("powershell")
        .args(["-NoProfile", "-Command", script])
        .output();
    if let Ok(o) = output {
        let stdout = String::from_utf8_lossy(&o.stdout);
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(stdout.trim()) {
            let clean = |key: &str| {
                v.get(key)
                    .and_then(|x| x.as_str())
                    .map(|x| x.trim().to_string())
                    .filter(|x| !x.is_empty())
            };
            return HostNetworkInfo {
                ssid: clean("ssid"),
                lan_ip: clean("lan_ip"),
            };
        }
    }
    HostNetworkInfo { ssid: None, lan_ip: None }
}

#[cfg(not(windows))]
fn host_network_info_impl() -> HostNetworkInfo {
    let ssid = std::process::Command::new("iwgetid")
        .arg("-r")
        .output()
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .filter(|s| !s.is_empty());
    let lan_ip = std::process::Command::new("hostname")
        .arg("-I")
        .output()
        .ok()
        .and_then(|o| {
            let s = String::from_utf8_lossy(&o.stdout);
            s.split_whitespace().next().map(|x| x.to_string())
        });
    HostNetworkInfo { ssid, lan_ip }
}

/// A WiFi network visible to the hub PC, for scan-and-pick provisioning.
#[derive(Debug, Clone, Serialize)]
pub struct WifiNetwork {
    pub ssid: String,
    /// Has at least one BSSID on a 2.4 GHz channel (ESP32/Pi require 2.4 GHz).
    pub band_24ghz: bool,
    pub band_5ghz: bool,
    /// Open network (no password needed).
    pub open: bool,
    /// A Windows profile is stored → the password can be auto-filled.
    pub saved: bool,
    pub connected: bool,
}

#[tauri::command]
pub async fn scan_wifi_networks(
    access_token: String,
    auth: State<'_, Arc<AuthManager>>,
) -> Result<Vec<WifiNetwork>, String> {
    guard::require_auth(&access_token, &auth)?;
    Ok(scan_wifi_impl())
}

/// The saved WiFi password for `ssid`, if the PC has a stored profile for it.
#[tauri::command]
pub async fn wifi_saved_password(
    access_token: String,
    ssid: String,
    auth: State<'_, Arc<AuthManager>>,
) -> Result<Option<String>, String> {
    guard::require_auth(&access_token, &auth)?;
    if ssid.contains(['\n', '\r', '"']) {
        return Err("Invalid SSID".into());
    }
    Ok(wifi_saved_password_impl(&ssid))
}

#[cfg(windows)]
fn scan_wifi_impl() -> Vec<WifiNetwork> {
    use std::collections::{BTreeMap, HashSet};
    use std::process::Command as StdCommand;
    let run = |args: &[&str]| {
        StdCommand::new("netsh")
            .args(args)
            .output()
            .ok()
            .map(|o| String::from_utf8_lossy(&o.stdout).into_owned())
            .unwrap_or_default()
    };

    let networks_out = run(&["wlan", "show", "networks", "mode=bssid"]);
    let profiles_out = run(&["wlan", "show", "profiles"]);
    let iface_out = run(&["wlan", "show", "interfaces"]);

    let saved: HashSet<String> = profiles_out
        .lines()
        .filter_map(|l| l.split_once(':'))
        .filter(|(k, _)| k.contains("Profile"))
        .map(|(_, v)| v.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();

    let connected_ssid = iface_out.lines().find_map(|l| {
        let t = l.trim();
        // "SSID : NAME" but not "BSSID : ...".
        if t.starts_with("SSID") && !t.starts_with("BSSID") {
            t.split_once(':').map(|(_, v)| v.trim().to_string())
        } else {
            None
        }
    });

    let mut map: BTreeMap<String, WifiNetwork> = BTreeMap::new();
    let mut cur: Option<String> = None;
    for line in networks_out.lines() {
        let t = line.trim();
        // "SSID 1 : NAME"
        if let Some(rest) = t.strip_prefix("SSID ") {
            if let Some((_, name)) = rest.split_once(':') {
                let name = name.trim().to_string();
                if name.is_empty() {
                    cur = None;
                    continue;
                }
                let connected = connected_ssid.as_deref() == Some(name.as_str());
                let saved_flag = saved.contains(&name);
                cur = Some(name.clone());
                map.entry(name.clone()).or_insert(WifiNetwork {
                    ssid: name,
                    band_24ghz: false,
                    band_5ghz: false,
                    open: false,
                    saved: saved_flag,
                    connected,
                });
            }
        } else if let Some(name) = cur.as_ref() {
            if let Some((_, v)) = t.split_once(':') {
                let v = v.trim();
                if t.starts_with("Authentication") {
                    if v.eq_ignore_ascii_case("Open") {
                        if let Some(n) = map.get_mut(name) {
                            n.open = true;
                        }
                    }
                } else if t.starts_with("Channel") {
                    if let Ok(ch) = v.parse::<u32>() {
                        if let Some(n) = map.get_mut(name) {
                            if ch <= 14 {
                                n.band_24ghz = true;
                            } else {
                                n.band_5ghz = true;
                            }
                        }
                    }
                }
            }
        }
    }

    let mut list: Vec<WifiNetwork> = map.into_values().collect();
    // Connected first, then 2.4GHz-capable, then saved, then alphabetical.
    list.sort_by(|a, b| {
        b.connected
            .cmp(&a.connected)
            .then(b.band_24ghz.cmp(&a.band_24ghz))
            .then(b.saved.cmp(&a.saved))
            .then(a.ssid.cmp(&b.ssid))
    });
    list
}

#[cfg(not(windows))]
fn scan_wifi_impl() -> Vec<WifiNetwork> {
    Vec::new()
}

#[cfg(windows)]
fn wifi_saved_password_impl(ssid: &str) -> Option<String> {
    let out = std::process::Command::new("netsh")
        .args(["wlan", "show", "profile", &format!("name={ssid}"), "key=clear"])
        .output()
        .ok()?;
    let text = String::from_utf8_lossy(&out.stdout);
    for line in text.lines() {
        let t = line.trim();
        if t.starts_with("Key Content") {
            if let Some((_, v)) = t.split_once(':') {
                let v = v.trim();
                if !v.is_empty() {
                    return Some(v.to_string());
                }
            }
        }
    }
    None
}

#[cfg(not(windows))]
fn wifi_saved_password_impl(_ssid: &str) -> Option<String> {
    None
}

/// Configure WiFi credentials on an ESP32 via serial port.
///
/// Sends WiFi credentials to the ESP32 using a simple serial protocol.
/// The ESP32 firmware should accept: `wifi_config <ssid> <password>\n`
#[tauri::command]
pub async fn configure_esp32_wifi(
    access_token: String,
    port: String,
    ssid: String,
    password: String,
    auth: State<'_, Arc<AuthManager>>,
) -> Result<String, String> {
    guard::require_auth(&access_token, &auth)?;
    use std::io::{Read, Write};
    use std::time::Duration;

    tracing::info!("Configuring WiFi on port: {}", port);

    // Open serial port
    let mut serial = serialport::new(&port, 115200)
        .timeout(Duration::from_secs(3))
        .open()
        .map_err(|e| format!("Failed to open port {}: {}", port, e))?;

    // Wait for ESP32 to be ready
    std::thread::sleep(Duration::from_millis(500));

    // Try multiple command formats that different firmware versions might accept
    let commands = [
        format!("wifi_config {} {}\r\n", ssid, password),
        format!("wifi {} {}\r\n", ssid, password),
        format!("set ssid {}\r\n", ssid),
    ];

    let mut response = String::new();
    let mut buf = [0u8; 512];

    for cmd in &commands {
        // Clear any pending data
        let _ = serial.read(&mut buf);

        // Send command
        serial.write_all(cmd.as_bytes())
            .map_err(|e| format!("Failed to write: {}", e))?;
        serial.flush().map_err(|e| format!("Failed to flush: {}", e))?;

        // Wait and read response
        std::thread::sleep(Duration::from_millis(500));

        match serial.read(&mut buf) {
            Ok(n) if n > 0 => {
                let text = String::from_utf8_lossy(&buf[..n]).to_string();
                response.push_str(&text);

                // Check for success indicators
                if text.to_lowercase().contains("ok")
                    || text.to_lowercase().contains("saved")
                    || text.to_lowercase().contains("configured") {
                    tracing::info!("WiFi config successful: {}", text.trim());
                    return Ok(format!("WiFi configured! Response: {}", text.trim()));
                }
            }
            _ => {}
        }
    }

    // Also try to send password separately if ssid command was sent
    let pwd_cmd = format!("set password {}\r\n", password);
    let _ = serial.write_all(pwd_cmd.as_bytes());
    let _ = serial.flush();
    std::thread::sleep(Duration::from_millis(300));
    if let Ok(n) = serial.read(&mut buf) {
        if n > 0 {
            response.push_str(&String::from_utf8_lossy(&buf[..n]));
        }
    }

    // Send reboot command
    let _ = serial.write_all(b"reboot\r\n");
    let _ = serial.flush();

    if response.is_empty() {
        Ok("Commands sent. ESP32 may need manual reboot to apply WiFi settings.".to_string())
    } else {
        Ok(format!("Commands sent. Response: {}", response.trim()))
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct SerialPortInfo {
    pub name: String,
    pub vid: Option<u16>,
    pub pid: Option<u16>,
    pub manufacturer: Option<String>,
    pub serial_number: Option<String>,
    pub is_esp32_compatible: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_beacon_response() {
        let data = b"WAVE_BEACON|AA:BB:CC:DD:EE:FF|1|0.3.0|esp32s3|coordinator|0|4";
        let addr: SocketAddr = "192.168.1.100:5006".parse().unwrap();

        let node = parse_beacon_response(data, addr).unwrap();
        assert_eq!(node.ip, "192.168.1.100");
        assert_eq!(node.mac, Some("AA:BB:CC:DD:EE:FF".to_string()));
        assert_eq!(node.node_id, 1);
        assert_eq!(node.firmware_version, Some("0.3.0".to_string()));
        assert_eq!(node.chip, Chip::Esp32s3);
        assert_eq!(node.mesh_role, MeshRole::Coordinator);
        assert_eq!(node.tdm_slot, Some(0));
        assert_eq!(node.tdm_total, Some(4));
    }

    #[test]
    fn test_is_esp32_compatible() {
        // CP2102
        assert!(is_esp32_compatible(0x10C4, 0xEA60));
        // CH340
        assert!(is_esp32_compatible(0x1A86, 0x7523));
        // ESP32-S3 native
        assert!(is_esp32_compatible(0x303A, 0x1001));
        // Unknown
        assert!(!is_esp32_compatible(0x0000, 0x0000));
    }

    #[test]
    fn test_extract_com_name() {
        assert_eq!(extract_com_name("USB Serial Device (COM7)").as_deref(), Some("COM7"));
        assert_eq!(extract_com_name("Silicon Labs CP210x USB to UART Bridge (COM3)").as_deref(), Some("COM3"));
        assert_eq!(extract_com_name("USB-Enhanced-SERIAL CH343 (COM15)").as_deref(), Some("COM15"));
        assert_eq!(extract_com_name("Some device with no port"), None);
    }

    #[test]
    fn test_extract_hex_after() {
        let dev = "USB\\VID_303A&PID_1001&MI_00\\6&37C81CD2&0&0000";
        assert_eq!(extract_hex_after(dev, "VID_"), Some(0x303A));
        assert_eq!(extract_hex_after(dev, "PID_"), Some(0x1001));
        // A CH343 bridge device id.
        let ch = "USB\\VID_1A86&PID_55D3\\5735002173";
        assert_eq!(extract_hex_after(ch, "VID_"), Some(0x1A86));
        assert_eq!(extract_hex_after(ch, "PID_"), Some(0x55D3));
        assert_eq!(extract_hex_after("no vid here", "VID_"), None);
    }
}
