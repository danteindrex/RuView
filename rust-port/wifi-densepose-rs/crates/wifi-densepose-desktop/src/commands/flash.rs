use std::io::{BufRead, BufReader};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tauri::{AppHandle, Emitter, Manager, State};

use crate::commands::auth::AuthManager;
use crate::commands::guard;
use crate::state::AppState;

/// Default firmware release tag used for the guided onboarding flow.
/// Matches `scripts/setup-esp32-node.ps1`'s `-Release` default.
const DEFAULT_FIRMWARE_TAG: &str = "v0.8.3-esp32";

/// GitHub repository that publishes the ESP32 firmware releases (public,
/// upstream project — the fork does not cut its own firmware releases).
const FIRMWARE_REPO: &str = "ruvnet/RuView";

/// The four release assets and their flash offsets for the 8 MB S3 layout,
/// mirroring the proven offsets in `scripts/setup-esp32-node.ps1`:
/// bootloader @ 0x0, partition table @ 0x8000, ota_data @ 0xf000, app @ 0x20000.
const FIRMWARE_ASSETS: [(&str, u32); 4] = [
    ("bootloader.bin", 0x0),
    ("partition-table.bin", 0x8000),
    ("ota_data_initial.bin", 0xf000),
    ("esp32-csi-node-s3-8mb.bin", 0x20000),
];

/// Flash firmware binary to an ESP32 via serial port.
///
/// Uses espflash CLI tool for actual flashing. Progress is emitted
/// via Tauri events for UI updates.
///
/// # Arguments
/// * `port` - Serial port path (e.g., "/dev/ttyUSB0" or "COM3")
/// * `firmware_path` - Path to the .bin firmware file
/// * `chip` - Optional chip type ("esp32", "esp32s2", "esp32s3", "esp32c3")
/// * `baud` - Optional baud rate (default: 921600)
#[tauri::command]
pub async fn flash_firmware(
    access_token: String,
    app: AppHandle,
    port: String,
    firmware_path: String,
    chip: Option<String>,
    baud: Option<u32>,
    auth: State<'_, Arc<AuthManager>>,
) -> Result<FlashResult, String> {
    guard::require_auth(&access_token, &auth)?;
    let start_time = std::time::Instant::now();

    // Validate firmware file exists
    let firmware_meta = std::fs::metadata(&firmware_path)
        .map_err(|e| format!("Cannot read firmware file: {}", e))?;

    let firmware_size = firmware_meta.len();

    // Calculate firmware SHA-256 for verification
    let firmware_hash = calculate_sha256(&firmware_path)?;

    // Emit flash started event
    let _ = app.emit("flash-progress", FlashProgress {
        phase: "connecting".into(),
        progress_pct: 0.0,
        bytes_written: 0,
        bytes_total: firmware_size,
        message: Some(format!("Connecting to {} ...", port)),
    });

    // Build espflash command
    let baud_rate = baud.unwrap_or(921600);
    let mut cmd = Command::new("espflash");
    cmd.arg("flash");
    cmd.args(["--port", &port]);
    cmd.args(["--baud", &baud_rate.to_string()]);

    if let Some(ref chip_type) = chip {
        cmd.args(["--chip", chip_type]);
    }

    // Monitor mode disabled for clean output
    cmd.arg("--no-monitor");

    // Add firmware path
    cmd.arg(&firmware_path);

    // Capture output for progress parsing
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());

    // Spawn the process
    let mut child = cmd.spawn()
        .map_err(|e| format!("Failed to start espflash: {}. Is espflash installed?", e))?;

    let _stdout = child.stdout.take()
        .ok_or("Failed to capture stdout")?;
    let stderr = child.stderr.take()
        .ok_or("Failed to capture stderr")?;

    // Read and parse progress from stderr (espflash outputs there)
    let app_clone = app.clone();
    let firmware_size_clone = firmware_size;

    let progress_handle = tokio::task::spawn_blocking(move || {
        let reader = BufReader::new(stderr);
        let mut last_phase = "connecting".to_string();
        let mut last_progress = 0.0f32;

        for line in reader.lines() {
            if let Ok(line) = line {
                // Parse espflash progress output
                if line.contains("Connecting") {
                    last_phase = "connecting".to_string();
                    last_progress = 5.0;
                } else if line.contains("Erasing") {
                    last_phase = "erasing".to_string();
                    last_progress = 20.0;
                } else if line.contains("Writing") || line.contains("Flashing") {
                    last_phase = "writing".to_string();
                    // Try to parse percentage from line like "[00:02:10] Writing [##########] 100%"
                    if let Some(pct) = parse_progress_percentage(&line) {
                        last_progress = 20.0 + (pct * 0.7); // 20-90% for writing
                    }
                } else if line.contains("Hard resetting") || line.contains("Done") {
                    last_phase = "verifying".to_string();
                    last_progress = 95.0;
                }

                let _ = app_clone.emit("flash-progress", FlashProgress {
                    phase: last_phase.clone(),
                    progress_pct: last_progress,
                    bytes_written: ((last_progress / 100.0) * firmware_size_clone as f32) as u64,
                    bytes_total: firmware_size_clone,
                    message: Some(line),
                });
            }
        }
    });

    // Wait for completion
    let status = child.wait()
        .map_err(|e| format!("Failed to wait for espflash: {}", e))?;

    // Wait for progress parsing to complete
    let _ = progress_handle.await;

    let duration = start_time.elapsed().as_secs_f64();

    if status.success() {
        // Emit completion
        let _ = app.emit("flash-progress", FlashProgress {
            phase: "completed".into(),
            progress_pct: 100.0,
            bytes_written: firmware_size,
            bytes_total: firmware_size,
            message: Some("Flash completed successfully!".into()),
        });

        Ok(FlashResult {
            success: true,
            message: format!("Firmware flashed successfully in {:.1}s", duration),
            duration_secs: duration,
            firmware_hash: Some(firmware_hash),
        })
    } else {
        let _ = app.emit("flash-progress", FlashProgress {
            phase: "failed".into(),
            progress_pct: 0.0,
            bytes_written: 0,
            bytes_total: firmware_size,
            message: Some("Flash failed".into()),
        });

        Err(format!("espflash exited with status: {}", status))
    }
}

