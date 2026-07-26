//! Hardware onboarding commands (backend "C") — done directly by the CLI so it
//! never depends on the desktop app. `serial list` and `wifi scan` here; ESP32
//! `flash`/`provision`/`serial-check` build on the same subprocess approach.
//!
//! Windows-first (the primary provisioning platform): serial enumeration uses
//! `Get-CimInstance` (catches native-USB VID 303A composite boards that generic
//! serial libs miss), WiFi scan uses `netsh`. Non-Windows gets a best-effort
//! fallback.

use anyhow::Result;
use serde::Serialize;

#[derive(Serialize)]
pub struct SerialPort {
    pub name: String,
    pub description: String,
    pub vid: Option<String>,
    pub pid: Option<String>,
    pub esp32_compatible: bool,
}

#[derive(Serialize)]
pub struct WifiNet {
    pub ssid: String,
    pub band: String,       // "2.4GHz" | "5GHz" | "?"
    pub channel: Option<u32>,
}

// ── serial list ──────────────────────────────────────────────────────────────

#[cfg(windows)]
pub fn serial_list() -> Result<Vec<SerialPort>> {
    // Enumerate COM devices via CIM (like the desktop's discovery path).
    let ps = "Get-CimInstance Win32_PnPEntity | Where-Object { $_.Name -match 'COM\\d+' } | \
              Select-Object Name, DeviceID | ConvertTo-Json -Compress";
    let out = std::process::Command::new("powershell")
        .args(["-NoProfile", "-Command", ps])
        .output()?;
    let text = String::from_utf8_lossy(&out.stdout);
    let json: serde_json::Value = serde_json::from_str(text.trim()).unwrap_or(serde_json::Value::Null);
    // CIM returns a single object (not an array) when there is exactly one match.
    let items = match json {
        serde_json::Value::Array(a) => a,
        serde_json::Value::Object(_) => vec![json],
        _ => vec![],
    };
    let mut ports = Vec::new();
    for it in items {
        let name = it.get("Name").and_then(|v| v.as_str()).unwrap_or("").to_string();
        let device_id = it.get("DeviceID").and_then(|v| v.as_str()).unwrap_or("");
        let com = extract_com(&name);
        if com.is_empty() {
            continue;
        }
        let vid = extract_hex(device_id, "VID_");
        let pid = extract_hex(device_id, "PID_");
        let esp32_compatible = vid.as_deref() == Some("303A")
            || name.contains("CH34")
            || name.contains("CP210")
            || name.contains("Silicon Labs")
            || name.contains("USB Serial")
            || name.contains("USB-Enhanced-SERIAL");
        ports.push(SerialPort {
            name: com,
            description: name,
            vid,
            pid,
            esp32_compatible,
        });
    }
    ports.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(ports)
}

#[cfg(not(windows))]
pub fn serial_list() -> Result<Vec<SerialPort>> {
    // Best-effort: list common USB-serial tty device nodes.
    let mut ports = Vec::new();
    for pat in ["/dev/ttyUSB", "/dev/ttyACM", "/dev/tty.usbserial", "/dev/tty.usbmodem"] {
        if let Some(dir) = std::path::Path::new(pat).parent() {
            if let Ok(rd) = std::fs::read_dir(dir) {
                for e in rd.flatten() {
                    let p = e.path();
                    let s = p.to_string_lossy().to_string();
                    if s.starts_with(pat) {
                        ports.push(SerialPort {
                            name: s,
                            description: "usb-serial".into(),
                            vid: None,
                            pid: None,
                            esp32_compatible: true,
                        });
                    }
                }
            }
        }
    }
    ports.sort_by(|a, b| a.name.cmp(&b.name));
    ports.dedup_by(|a, b| a.name == b.name);
    Ok(ports)
}

#[cfg(windows)]
fn extract_com(name: &str) -> String {
    // Pull "COM7" out of "USB-Enhanced-SERIAL CH343 (COM7)".
    if let Some(start) = name.find("COM") {
        let tail: String = name[start..]
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric())
            .collect();
        if tail.len() > 3 && tail[3..].chars().all(|c| c.is_ascii_digit()) {
            return tail;
        }
    }
    String::new()
}

