//! ESP32 onboarding actions (backend "C"): flash firmware, provision NVS, and
//! serial-verify. Done directly via `esptool`/`pyserial` subprocesses + the
//! copied golden NVS generator — no desktop code touched.

use anyhow::{Context, Result};
use std::path::PathBuf;
use std::process::Command;

use crate::nvs::{generate_nvs_csi_cfg, Esp32NvsConfig};

/// Public GitHub repo + tag that publishes the ESP32 firmware release.
pub const FIRMWARE_REPO: &str = "ruvnet/RuView";
pub const DEFAULT_TAG: &str = "v0.8.3-esp32";

/// The 4 release assets + flash offsets for the 8 MB S3 layout.
const ASSETS: [(&str, &str); 4] = [
    ("bootloader.bin", "0x0"),
    ("partition-table.bin", "0x8000"),
    ("ota_data_initial.bin", "0xf000"),
    ("esp32-csi-node-s3-8mb.bin", "0x20000"),
];

const NVS_OFFSET: &str = "0x9000";

/// Firmware download cache: `%LOCALAPPDATA%\Wave\firmware\<tag>` (Windows) or
/// `$TMPDIR/Wave-firmware/<tag>`.
fn cache_dir(tag: &str) -> Result<PathBuf> {
    let base = std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    let dir = base.join("Wave").join("firmware").join(tag);
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}

/// Run `python -m esptool <args>` inheriting stdio so progress shows live.
fn run_esptool(args: &[String]) -> Result<()> {
    let mut full = vec!["-m".to_string(), "esptool".to_string()];
    full.extend_from_slice(args);
    let status = Command::new("python")
        .args(&full)
        .status()
        .context("running esptool (is Python + esptool installed? `pip install esptool`)")?;
    if !status.success() {
        anyhow::bail!("esptool failed (exit {:?})", status.code());
    }
    Ok(())
}

/// Flash the 4-segment firmware bundle (downloads + caches release assets).
pub async fn flash(port: &str, chip: &str, tag: &str, baud: u32) -> Result<()> {
    let dir = cache_dir(tag)?;
    let http = reqwest::Client::new();
    let mut args: Vec<String> = vec![
        "--chip".into(), chip.into(),
        "--port".into(), port.into(),
        "--baud".into(), baud.to_string(),
        "write-flash".into(),
        "--flash-mode".into(), "dio".into(),
        "--flash-size".into(), "8MB".into(),
    ];
    for (asset, off) in ASSETS {
        let dest = dir.join(asset);
        if !dest.exists() {
            let url = format!("https://github.com/{FIRMWARE_REPO}/releases/download/{tag}/{asset}");
            eprintln!("downloading {asset} …");
            let resp = http.get(&url).send().await.with_context(|| format!("GET {url}"))?;
            let resp = resp.error_for_status().with_context(|| {
                format!("download {asset} failed (is release '{tag}' published?)")
            })?;
            let bytes = resp.bytes().await?;
            std::fs::write(&dest, &bytes)?;
        }
        args.push(off.to_string());
        args.push(dest.to_string_lossy().to_string());
    }
    eprintln!("flashing {chip} on {port} …");
    run_esptool(&args)
}

/// Download (and cache) the release firmware bundle without flashing. Returns
/// the local paths + offsets so a later `verify`/`flash` reuses the cache.
pub async fn firmware_fetch(tag: &str) -> Result<serde_json::Value> {
    let dir = cache_dir(tag)?;
    let http = reqwest::Client::new();
    let mut files = Vec::new();
    for (asset, off) in ASSETS {
        let dest = dir.join(asset);
        if !dest.exists() {
            let url = format!("https://github.com/{FIRMWARE_REPO}/releases/download/{tag}/{asset}");
            eprintln!("downloading {asset} …");
            let resp = http.get(&url).send().await.with_context(|| format!("GET {url}"))?;
            let resp = resp.error_for_status().with_context(|| {
                format!("download {asset} failed (is release '{tag}' published?)")
            })?;
            std::fs::write(&dest, &resp.bytes().await?)?;
        }
        files.push(serde_json::json!({
            "asset": asset,
            "offset": off,
            "path": dest.to_string_lossy(),
            "size": std::fs::metadata(&dest).map(|m| m.len()).unwrap_or(0),
        }));
    }
    Ok(serde_json::json!({ "tag": tag, "dir": dir.to_string_lossy(), "files": files }))
}

/// Verify the flashed firmware against the release bundle (esptool verify-flash).
pub async fn verify(port: &str, chip: &str, tag: &str, baud: u32) -> Result<()> {
    let dir = cache_dir(tag)?;
    // Ensure the bundle is present locally to compare against.
    firmware_fetch(tag).await?;
    let mut args: Vec<String> = vec![
        "--chip".into(), chip.into(),
        "--port".into(), port.into(),
        "--baud".into(), baud.to_string(),
        "verify-flash".into(),
        "--flash-size".into(), "8MB".into(),
    ];
    for (asset, off) in ASSETS {
        args.push(off.to_string());
        args.push(dir.join(asset).to_string_lossy().to_string());
    }
    eprintln!("verifying {chip} on {port} …");
    run_esptool(&args)
}