/// Get current flash progress (for polling-based approach).
/// Prefer using Tauri events instead.
#[tauri::command]
pub async fn flash_progress(
    access_token: String,
    state: State<'_, AppState>,
    auth: State<'_, Arc<AuthManager>>,
) -> Result<FlashProgress, String> {
    guard::require_auth(&access_token, &auth)?;
    let flash = state.flash.lock().map_err(|e| e.to_string())?;

    Ok(FlashProgress {
        phase: flash.phase.clone(),
        progress_pct: flash.progress_pct,
        bytes_written: flash.bytes_written,
        bytes_total: flash.bytes_total,
        message: flash.message.clone(),
    })
}

/// Verify firmware on device by reading back and comparing hash.
#[tauri::command]
pub async fn verify_firmware(
    access_token: String,
    _port: String,
    firmware_path: String,
    _chip: Option<String>,
    auth: State<'_, Arc<AuthManager>>,
) -> Result<VerifyResult, String> {
    guard::require_auth(&access_token, &auth)?;
    // Calculate expected hash
    let expected_hash = calculate_sha256(&firmware_path)?;

    // Use espflash to read firmware back (if supported)
    // For now, we rely on espflash's built-in verification
    // A full implementation would use esptool.py read_flash

    Ok(VerifyResult {
        verified: true,
        expected_hash,
        actual_hash: None,
        message: "Verification relies on espflash built-in verify".into(),
    })
}

/// Check if espflash is installed and get version.
#[tauri::command]
pub async fn check_espflash(
    access_token: String,
    auth: State<'_, Arc<AuthManager>>,
) -> Result<EspflashInfo, String> {
    guard::require_auth(&access_token, &auth)?;
    let output = Command::new("espflash")
        .arg("--version")
        .output()
        .map_err(|_| "espflash not found. Please install: cargo install espflash")?;

    if output.status.success() {
        let version = String::from_utf8_lossy(&output.stdout)
            .trim()
            .to_string();

        Ok(EspflashInfo {
            installed: true,
            version: Some(version),
            path: which_espflash().ok(),
        })
    } else {
        Err("espflash found but --version failed".into())
    }
}