#[cfg(windows)]
fn extract_hex(s: &str, prefix: &str) -> Option<String> {
    let i = s.find(prefix)? + prefix.len();
    let hex: String = s[i..].chars().take(4).collect();
    if hex.len() == 4 && hex.chars().all(|c| c.is_ascii_hexdigit()) {
        Some(hex.to_uppercase())
    } else {
        None
    }
}

// ── wifi scan ────────────────────────────────────────────────────────────────

#[cfg(windows)]
pub fn wifi_scan() -> Result<Vec<WifiNet>> {
    let out = std::process::Command::new("netsh")
        .args(["wlan", "show", "networks", "mode=bssid"])
        .output()?;
    let text = String::from_utf8_lossy(&out.stdout);
    let mut nets = Vec::new();
    let mut cur_ssid: Option<String> = None;
    for line in text.lines() {
        let l = line.trim();
        if let Some(rest) = l.strip_prefix("SSID ") {
            // "SSID 1 : MyNet"
            if let Some(idx) = rest.find(':') {
                let ssid = rest[idx + 1..].trim().to_string();
                cur_ssid = if ssid.is_empty() { None } else { Some(ssid) };
            }
        } else if l.starts_with("Channel") {
            if let (Some(ssid), Some(idx)) = (cur_ssid.clone(), l.find(':')) {
                let ch: u32 = l[idx + 1..].trim().parse().unwrap_or(0);
                let band = if (1..=14).contains(&ch) { "2.4GHz" } else if ch > 14 { "5GHz" } else { "?" };
                nets.push(WifiNet { ssid, band: band.into(), channel: if ch > 0 { Some(ch) } else { None } });
                cur_ssid = None; // one channel row per SSID block is enough
            }
        }
    }
    // Dedup by SSID (multiple BSSIDs per SSID).
    nets.sort_by(|a, b| a.ssid.cmp(&b.ssid));
    nets.dedup_by(|a, b| a.ssid == b.ssid);
    Ok(nets)
}

#[cfg(not(windows))]
pub fn wifi_scan() -> Result<Vec<WifiNet>> {
    anyhow::bail!("wifi scan is currently Windows-only (uses netsh)");
}

// ── helpers for provisioning ────────────────────────────────────────────────

/// Read the saved WiFi key for an SSID from the Windows profile (for auto-fill).
#[cfg(windows)]
pub fn wifi_saved_password(ssid: &str) -> Option<String> {
    if ssid.contains('"') || ssid.contains('\n') || ssid.contains('\r') {
        return None;
    }
    let out = std::process::Command::new("netsh")
        .args(["wlan", "show", "profile", &format!("name={ssid}"), "key=clear"])
        .output()
        .ok()?;
    let text = String::from_utf8_lossy(&out.stdout);
    for line in text.lines() {
        if let Some(idx) = line.find("Key Content") {
            if let Some(c) = line[idx..].find(':') {
                let pw = line[idx + c + 1..].trim().to_string();
                if !pw.is_empty() {
                    return Some(pw);
                }
            }
        }
    }
    None
}

#[cfg(not(windows))]
pub fn wifi_saved_password(_ssid: &str) -> Option<String> {
    None
}

/// This PC's Wi-Fi LAN IPv4 (the address a node should stream to).
#[cfg(windows)]
pub fn host_lan_ip() -> Option<String> {
    let ps = "(Get-NetIPAddress -AddressFamily IPv4 | Where-Object { \
              $_.InterfaceAlias -like 'Wi-Fi*' -and $_.IPAddress -notlike '169.254*' } | \
              Select-Object -First 1).IPAddress";
    let out = std::process::Command::new("powershell")
        .args(["-NoProfile", "-Command", ps])
        .output()
        .ok()?;
    let ip = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if ip.is_empty() { None } else { Some(ip) }
}

#[cfg(not(windows))]
pub fn host_lan_ip() -> Option<String> {
    None
}
