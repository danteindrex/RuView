use std::path::PathBuf;
use std::time::Duration;

use anyhow::Result;
use clap::Parser;
use tokio::net::UdpSocket;
use tracing::{debug, info, warn};

use wifi_densepose_pi_node_agent::config::AgentConfig;
use wifi_densepose_pi_node_agent::edge_dsp::{process_frame, EdgeDspState};
use wifi_densepose_pi_node_agent::frame_encoder::{
    encode_feature_packet, encode_fused_vitals_packet, encode_raw_frame, encode_vitals_packet,
    encode_wasm_v2_packet, EdgeVitals,
};
use wifi_densepose_pi_node_agent::mmwave::{
    fuse_with_mmwave, MmwaveReader, MmwaveState, MockMmwaveReader,
};
use wifi_densepose_pi_node_agent::nexmon_capture::{nexmon_to_raw_frame, parse_nexmon_payload};
use wifi_densepose_pi_node_agent::wasm_runtime::{EdgeFrameContext, WasmRuntime};

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// UDP listen endpoint for Nexmon CSI packets.
    #[arg(long, default_value = "0.0.0.0:5500")]
    listen: String,

    /// Aggregator endpoint (sensing-server UDP).
    #[arg(long, default_value = "127.0.0.1:5005")]
    aggregator: String,

    /// Base node ID used when mapping Nexmon core/ss to logical node IDs.
    #[arg(long, default_value_t = 10)]
    node_base: u8,

    /// Tier level: >=2 enables compressed packet emission.
    #[arg(long, default_value_t = 2)]
    tier: u8,

    /// Default RSSI used when packet cannot infer power.
    #[arg(long, default_value_t = -55)]
    default_rssi: i8,

    /// Noise floor dBm attached to emitted raw frame packets.
    #[arg(long, default_value_t = -92)]
    noise_floor: i8,

    /// Enable synthetic mmWave fusion state for parity path verification.
    #[arg(long, default_value_t = false)]
    mmwave_mock: bool,

    /// Enable WASM event generation path.
    #[arg(long, default_value_t = false)]
    enable_wasm: bool,

    /// Optional path to a WASM module.
    #[arg(long)]
    wasm_path: Option<PathBuf>,

    /// Logical module id to include in WASM v2 packet.
    #[arg(long, default_value_t = 1)]
    wasm_module_id: u8,
}

fn build_config(args: Args) -> Result<AgentConfig> {
    AgentConfig::from_strings(
        &args.listen,
        &args.aggregator,
        args.node_base,
        args.default_rssi,
        args.noise_floor,
        args.tier,
        args.mmwave_mock,
        args.enable_wasm,
        args.wasm_path,
        args.wasm_module_id,
    )
}

fn build_wasm_runtime(config: &AgentConfig) -> Result<WasmRuntime> {
    if !config.enable_wasm {
        return Ok(WasmRuntime::disabled(config.wasm_module_id));
    }
    match &config.wasm_path {
        Some(path) => match WasmRuntime::from_module(path, config.wasm_module_id) {
            Ok(runtime) => Ok(runtime),
            Err(err) => {
                warn!(
                    "failed to load wasm module '{}': {}; using synthetic runtime",
                    path.display(),
                    err
                );
                Ok(WasmRuntime::enabled_without_module(config.wasm_module_id))
            }
        },
        None => Ok(WasmRuntime::enabled_without_module(config.wasm_module_id)),
    }
}

fn mmwave_state_from_vitals(vitals: &EdgeVitals) -> MmwaveState {
    MmwaveState {
        presence_score: vitals.presence_score.max(0.35),
        motion_energy: vitals.motion_energy,
        distance_m: 1.5,
        fall_detected: vitals.fall_detected,
        n_persons: vitals.n_persons.max(1),
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();

    let args = Args::parse();
    let config = build_config(args)?;
    run(config).await
}

async fn run(config: AgentConfig) -> Result<()> {
    info!("RuView Pi Node Agent starting");
    info!("  listen: {}", config.listen_addr);
    info!("  aggregator: {}", config.aggregator_addr);
    info!("  node_base: {}", config.node_base);
    info!("  tier: {}", config.tier);
    info!("  mmwave_mock: {}", config.enable_mmwave_mock);
    info!("  wasm: {}", config.enable_wasm);

    let socket = UdpSocket::bind(config.listen_addr).await?;
    let uplink = UdpSocket::bind("0.0.0.0:0").await?;
    let mut dsp = EdgeDspState::new(config.tier);
    let mut wasm = build_wasm_runtime(&config)?;
    let mut mmwave = if config.enable_mmwave_mock {
        Some(MockMmwaveReader::default())
    } else {
        None
    };

    let mut buf = vec![0u8; 8192];
    loop {
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {
                info!("shutdown signal received");
                break;
            }
            recv = socket.recv_from(&mut buf) => {
                let (len, src) = match recv {
                    Ok(v) => v,
                    Err(err) => {
                        warn!("udp recv error: {err}");
                        tokio::time::sleep(Duration::from_millis(10)).await;
                        continue;
                    }
                };

                let payload = &buf[..len];
                let Some(nexmon) = parse_nexmon_payload(payload) else {
                    debug!("dropping non-nexmon payload from {src}");
                    continue;
                };
                debug!("nexmon pkt from {src}: seq={} core_ss={} chip=0x{:04x}", nexmon.seq, nexmon.core_ss, nexmon.chip);

                let frame = nexmon_to_raw_frame(&nexmon, config.node_base, config.default_rssi, config.noise_floor);
                let raw_packet = encode_raw_frame(&frame);
                uplink.send_to(&raw_packet, config.aggregator_addr).await?;

                let now = chrono::Utc::now();
                let now_ms = now.timestamp_millis().max(0) as u64;
                let now_us = now.timestamp_micros();

                let outputs = process_frame(&mut dsp, &frame, now_ms);

                if let Some(vitals) = outputs.vitals.clone() {
                    let packet = encode_vitals_packet(&vitals);
                    uplink.send_to(&packet, config.aggregator_addr).await?;

                    if let Some(reader) = mmwave.as_mut() {
                        reader.push(mmwave_state_from_vitals(&vitals));
                        if let Some(mmwave_state) = reader.poll() {
                            let fused = fuse_with_mmwave(&vitals, &mmwave_state);
                            let packet = encode_fused_vitals_packet(&fused);
                            uplink.send_to(&packet, config.aggregator_addr).await?;
                        }
                    }

                    if let Some(features) = outputs.feature {
                        let packet = encode_feature_packet(
                            frame.node_id,
                            (frame.sequence & 0xffff) as u16,
                            now_us,
                            features,
                        );
                        uplink.send_to(&packet, config.aggregator_addr).await?;
                    }

                    let ctx = EdgeFrameContext {
                        node_id: frame.node_id,
                        sequence: frame.sequence,
                        motion_energy: vitals.motion_energy,
                        presence_score: vitals.presence_score,
                        timestamp_ms: now_ms,
                    };
                    let events = wasm.on_frame(&ctx);
                    if !events.is_empty() {
                        let packet = encode_wasm_v2_packet(ctx.node_id, wasm.module_id(), &events);
                        uplink.send_to(&packet, config.aggregator_addr).await?;
                    }
                }

                if let Some(compressed) = outputs.compressed {
                    uplink.send_to(&compressed, config.aggregator_addr).await?;
                }
            }
        }
    }

    info!("RuView Pi Node Agent stopped");
    Ok(())
}