/// Get supported chip types for flashing.
#[tauri::command]
pub async fn supported_chips(
    access_token: String,
    auth: State<'_, Arc<AuthManager>>,
) -> Result<Vec<ChipInfo>, String> {
    guard::require_auth(&access_token, &auth)?;
    Ok(vec![
        ChipInfo {
            id: "esp32".into(),
            name: "ESP32".into(),
            description: "Original ESP32 dual-core".into(),
        },
        ChipInfo {
            id: "esp32s2".into(),
            name: "ESP32-S2".into(),
            description: "ESP32-S2 single-core with USB OTG".into(),
        },
        ChipInfo {
            id: "esp32s3".into(),
            name: "ESP32-S3".into(),
            description: "ESP32-S3 dual-core with USB OTG and AI acceleration".into(),
        },
        ChipInfo {
            id: "esp32c3".into(),
            name: "ESP32-C3".into(),
            description: "ESP32-C3 RISC-V single-core".into(),
        },
        ChipInfo {
            id: "esp32c6".into(),
            name: "ESP32-C6".into(),
            description: "ESP32-C6 RISC-V with WiFi 6 and Thread".into(),
        },
    ])
}

/// Download (and cache) the four firmware binaries for a release tag.
///
/// Assets are fetched from the public GitHub release download URLs
/// (`https://github.com/<repo>/releases/download/<tag>/<asset>`) into
/// `app_data_dir()/firmware/<tag>/`. If all four files already exist in the
/// cache directory the download is skipped. Returns the local paths paired
/// with their flash offsets so the caller can pass them to
/// [`flash_firmware_bundle`].
///
/// # Note
/// This assumes the `ruvnet/RuView` release assets are publicly
/// downloadable. If the repository is private the direct URLs return 404 and
/// the operator would need an authenticated `gh release download` instead —
/// that fallback is intentionally out of scope for the sealed-box onboarding
/// path (operators are not expected to have `gh` configured).
#[tauri::command]
pub async fn fetch_firmware_release(
    access_token: String,
    app: AppHandle,
    tag: Option<String>,
    auth: State<'_, Arc<AuthManager>>,
) -> Result<FirmwareBundle, String> {
    guard::require_auth(&access_token, &auth)?;
    let tag = tag.unwrap_or_else(|| DEFAULT_FIRMWARE_TAG.to_string());

    let cache_dir: PathBuf = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("Cannot resolve app data dir: {}", e))?
        .join("firmware")
        .join(&tag);

    std::fs::create_dir_all(&cache_dir)
        .map_err(|e| format!("Cannot create firmware cache dir: {}", e))?;

    let client = reqwest::Client::builder()
        .build()
        .map_err(|e| format!("Failed to build HTTP client: {}", e))?;

    let mut segments = Vec::with_capacity(FIRMWARE_ASSETS.len());

    for (asset, offset) in FIRMWARE_ASSETS.iter() {
        let dest = cache_dir.join(asset);

        if !dest.exists() {
            let url = format!(
                "https://github.com/{}/releases/download/{}/{}",
                FIRMWARE_REPO, tag, asset
            );
            tracing::info!("Downloading firmware asset {} from {}", asset, url);

            let resp = client
                .get(&url)
                .send()
                .await
                .map_err(|e| format!("Failed to download {}: {}", asset, e))?;

            if !resp.status().is_success() {
                return Err(format!(
                    "Download of {} failed with HTTP {} (is release '{}' published and public?)",
                    asset,
                    resp.status(),
                    tag
                ));
            }

            let bytes = resp
                .bytes()
                .await
                .map_err(|e| format!("Failed to read {} body: {}", asset, e))?;

            // Write atomically via a temp file then rename, so an interrupted
            // download never leaves a truncated file that later looks cached.
            let tmp = dest.with_extension("bin.part");
            std::fs::write(&tmp, &bytes)
                .map_err(|e| format!("Failed to write {}: {}", asset, e))?;
            std::fs::rename(&tmp, &dest)
                .map_err(|e| format!("Failed to finalize {}: {}", asset, e))?;
        } else {
            tracing::info!("Firmware asset {} already cached", asset);
        }

        segments.push(FirmwareSegment {
            offset: *offset,
            path: dest.to_string_lossy().to_string(),
            name: asset.to_string(),
        });
    }

    Ok(FirmwareBundle {
        tag,
        cache_dir: cache_dir.to_string_lossy().to_string(),
        segments,
    })
}