/// Provision the `csi_cfg` NVS (WiFi creds + hub target) and write it at 0x9000.
pub fn provision(
    port: &str,
    chip: &str,
    ssid: &str,
    password: &str,
    target_ip: &str,
    target_port: u16,
    node_id: u8,
    baud: u32,
) -> Result<()> {
    let cfg = Esp32NvsConfig {
        ssid: ssid.to_string(),
        password: password.to_string(),
        target_ip: target_ip.to_string(),
        target_port,
        node_id,
        tdm_slot: None,
        tdm_nodes: None,
    };
    let image = generate_nvs_csi_cfg(&cfg).map_err(|e| anyhow::anyhow!("NVS gen: {e}"))?;
    let mut tmp = std::env::temp_dir();
    tmp.push(format!("wave-nvs-{}.bin", std::process::id()));
    std::fs::write(&tmp, &image)?;
    let args: Vec<String> = vec![
        "--chip".into(), chip.into(),
        "--port".into(), port.into(),
        "--baud".into(), baud.to_string(),
        "write-flash".into(),
        NVS_OFFSET.into(),
        tmp.to_string_lossy().to_string(),
    ];
    let r = run_esptool(&args);
    let _ = std::fs::remove_file(&tmp);
    r
}

/// Erase the node's NVS region (0x9000, 0x6000) — clears provisioning.
pub fn erase_nvs(port: &str, chip: &str, baud: u32) -> Result<()> {
    let args: Vec<String> = vec![
        "--chip".into(), chip.into(),
        "--port".into(), port.into(),
        "--baud".into(), baud.to_string(),
        "erase-region".into(),
        "0x9000".into(),
        "0x6000".into(),
    ];
    run_esptool(&args)
}

/// Read the raw NVS region to a local file (for inspection / backup).
pub fn read_nvs(port: &str, chip: &str, out_file: &str, baud: u32) -> Result<()> {
    let args: Vec<String> = vec![
        "--chip".into(), chip.into(),
        "--port".into(), port.into(),
        "--baud".into(), baud.to_string(),
        "read-flash".into(),
        "0x9000".into(),
        "0x6000".into(),
        out_file.into(),
    ];
    run_esptool(&args)
}

/// The chips wave-cli's onboarding supports.
pub fn supported_chips() -> serde_json::Value {
    serde_json::json!({ "chips": ["esp32s3", "esp32c6", "esp32", "esp32s2"] })
}

/// Read the serial boot log to confirm WiFi association. Returns (joined, ip, log_tail).
pub fn serial_check(port: &str, timeout_secs: u64) -> Result<(bool, Option<String>, String)> {
    let script = r#"import serial, sys, time, re
port = sys.argv[1]
deadline = time.time() + int(sys.argv[2])
try:
    s = serial.Serial(port, 115200, timeout=1)
except Exception as e:
    print("PORTERR"); print(""); print(str(e)); sys.exit(0)
try:
    s.setDTR(False); s.setRTS(True); time.sleep(0.1); s.setRTS(False)
except Exception:
    pass
joined = False; ip = ""; buf = b""
while time.time() < deadline:
    try:
        buf += s.read(4096)
    except Exception:
        break
    text = buf.decode("utf-8", errors="replace")
    if "Connected to WiFi" in text:
        joined = True
    m = re.search(r"Got IP:\s*(\d{1,3}\.\d{1,3}\.\d{1,3}\.\d{1,3})", text)
    if m:
        ip = m.group(1)
    if joined and ip:
        break
try:
    s.close()
except Exception:
    pass
text = buf.decode("utf-8", errors="replace")
print("JOINED" if joined else "TIMEOUT")
print(ip)
print(text[-1200:])
"#;
    let mut tmp = std::env::temp_dir();
    tmp.push(format!("wave-serialcheck-{}.py", std::process::id()));
    std::fs::write(&tmp, script)?;
    let out = Command::new("python")
        .arg("-X").arg("utf8")
        .arg(&tmp)
        .arg(port)
        .arg(timeout_secs.to_string())
        .output();
    let _ = std::fs::remove_file(&tmp);
    let out = out.context("running serial check (Python + pyserial required)")?;
    let stdout = String::from_utf8_lossy(&out.stdout);
    let mut lines = stdout.splitn(3, '\n');
    let status = lines.next().unwrap_or("").trim().to_string();
    let ip = lines.next().unwrap_or("").trim().to_string();
    let log = lines.next().unwrap_or("").to_string();
    if status == "PORTERR" {
        anyhow::bail!("could not open {port} (a monitor/flash still using it?): {}", log.trim());
    }
    Ok((status == "JOINED", if ip.is_empty() { None } else { Some(ip) }, log))
}