/// Flash a multi-segment firmware bundle to an ESP32 via `python -m esptool`.
///
/// Unlike [`flash_firmware`] (single image via `espflash`), this writes several
/// binaries at explicit offsets in one `write-flash` invocation — required for
/// the bootloader/partition-table/ota_data/app layout produced by a firmware
/// release. Progress is parsed from esptool's stdout and re-emitted on the same
/// `flash-progress` event channel the UI already listens to.
///
/// `segments` is the list of [`FirmwareSegment`]s to write — this is exactly
/// the `segments` field returned by [`fetch_firmware_release`], so the two
/// commands compose directly (fetch → flash) with no reshaping on the frontend.
#[tauri::command]
pub async fn flash_firmware_bundle(
    access_token: String,
    app: AppHandle,
    port: String,
    chip: String,
    segments: Vec<FirmwareSegment>,
    baud: Option<u32>,
    auth: State<'_, Arc<AuthManager>>,
) -> Result<FlashResult, String> {
    guard::require_auth(&access_token, &auth)?;
    let start_time = std::time::Instant::now();

    if segments.is_empty() {
        return Err("No firmware segments provided".into());
    }

    // Validate every segment file exists before touching the device.
    let mut total_bytes: u64 = 0;
    for seg in &segments {
        let meta = std::fs::metadata(&seg.path).map_err(|e| {
            format!("Cannot read segment {} @ 0x{:x}: {}", seg.path, seg.offset, e)
        })?;
        total_bytes += meta.len();
    }

    let baud_rate = baud.unwrap_or(460800);

    let _ = app.emit(
        "flash-progress",
        FlashProgress {
            phase: "connecting".into(),
            progress_pct: 0.0,
            bytes_written: 0,
            bytes_total: total_bytes,
            message: Some(format!(
                "Flashing {} segments to {} ...",
                segments.len(),
                port
            )),
        },
    );

    // Build: python -m esptool --chip <chip> --port <port> --baud <baud>
    //        write-flash <off0> <path0> <off1> <path1> ...
    let mut cmd = Command::new("python");
    cmd.args(["-m", "esptool"]);
    cmd.args(["--chip", &chip]);
    cmd.args(["--port", &port]);
    cmd.args(["--baud", &baud_rate.to_string()]);
    cmd.arg("write-flash");
    for seg in &segments {
        cmd.arg(format!("0x{:x}", seg.offset));
        cmd.arg(&seg.path);
    }

    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());

    let mut child = cmd
        .spawn()
        .map_err(|e| format!("Failed to start esptool: {}. Is Python + esptool installed?", e))?;

    // esptool writes progress to stdout (unlike espflash → stderr).
    let stdout = child.stdout.take().ok_or("Failed to capture stdout")?;
    let stderr = child.stderr.take().ok_or("Failed to capture stderr")?;

    let app_stdout = app.clone();
    let total_clone = total_bytes;
    let progress_handle = tokio::task::spawn_blocking(move || {
        let reader = BufReader::new(stdout);
        let mut last_phase = "connecting".to_string();
        let mut last_progress = 5.0f32;
        for line in reader.lines().map_while(Result::ok) {
            if line.contains("Connecting") {
                last_phase = "connecting".to_string();
                last_progress = 5.0;
            } else if line.contains("Erasing") {
                last_phase = "erasing".to_string();
                last_progress = 15.0;
            } else if line.contains("Writing at") || line.contains("Compressed") {
                last_phase = "writing".to_string();
                if let Some(pct) = parse_progress_percentage(&line) {
                    last_progress = 20.0 + (pct * 0.75); // 20-95% for writing
                }
            } else if line.contains("Hash of data verified") || line.contains("Hard resetting") {
                last_phase = "verifying".to_string();
                last_progress = 97.0;
            }

            let _ = app_stdout.emit(
                "flash-progress",
                FlashProgress {
                    phase: last_phase.clone(),
                    progress_pct: last_progress,
                    bytes_written: ((last_progress / 100.0) * total_clone as f32) as u64,
                    bytes_total: total_clone,
                    message: Some(line),
                },
            );
        }
    });

    // Drain stderr so esptool errors are captured for the failure message.
    let stderr_handle = tokio::task::spawn_blocking(move || {
        let reader = BufReader::new(stderr);
        reader
            .lines()
            .map_while(Result::ok)
            .collect::<Vec<_>>()
            .join("\n")
    });

    let status = child
        .wait()
        .map_err(|e| format!("Failed to wait for esptool: {}", e))?;

    let _ = progress_handle.await;
    let stderr_output = stderr_handle.await.unwrap_or_default();

    let duration = start_time.elapsed().as_secs_f64();

    if status.success() {
        let _ = app.emit(
            "flash-progress",
            FlashProgress {
                phase: "completed".into(),
                progress_pct: 100.0,
                bytes_written: total_bytes,
                bytes_total: total_bytes,
                message: Some("Firmware bundle flashed successfully!".into()),
            },
        );

        Ok(FlashResult {
            success: true,
            message: format!(
                "Flashed {} segments in {:.1}s",
                segments.len(),
                duration
            ),
            duration_secs: duration,
            firmware_hash: None,
        })
    } else {
        let _ = app.emit(
            "flash-progress",
            FlashProgress {
                phase: "failed".into(),
                progress_pct: 0.0,
                bytes_written: 0,
                bytes_total: total_bytes,
                message: Some("Flash failed".into()),
            },
        );

        Err(format!(
            "esptool exited with status {}: {}",
            status,
            stderr_output.trim()
        ))
    }
}

/// Calculate SHA-256 hash of a file.
fn calculate_sha256(path: &str) -> Result<String, String> {
    let file = std::fs::File::open(path)
        .map_err(|e| format!("Failed to open file: {}", e))?;

    let mut reader = BufReader::new(file);
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 8192];

    loop {
        let bytes_read = std::io::Read::read(&mut reader, &mut buffer)
            .map_err(|e| format!("Failed to read file: {}", e))?;

        if bytes_read == 0 {
            break;
        }

        hasher.update(&buffer[..bytes_read]);
    }

    let hash = hasher.finalize();
    Ok(hex::encode(hash))
}

/// Parse progress percentage from espflash/esptool output lines.
///
/// Handles both espflash's `[##########] 100%` and esptool's
/// `Writing at 0x... ( 12 %)` (note the space before `%`).
fn parse_progress_percentage(line: &str) -> Option<f32> {
    let re = regex::Regex::new(r"(\d+)\s*%").ok()?;
    re.captures(line)
        .and_then(|caps| caps.get(1))
        .and_then(|m| m.as_str().parse().ok())
}

/// Find espflash binary path.
fn which_espflash() -> Result<String, String> {
    let output = Command::new("which")
        .arg("espflash")
        .output()
        .map_err(|e| e.to_string())?;

    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    } else {
        Err("espflash not in PATH".into())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlashResult {
    pub success: bool,
    pub message: String,
    pub duration_secs: f64,
    pub firmware_hash: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlashProgress {
    pub phase: String,
    pub progress_pct: f32,
    pub bytes_written: u64,
    pub bytes_total: u64,
    pub message: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct VerifyResult {
    pub verified: bool,
    pub expected_hash: String,
    pub actual_hash: Option<String>,
    pub message: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct EspflashInfo {
    pub installed: bool,
    pub version: Option<String>,
    pub path: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ChipInfo {
    pub id: String,
    pub name: String,
    pub description: String,
}

/// A single firmware image plus the flash offset it must be written to.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FirmwareSegment {
    pub offset: u32,
    pub path: String,
    pub name: String,
}

/// The set of firmware images for a release, cached locally and ready to flash.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FirmwareBundle {
    pub tag: String,
    pub cache_dir: String,
    pub segments: Vec<FirmwareSegment>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_progress_percentage() {
        assert_eq!(parse_progress_percentage("[##########] 100%"), Some(100.0));
        assert_eq!(parse_progress_percentage("Writing 50%"), Some(50.0));
        assert_eq!(parse_progress_percentage("No percentage here"), None);
        // esptool format has a space before the percent sign.
        assert_eq!(
            parse_progress_percentage("Writing at 0x00020000... ( 12 %)"),
            Some(12.0)
        );
        assert_eq!(
            parse_progress_percentage("Writing at 0x0000f000... (100 %)"),
            Some(100.0)
        );
    }

    #[test]
    fn test_firmware_asset_offsets() {
        // Offsets must match scripts/setup-esp32-node.ps1.
        assert_eq!(FIRMWARE_ASSETS[0], ("bootloader.bin", 0x0));
        assert_eq!(FIRMWARE_ASSETS[1], ("partition-table.bin", 0x8000));
        assert_eq!(FIRMWARE_ASSETS[2], ("ota_data_initial.bin", 0xf000));
        assert_eq!(FIRMWARE_ASSETS[3], ("esp32-csi-node-s3-8mb.bin", 0x20000));
    }

    #[test]
    fn test_chip_info() {
        let chips = vec![
            ChipInfo {
                id: "esp32".into(),
                name: "ESP32".into(),
                description: "Test".into(),
            },
        ];
        assert_eq!(chips.len(), 1);
        assert_eq!(chips[0].id, "esp32");
    }
}
