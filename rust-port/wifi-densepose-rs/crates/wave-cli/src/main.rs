//! wave-cli — Wave / RuView management CLI.
//!
//! Manages the sensing system from the command line. This foundation covers the
//! read/monitor surface over the sensing-server REST API (backend "A") plus a
//! `doctor` self-check. Hardware (flash/provision), server-lifecycle, and admin
//! (wave.db) command groups build on this skeleton — see
//! `docs/plan-management-cli.md`.
//!
//! Design follows clig.dev: noun-verb commands, TTY-aware output (`-o`), quiet
//! stdout for piping, diagnostics to stderr, and categorized exit codes.

mod admin;
mod client;
mod esp32;
mod hardware;
mod node;
mod nvs;
mod output;
mod pathcmd;
mod pi;
mod serverproc;

use clap::{Args, Parser, Subcommand};

use client::Client;
use output::{Format, Table};

/// Exit-code categories (clig.dev: scripts branch on these).
mod exit {
    pub const USAGE: i32 = 2;
    pub const NETWORK: i32 = 6;
}

#[derive(Parser, Debug)]
#[command(name = "wave-cli", version, about = "Wave / RuView management CLI", propagate_version = true)]
#[command(long_about = "Manage the Wave/RuView sensing system: nodes, live sensing, \
calibration, models, and (soon) hardware provisioning — scriptable and headless.")]
struct Cli {
    /// Output format. `auto` = table on a terminal, JSON when piped.
    #[arg(short, long, global = true, value_enum, default_value = "auto")]
    output: Format,

    /// Sensing-server base URL (overrides $WAVE_URL and the localhost default).
    #[arg(long, global = true, value_name = "URL")]
    url: Option<String>,

    /// Never emit color (also honors $NO_COLOR).
    #[arg(long, global = true)]
    no_color: bool,

    /// Suppress non-essential stderr notes.
    #[arg(short, long, global = true)]
    quiet: bool,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Manage / inspect the sensing server.
    Server(GroupArgs<ServerAction>),
    /// Inspect sensing nodes.
    Node(GroupArgs<NodeAction>),
    /// Inspect live sensing output.
    Sensing(GroupArgs<SensingAction>),
    /// List serial ports (find a connected ESP32).
    Serial(GroupArgs<SerialAction>),
    /// Scan nearby WiFi networks (pick a 2.4GHz SSID to provision).
    Wifi(GroupArgs<WifiAction>),
    /// Onboard an ESP32: flash, provision, verify.
    Esp32(GroupArgs<Esp32Action>),
    /// OTA firmware updates (node-direct HTTP).
    Ota(GroupArgs<OtaAction>),
    /// WASM edge modules on a node (node-direct HTTP).
    Wasm(GroupArgs<WasmAction>),
    /// Raspberry Pi node management over SSH.
    Pi(GroupArgs<PiAction>),
    /// Live terminal dashboard: CSI stream rate, DSP pipeline, ML inference.
    Monitor {
        /// Refresh interval in milliseconds.
        #[arg(long, default_value_t = 500)]
        interval_ms: u64,
    },
    /// Show which backend endpoints the CLI covers.
    Coverage,
    /// Empty-room field-model calibration.
    Calibrate(GroupArgs<CalibrateAction>),
    /// Inspect models (server-loaded + bundled).
    Model(GroupArgs<ModelAction>),
    /// WiFi-derived pose (current, stats, zones).
    Pose(GroupArgs<PoseAction>),
    /// Show live vital signs.
    Vitals,
    /// Record / list / delete CSI sessions.
    Recording(GroupArgs<RecordingAction>),
    /// Training pipeline control.
    Train(GroupArgs<TrainAction>),
    /// Adaptive classifier control.
    Adaptive(GroupArgs<AdaptiveAction>),
    /// FTM ranging (auto node positioning).
    Ranging(GroupArgs<RangingAction>),
    /// User management (reads/writes wave.db).
    User(GroupArgs<UserAction>),
    /// List roles (wave.db).
    Role(GroupArgs<RoleAction>),
    /// List tenants (wave.db).
    Tenant(GroupArgs<TenantAction>),
    /// Show the license status (wave.db).
    License,
    /// Show the plan tier (wave.db).
    Plan,
    /// Add/remove wave-cli from the system PATH (run by the installer, or manually).
    Path(GroupArgs<pathcmd::PathAction>),
    /// Environment + connectivity self-check.
    Doctor,
}

#[derive(Subcommand, Debug)]
enum UserAction {
    /// List users.
    List,
    /// Show one user by email.
    Get { email: String },
    /// Create a user (Argon2id-hashed password).
    Create {
        #[arg(long)]
        email: String,
        #[arg(long)]
        first: String,
        #[arg(long)]
        last: String,
        /// Password (or set WAVE_NEW_PASSWORD).
        #[arg(long)]
        password: Option<String>,
        /// Tenant id (defaults to the first tenant).
        #[arg(long)]
        tenant: Option<String>,
    },
    /// Delete a user by email.
    Delete {
        email: String,
        #[arg(short, long)]
        yes: bool,
    },
}

#[derive(Subcommand, Debug)]
enum RoleAction {
    /// List roles.
    List,
}

#[derive(Subcommand, Debug)]
enum OtaAction {
    /// Check a node's OTA endpoint.
    Check { #[arg(long)] node: String },
    /// Flash firmware to a node over the network.
    Update {
        #[arg(long)]
        node: String,
        #[arg(long)]
        file: String,
        #[arg(long)]
        psk: Option<String>,
    },
    /// Flash firmware to many nodes.
    Batch {
        /// Comma-separated node IPs.
        #[arg(long, value_delimiter = ',')]
        nodes: Vec<String>,
        #[arg(long)]
        file: String,
        #[arg(long)]
        psk: Option<String>,
    },
}

#[derive(Subcommand, Debug)]
enum PiAction {
    /// Probe a Pi over SSH (model, kernel, uptime).
    Probe { #[arg(long)] host: String },
    /// Control the node agent service (start|stop|restart|status).
    Service {
        #[arg(long)]
        host: String,
        #[arg(long)]
        action: String,
        #[arg(long, default_value = "ruview-node")]
        name: String,
    },
    /// Check CSI health (agent active + monitor mode).
    Health {
        #[arg(long)]
        host: String,
        #[arg(long, default_value = "ruview-node")]
        service: String,
    },
}

#[derive(Subcommand, Debug)]
enum WasmAction {
    /// List modules on a node.
    List { #[arg(long)] node: String },
    /// Upload a WASM module to a node.
    Upload {
        #[arg(long)]
        node: String,
        #[arg(long)]
        file: String,
        #[arg(long)]
        name: String,
        #[arg(long)]
        auto_start: bool,
    },
    /// Start a module.
    Start { #[arg(long)] node: String, #[arg(long)] id: String },
    /// Stop a module.
    Stop { #[arg(long)] node: String, #[arg(long)] id: String },
}

#[derive(Subcommand, Debug)]
enum TenantAction {
    /// List tenants.
    List,
}

/// Wrapper so each group gets its own action subcommand.
#[derive(Args, Debug)]
struct GroupArgs<A: Subcommand> {
    #[command(subcommand)]
    action: A,
}

#[derive(Subcommand, Debug)]
enum ServerAction {
    /// Show server status (source, ports, health).
    Status,
    /// Start the bundled sensing-server (detached).
    Start {
        #[arg(long, default_value_t = 4000)]
        port: u16,
        #[arg(long, default_value = "esp32")]
        source: String,
    },
    /// Stop the running sensing-server.
    Stop,
    /// Restart the sensing-server.
    Restart {
        #[arg(long, default_value_t = 4000)]
        port: u16,
        #[arg(long, default_value = "esp32")]
        source: String,
    },
    /// Tail the sensing-server log.
    Logs {
        #[arg(long, default_value_t = 50)]
        lines: usize,
    },
}

#[derive(Subcommand, Debug)]
enum NodeAction {
    /// List all nodes in the streaming roster.
    List,
    /// Show one node by id.
    Get {
        /// Node id.
        id: u64,
    },
    /// Live-refresh the node roster until Ctrl-C.
    Watch {
        /// Refresh interval in seconds.
        #[arg(long, default_value_t = 2)]
        interval: u64,
    },
    /// Show the persisted 3D node-position map.
    Positions,
}

#[derive(Subcommand, Debug)]
enum SensingAction {
    /// Show the latest sensing frame (presence, persons, vitals).
    Latest,
}

#[derive(Subcommand, Debug)]
enum SerialAction {
    /// List serial ports; ESP32-compatible ones are flagged.
    List,
}

#[derive(Subcommand, Debug)]
enum WifiAction {
    /// Scan nearby WiFi networks (SSID + band).
    Scan,
}

#[derive(Subcommand, Debug)]
enum Esp32Action {
    /// Flash the release firmware bundle (downloads + esptool).
    Flash {
        #[arg(long)]
        port: String,
        #[arg(long, default_value = "esp32s3")]
        chip: String,
        #[arg(long, default_value = "v0.8.3-esp32")]
        tag: String,
        #[arg(long, default_value_t = 460800)]
        baud: u32,
    },
    /// Write WiFi credentials + hub target to the node's NVS.
    Provision {
        #[arg(long)]
        port: String,
        #[arg(long, default_value = "esp32s3")]
        chip: String,
        #[arg(long)]
        ssid: String,
        /// WiFi password (or set WAVE_WIFI_PASSWORD, or rely on the saved profile).
        #[arg(long)]
        password: Option<String>,
        /// Hub IP the node streams to (defaults to this PC's Wi-Fi IPv4).
        #[arg(long)]
        target_ip: Option<String>,
        #[arg(long, default_value_t = 5005)]
        target_port: u16,
        #[arg(long, default_value_t = 1)]
        node_id: u8,
        #[arg(long, default_value_t = 460800)]
        baud: u32,
    },
    /// Read the serial boot log to confirm the node joined WiFi.
    SerialCheck {
        #[arg(long)]
        port: String,
        #[arg(long, default_value_t = 30)]
        timeout: u64,
    },
    /// Full flow: flash → provision → serial-check → watch the roster.
    Onboard {
        #[arg(long)]
        port: String,
        #[arg(long, default_value = "esp32s3")]
        chip: String,
        #[arg(long)]
        ssid: String,
        #[arg(long)]
        password: Option<String>,
        #[arg(long)]
        target_ip: Option<String>,
        #[arg(long, default_value_t = 1)]
        node_id: u8,
        /// Skip flashing (node already has firmware).
        #[arg(long)]
        skip_flash: bool,
    },
}

#[derive(Subcommand, Debug)]
enum CalibrateAction {
    /// Start calibration for N seconds (keep the room empty).
    Start {
        /// Duration in seconds.
        #[arg(long, default_value_t = 30)]
        duration: u64,
    },
    /// Stop calibration early.
    Stop,
    /// Show calibration status.
    Status,
}

#[derive(Subcommand, Debug)]
enum ModelAction {
    /// List models loaded on the server.
    List,
    /// List the pretrained models bundled next to the CLI.
    Bundled,
    /// Show the active model's info.
    Info,
    /// Show the currently active model.
    Active,
    /// Load a model by id.
    Load {
        id: String,
    },
    /// Unload the active model.
    Unload,
    /// List SONA profiles.
    Sona,
    /// List LoRA profiles.
    Lora,
    /// Show progressive-loading layers.
    Layers,
    /// Show progressive-loading segments.
    Segments,
}

#[derive(Subcommand, Debug)]
enum PoseAction {
    /// Current WiFi-derived pose.
    Current,
    /// Pose statistics.
    Stats,
    /// Per-zone occupancy summary.
    Zones,
}

#[derive(Subcommand, Debug)]
enum RecordingAction {
    /// List recordings.
    List,
    /// Start recording.
    Start,
    /// Stop recording.
    Stop,
    /// Delete a recording by id.
    Rm { id: String },
}

#[derive(Subcommand, Debug)]
enum TrainAction {
    /// Start training.
    Start,
    /// Stop training.
    Stop,
    /// Training status.
    Status,
}

#[derive(Subcommand, Debug)]
enum AdaptiveAction {
    /// Train the adaptive classifier.
    Train,
    /// Unload the adaptive classifier.
    Unload,
    /// Adaptive classifier status.
    Status,
}

#[derive(Subcommand, Debug)]
enum RangingAction {
    /// Run FTM ranging (auto node positioning).
    Run,
    /// Apply the computed positions.
    Apply,
    /// Ranging status.
    Status,
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();
    let code = match run(&cli).await {
        Ok(()) => 0,
        Err(e) => {
            // clig.dev: human-readable error to stderr; important info at the end.
            eprintln!("error: {e:#}");
            e.downcast_ref::<CliError>().map(|c| c.code).unwrap_or(1)
        }
    };
    std::process::exit(code);
}

/// Carries an exit code alongside an error message.
#[derive(Debug)]
struct CliError {
    code: i32,
    msg: String,
}
impl std::fmt::Display for CliError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.msg)
    }
}
impl std::error::Error for CliError {}

async fn run(cli: &Cli) -> anyhow::Result<()> {
    match &cli.command {
        Command::Doctor => doctor(cli).await,
        Command::Server(g) => match &g.action {
            ServerAction::Status => server_status(cli).await,
            ServerAction::Start { port, source } => {
                let pid = serverproc::start(*port, source)?;
                output::note(&format!("sensing-server started (pid {pid}) on :{port} [source={source}]"));
                Ok(())
            }
            ServerAction::Stop => {
                serverproc::stop()?;
                output::note("sensing-server stopped");
                Ok(())
            }
            ServerAction::Restart { port, source } => {
                let _ = serverproc::stop();
                tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                let pid = serverproc::start(*port, source)?;
                output::note(&format!("sensing-server restarted (pid {pid}) on :{port}"));
                Ok(())
            }
            ServerAction::Logs { lines } => {
                println!("{}", serverproc::logs(*lines)?);
                Ok(())
            }
        },
        Command::Node(g) => match &g.action {
            NodeAction::List => node_list(cli, None).await,
            NodeAction::Get { id } => node_list(cli, Some(*id)).await,
            NodeAction::Watch { interval } => node_watch(cli, *interval).await,
            NodeAction::Positions => get_show(cli, "/api/v1/config/node-positions").await,
        },
        Command::Sensing(g) => match g.action {
            SensingAction::Latest => sensing_latest(cli).await,
        },
        Command::Serial(g) => match g.action {
            SerialAction::List => serial_list_cmd(cli),
        },
        Command::Wifi(g) => match g.action {
            WifiAction::Scan => wifi_scan_cmd(cli),
        },
        Command::Esp32(g) => match &g.action {
            Esp32Action::Flash { port, chip, tag, baud } => {
                esp32::flash(port, chip, tag, *baud).await?;
                output::note("flashed. next: wave-cli esp32 provision --port ... --ssid ...");
                Ok(())
            }
            Esp32Action::Provision {
                port, chip, ssid, password, target_ip, target_port, node_id, baud,
            } => {
                let pw = resolve_password(password.clone(), ssid)?;
                let ip = resolve_target_ip(target_ip.clone())?;
                esp32::provision(port, chip, ssid, &pw, &ip, *target_port, *node_id, *baud)?;
                output::note(&format!("provisioned node {node_id} → '{ssid}', streaming to {ip}:{target_port}"));
                Ok(())
            }
            Esp32Action::SerialCheck { port, timeout } => {
                let (joined, ip, log) = esp32::serial_check(port, *timeout)?;
                let v = serde_json::json!({ "joined": joined, "ip": ip });
                println!("{}", output::render(cli.output, &v, || {
                    let mut t = Table::new(&["Field", "Value"]);
                    t.row(vec!["joined".into(), joined.to_string()]);
                    t.row(vec!["ip".into(), ip.clone().unwrap_or_default()]);
                    t
                })?);
                if !joined {
                    eprintln!("{}", log.trim());
                    return Err(CliError { code: exit::USAGE, msg: format!("node did not join '{port}' within {timeout}s") }.into());
                }
                Ok(())
            }
            Esp32Action::Onboard { port, chip, ssid, password, target_ip, node_id, skip_flash } => {
                esp32_onboard(cli, port, chip, ssid, password.clone(), target_ip.clone(), *node_id, *skip_flash).await
            }
        },
        Command::Ota(g) => match &g.action {
            OtaAction::Check { node } => {
                println!("{}", output::auto(cli.output, &node::ota_check(node).await?)?);
                Ok(())
            }
            OtaAction::Update { node, file, psk } => {
                let v = node::ota_update(node, file, psk.as_deref()).await?;
                output::note(&format!("OTA sent to {node}"));
                println!("{}", output::auto(cli.output, &v)?);
                Ok(())
            }
            OtaAction::Batch { nodes, file, psk } => {
                let mut results = Vec::new();
                for n in nodes {
                    let r = node::ota_update(n, file, psk.as_deref()).await;
                    output::note(&format!("{n}: {}", if r.is_ok() { "ok" } else { "FAILED" }));
                    results.push(serde_json::json!({ "node": n, "ok": r.is_ok(),
                        "error": r.as_ref().err().map(|e| e.to_string()) }));
                }
                println!("{}", output::auto(cli.output, &serde_json::json!({ "results": results }))?);
                Ok(())
            }
        },
        Command::Wasm(g) => match &g.action {
            WasmAction::List { node } => {
                println!("{}", output::auto(cli.output, &node::wasm_list(node).await?)?);
                Ok(())
            }
            WasmAction::Upload { node, file, name, auto_start } => {
                let v = node::wasm_upload(node, file, name, *auto_start).await?;
                output::note(&format!("uploaded {name} to {node}"));
                println!("{}", output::auto(cli.output, &v)?);
                Ok(())
            }
            WasmAction::Start { node, id } => {
                node::wasm_control(node, id, "start").await?;
                output::note(&format!("started {id} on {node}"));
                Ok(())
            }
            WasmAction::Stop { node, id } => {
                node::wasm_control(node, id, "stop").await?;
                output::note(&format!("stopped {id} on {node}"));
                Ok(())
            }
        },
        Command::Pi(g) => match &g.action {
            PiAction::Probe { host } => {
                println!("{}", output::auto(cli.output, &pi::probe(host)?)?);
                Ok(())
            }
            PiAction::Service { host, action, name } => {
                println!("{}", output::auto(cli.output, &pi::service(host, name, action)?)?);
                Ok(())
            }
            PiAction::Health { host, service } => {
                println!("{}", output::auto(cli.output, &pi::health(host, service)?)?);
                Ok(())
            }
        },
        Command::Monitor { interval_ms } => monitor(cli, *interval_ms).await,
        Command::Coverage => {
            print_coverage(cli);
            Ok(())
        }
        Command::Calibrate(g) => match g.action {
            CalibrateAction::Start { duration } => calibrate_start(cli, duration).await,
            CalibrateAction::Stop => calibrate_stop(cli).await,
            CalibrateAction::Status => calibrate_status(cli).await,
        },
        Command::Model(g) => match &g.action {
            ModelAction::List => model_list(cli).await,
            ModelAction::Bundled => model_bundled(cli).await,
            ModelAction::Info => model_info(cli).await,
            ModelAction::Active => get_show(cli, "/api/v1/models/active").await,
            ModelAction::Load { id } => {
                let c = client(cli)?;
                let body = serde_json::json!({ "id": id });
                let v: serde_json::Value = c.post_json("/api/v1/models/load", Some(&body)).await.map_err(net)?;
                if !cli.quiet { output::note(&format!("load requested: {id}")); }
                if !v.is_null() { println!("{}", output::auto(cli.output, &v)?); }
                Ok(())
            }
            ModelAction::Unload => post_show(cli, "/api/v1/models/unload", "unload requested").await,
            ModelAction::Sona => get_show(cli, "/api/v1/model/sona/profiles").await,
            ModelAction::Lora => get_show(cli, "/api/v1/models/lora/profiles").await,
            ModelAction::Layers => get_show(cli, "/api/v1/model/layers").await,
            ModelAction::Segments => get_show(cli, "/api/v1/model/segments").await,
        },
        Command::Pose(g) => match g.action {
            PoseAction::Current => get_show(cli, "/api/v1/pose/current").await,
            PoseAction::Stats => get_show(cli, "/api/v1/pose/stats").await,
            PoseAction::Zones => get_show(cli, "/api/v1/pose/zones/summary").await,
        },
        Command::Vitals => get_show(cli, "/api/v1/vital-signs").await,
        Command::Recording(g) => match &g.action {
            RecordingAction::List => get_show(cli, "/api/v1/recording/list").await,
            RecordingAction::Start => post_show(cli, "/api/v1/recording/start", "recording started").await,
            RecordingAction::Stop => post_show(cli, "/api/v1/recording/stop", "recording stopped").await,
            RecordingAction::Rm { id } => {
                delete_show(cli, &format!("/api/v1/recording/{id}"), &format!("deleted recording {id}")).await
            }
        },
        Command::Train(g) => match g.action {
            TrainAction::Start => post_show(cli, "/api/v1/train/start", "training started").await,
            TrainAction::Stop => post_show(cli, "/api/v1/train/stop", "training stopped").await,
            TrainAction::Status => get_show(cli, "/api/v1/train/status").await,
        },
        Command::Adaptive(g) => match g.action {
            AdaptiveAction::Train => post_show(cli, "/api/v1/adaptive/train", "adaptive training started").await,
            AdaptiveAction::Unload => post_show(cli, "/api/v1/adaptive/unload", "adaptive classifier unloaded").await,
            AdaptiveAction::Status => get_show(cli, "/api/v1/adaptive/status").await,
        },
        Command::Ranging(g) => match g.action {
            RangingAction::Run => post_show(cli, "/api/v1/ranging/run", "FTM ranging started").await,
            RangingAction::Apply => post_show(cli, "/api/v1/ranging/apply", "positions applied").await,
            RangingAction::Status => get_show(cli, "/api/v1/ranging/status").await,
        },
        Command::User(g) => match &g.action {
            UserAction::List => {
                println!("{}", output::auto(cli.output, &admin::user_list(None).await?)?);
                Ok(())
            }
            UserAction::Get { email } => {
                println!("{}", output::auto(cli.output, &admin::user_list(Some(email)).await?)?);
                Ok(())
            }
            UserAction::Create { email, first, last, password, tenant } => {
                let pw = password
                    .clone()
                    .or_else(|| std::env::var("WAVE_NEW_PASSWORD").ok())
                    .filter(|p| !p.is_empty())
                    .ok_or_else(|| CliError { code: exit::USAGE, msg: "provide --password or set WAVE_NEW_PASSWORD".into() })?;
                admin::user_create(email, first, last, &pw, tenant.as_deref()).await?;
                output::note(&format!("created user {email}"));
                Ok(())
            }
            UserAction::Delete { email, yes } => {
                if !yes && std::io::IsTerminal::is_terminal(&std::io::stdin()) {
                    eprint!("delete user {email}? [y/N] ");
                    use std::io::Write;
                    let _ = std::io::stderr().flush();
                    let mut line = String::new();
                    std::io::stdin().read_line(&mut line).ok();
                    if !line.trim().eq_ignore_ascii_case("y") {
                        output::note("aborted");
                        return Ok(());
                    }
                }
                let n = admin::user_delete(email).await?;
                output::note(&format!("deleted {n} user(s)"));
                Ok(())
            }
        },
        Command::Role(g) => match g.action {
            RoleAction::List => {
                println!("{}", output::auto(cli.output, &admin::role_list().await?)?);
                Ok(())
            }
        },
        Command::Tenant(g) => match g.action {
            TenantAction::List => {
                println!("{}", output::auto(cli.output, &admin::tenant_list().await?)?);
                Ok(())
            }
        },
        Command::License => {
            println!("{}", output::auto(cli.output, &admin::license_status().await?)?);
            Ok(())
        }
        Command::Plan => {
            println!("{}", output::auto(cli.output, &admin::plan().await?)?);
            Ok(())
        }
        Command::Path(g) => pathcmd::run(&g.action, cli.quiet),
    }
}

/// Map any error to a network-category CLI error (shared by REST handlers).
fn net(e: anyhow::Error) -> anyhow::Error {
    CliError { code: exit::NETWORK, msg: e.to_string() }.into()
}

/// GET a path and auto-render it (table/json/yaml). Used by the thin REST verbs.
async fn get_show(cli: &Cli, path: &str) -> anyhow::Result<()> {
    let c = client(cli)?;
    let v: serde_json::Value = c.get_json(path).await.map_err(net)?;
    println!("{}", output::auto(cli.output, &v)?);
    Ok(())
}

/// POST a path (no body), print a note, and show any JSON response.
async fn post_show(cli: &Cli, path: &str, note: &str) -> anyhow::Result<()> {
    let c = client(cli)?;
    let v: serde_json::Value = c.post_json::<(), _>(path, None).await.map_err(net)?;
    if !cli.quiet {
        output::note(note);
    }
    if !v.is_null() {
        println!("{}", output::auto(cli.output, &v)?);
    }
    Ok(())
}

/// DELETE a path and print a note.
async fn delete_show(cli: &Cli, path: &str, note: &str) -> anyhow::Result<()> {
    let c = client(cli)?;
    c.delete(path).await.map_err(net)?;
    if !cli.quiet {
        output::note(note);
    }
    Ok(())
}

fn client(cli: &Cli) -> anyhow::Result<Client> {
    Client::new(cli.url.as_deref())
}

async fn server_status(cli: &Cli) -> anyhow::Result<()> {
    let c = client(cli)?;
    let status: serde_json::Value = c.get_json("/api/v1/status").await.map_err(|e| CliError {
        code: exit::NETWORK,
        msg: e.to_string(),
    })?;
    let nodes: serde_json::Value = c.get_json("/api/v1/nodes").await.unwrap_or(serde_json::json!({"total": 0}));
    let out = output::render(cli.output, &status, || {
        let mut t = Table::new(&["Field", "Value"]);
        t.row(vec!["url".into(), c.base().to_string()]);
        t.row(vec!["status".into(), jstr(&status, "status")]);
        t.row(vec!["source".into(), jstr(&status, "source")]);
        t.row(vec!["nodes".into(), jstr(&nodes, "total")]);
        t
    })?;
    println!("{out}");
    Ok(())
}

async fn node_list(cli: &Cli, only: Option<u64>) -> anyhow::Result<()> {
    let c = client(cli)?;
    let resp: serde_json::Value = c.get_json("/api/v1/nodes").await.map_err(|e| CliError {
        code: exit::NETWORK,
        msg: e.to_string(),
    })?;
    let empty = vec![];
    let mut nodes: Vec<serde_json::Value> = resp
        .get("nodes")
        .and_then(|n| n.as_array())
        .unwrap_or(&empty)
        .clone();
    if let Some(id) = only {
        nodes.retain(|n| n.get("node_id").and_then(|v| v.as_u64()) == Some(id));
    }
    let out = output::render(cli.output, &nodes, || {
        let mut t = Table::new(&["ID", "STATUS", "RSSI", "LAST_SEEN_MS", "PERSONS", "ORIGIN"]);
        for n in &nodes {
            t.row(vec![
                jstr(n, "node_id"),
                jstr(n, "status"),
                jstr(n, "rssi_dbm"),
                jstr(n, "last_seen_ms"),
                jstr(n, "person_count"),
                jstr(n, "origin"),
            ]);
        }
        t
    })?;
    println!("{out}");
    if nodes.is_empty() && !cli.quiet {
        output::note("no nodes streaming yet — provision a node or check it's on the hub network");
    }
    Ok(())
}

async fn sensing_latest(cli: &Cli) -> anyhow::Result<()> {
    let c = client(cli)?;
    let s: serde_json::Value = c.get_json("/api/v1/sensing/latest").await.map_err(|e| CliError {
        code: exit::NETWORK,
        msg: e.to_string(),
    })?;
    let out = output::render(cli.output, &s, || {
        let mut t = Table::new(&["Field", "Value"]);
        let presence = s
            .get("classification")
            .and_then(|c| c.get("presence"))
            .map(|v| v.to_string())
            .unwrap_or_else(|| "-".into());
        let persons = s
            .get("persons")
            .and_then(|p| p.as_array())
            .map(|a| a.len().to_string())
            .unwrap_or_else(|| jstr(&s, "estimated_persons"));
        t.row(vec!["presence".into(), presence]);
        t.row(vec!["persons".into(), persons]);
        if let Some(v) = s.get("vital_signs") {
            t.row(vec!["breathing_bpm".into(), jstr(v, "breathing_rate_bpm")]);
            t.row(vec!["heart_bpm".into(), jstr(v, "heart_rate_bpm")]);
        }
        t
    })?;
    println!("{out}");
    Ok(())
}

async fn doctor(cli: &Cli) -> anyhow::Result<()> {
    let c = client(cli)?;
    let server_ok = c.healthy().await;
    let esptool = which_ok("esptool") || python_esptool();
    let checks = serde_json::json!({
        "server_url": c.base(),
        "server_reachable": server_ok,
        "esptool_available": esptool,
    });
    let out = output::render(cli.output, &checks, || {
        let mut t = Table::new(&["Check", "Result"]);
        t.row(vec!["server url".into(), c.base().to_string()]);
        t.row(vec!["server reachable".into(), yn(server_ok)]);
        t.row(vec!["esptool available".into(), yn(esptool)]);
        t
    })?;
    println!("{out}");
    if !server_ok {
        return Err(CliError {
            code: exit::NETWORK,
            msg: format!("sensing server not reachable at {}", c.base()),
        }
        .into());
    }
    let _ = exit::USAGE; // reserved for arg-validation paths
    Ok(())
}

async fn node_watch(cli: &Cli, interval: u64) -> anyhow::Result<()> {
    let c = client(cli)?;
    let json_mode = matches!(cli.output, Format::Json | Format::Yaml);
    loop {
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {
                if !cli.quiet { output::note("stopped."); }
                break;
            }
            res = c.get_json::<serde_json::Value>("/api/v1/nodes") => {
                let resp = res.map_err(net)?;
                let empty = vec![];
                let nodes = resp.get("nodes").and_then(|n| n.as_array()).unwrap_or(&empty);
                if json_mode {
                    // JSON-lines: one array per tick, script-friendly.
                    println!("{}", serde_json::to_string(nodes)?);
                } else {
                    print!("\x1b[2J\x1b[H"); // clear + home
                    let table = output::render(Format::Table, nodes, || {
                        let mut t = Table::new(&["ID", "STATUS", "RSSI", "LAST_SEEN_MS", "PERSONS", "ORIGIN"]);
                        for n in nodes {
                            t.row(vec![
                                jstr(n, "node_id"), jstr(n, "status"), jstr(n, "rssi_dbm"),
                                jstr(n, "last_seen_ms"), jstr(n, "person_count"), jstr(n, "origin"),
                            ]);
                        }
                        t
                    })?;
                    println!(
                        "{}  ({} nodes, every {interval}s — Ctrl-C to stop)\n{table}",
                        chrono::Local::now().format("%H:%M:%S"),
                        nodes.len()
                    );
                }
                tokio::time::sleep(std::time::Duration::from_secs(interval.max(1))).await;
            }
        }
    }
    Ok(())
}

async fn calibrate_start(cli: &Cli, duration: u64) -> anyhow::Result<()> {
    let c = client(cli)?;
    let path = format!("/api/v1/calibration/start?duration_secs={duration}");
    let _: serde_json::Value = c.post_json::<(), _>(&path, None).await.map_err(net)?;
    if !cli.quiet {
        output::note(&format!("calibration started for {duration}s — keep the room empty"));
    }
    calibrate_status(cli).await
}

async fn calibrate_stop(cli: &Cli) -> anyhow::Result<()> {
    let c = client(cli)?;
    let _: serde_json::Value = c.post_json::<(), _>("/api/v1/calibration/stop", None).await.map_err(net)?;
    if !cli.quiet {
        output::note("calibration stopped");
    }
    Ok(())
}

async fn calibrate_status(cli: &Cli) -> anyhow::Result<()> {
    let c = client(cli)?;
    let s: serde_json::Value = c.get_json("/api/v1/calibration/status").await.map_err(net)?;
    let out = output::render(cli.output, &s, || {
        let mut t = Table::new(&["Field", "Value"]);
        t.row(vec!["running".into(), jstr(&s, "running")]);
        t.row(vec!["scheduled".into(), jstr(&s, "scheduled")]);
        t.row(vec!["seconds_remaining".into(), jstr(&s, "seconds_remaining")]);
        t.row(vec!["variance_explained".into(), jstr(&s, "variance_explained")]);
        t
    })?;
    println!("{out}");
    Ok(())
}

async fn model_list(cli: &Cli) -> anyhow::Result<()> {
    let c = client(cli)?;
    let v: serde_json::Value = c.get_json("/api/v1/models").await.map_err(net)?;
    let out = output::render(cli.output, &v, || {
        // Accept either {models:[...]} or a bare array.
        let arr = v
            .get("models")
            .and_then(|m| m.as_array())
            .cloned()
            .or_else(|| v.as_array().cloned())
            .unwrap_or_default();
        let mut t = Table::new(&["NAME/ID", "STATUS", "ACTIVE"]);
        for m in &arr {
            let name = if m.get("name").is_some() { jstr(m, "name") } else { jstr(m, "id") };
            t.row(vec![name, jstr(m, "status"), jstr(m, "active")]);
        }
        t
    })?;
    println!("{out}");
    Ok(())
}

async fn model_info(cli: &Cli) -> anyhow::Result<()> {
    let c = client(cli)?;
    let v: serde_json::Value = c.get_json("/api/v1/model/info").await.map_err(net)?;
    let out = output::render(cli.output, &v, || {
        let mut t = Table::new(&["Field", "Value"]);
        if let Some(obj) = v.as_object() {
            for (k, val) in obj {
                let s = match val {
                    serde_json::Value::String(s) => s.clone(),
                    other => other.to_string(),
                };
                t.row(vec![k.clone(), s]);
            }
        }
        t
    })?;
    println!("{out}");
    Ok(())
}

async fn model_bundled(cli: &Cli) -> anyhow::Result<()> {
    let files: Vec<serde_json::Value> = bundled_models_dir()
        .and_then(|d| std::fs::read_dir(d).ok())
        .map(|rd| {
            rd.flatten()
                .filter(|e| e.path().is_file())
                .map(|e| {
                    serde_json::json!({
                        "name": e.file_name().to_string_lossy(),
                        "size_bytes": e.metadata().map(|m| m.len()).unwrap_or(0),
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    let out = output::render(cli.output, &files, || {
        let mut t = Table::new(&["MODEL FILE", "SIZE (KB)"]);
        for f in &files {
            let kb = f.get("size_bytes").and_then(|v| v.as_u64()).unwrap_or(0) as f64 / 1024.0;
            t.row(vec![jstr(f, "name"), format!("{kb:.1}")]);
        }
        t
    })?;
    println!("{out}");
    if files.is_empty() && !cli.quiet {
        output::note("no bundled models found next to wave-cli (expected resources/models/)");
    }
    Ok(())
}

/// Live terminal dashboard — the CLI equivalent of the 3D-pose tab: CSI stream
/// rate, DSP pipeline state, and ML inference speed, refreshed in place.
async fn monitor(cli: &Cli, interval_ms: u64) -> anyhow::Result<()> {
    let c = client(cli)?;
    let interval = std::time::Duration::from_millis(interval_ms.max(100));
    loop {
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {
                print!("\x1b[2J\x1b[H");
                if !cli.quiet { output::note("stopped."); }
                break;
            }
            _ = tokio::time::sleep(interval) => {
                let nodes_v: serde_json::Value = c.get_json("/api/v1/nodes").await.map_err(net)?;
                let latest: serde_json::Value = c.get_json("/api/v1/sensing/latest").await.unwrap_or(serde_json::Value::Null);
                let stream: serde_json::Value = c.get_json("/api/v1/stream/status").await.unwrap_or(serde_json::Value::Null);
                let status: serde_json::Value = c.get_json("/api/v1/status").await.unwrap_or(serde_json::Value::Null);
                let empty = vec![];
                let nodes = nodes_v.get("nodes").and_then(|n| n.as_array()).unwrap_or(&empty);

                // Per-node inference latency (best-effort).
                let mut infer_lat: std::collections::HashMap<u64, f64> = std::collections::HashMap::new();
                for n in nodes {
                    if let Some(id) = n.get("node_id").and_then(|v| v.as_u64()) {
                        if let Ok(inf) = c.get_json::<serde_json::Value>(&format!("/api/v1/nodes/{id}/inference")).await {
                            if let Some(us) = inf.get("latency_us").and_then(|v| v.as_f64()) {
                                infer_lat.insert(id, us);
                            }
                        }
                    }
                }

                print!("\x1b[2J\x1b[H");
                let now = chrono::Local::now().format("%H:%M:%S");
                println!(
                    "Wave Live Monitor   {now}   server: {}/{}   (every {interval_ms}ms · Ctrl-C to quit)\n",
                    jstr(&status, "status"), jstr(&status, "source")
                );

                println!("── CSI STREAM (per node) ──");
                let table = output::render(Format::Table, nodes, || {
                    let mut t = Table::new(&["NODE", "FPS", "RSSI", "HEALTH", "DSP MOTION", "PERSONS", "NEURAL", "SRC", "INFER µs"]);
                    for n in nodes {
                        let id = n.get("node_id").and_then(|v| v.as_u64()).unwrap_or(0);
                        t.row(vec![
                            id.to_string(),
                            fmt_num(n, "fps"),
                            jstr(n, "rssi_dbm"),
                            jstr(n, "health"),
                            jstr(n, "motion_level"),
                            jstr(n, "person_count"),
                            fmt_num(n, "neural_presence"),
                            jstr(n, "neural_source"),
                            infer_lat.get(&id).map(|u| format!("{u:.0}")).unwrap_or_else(|| "-".into()),
                        ]);
                    }
                    t
                })?;
                println!("{table}");

                println!("\n── DSP PIPELINE (sensing/latest) ──");
                let presence = latest.get("classification").and_then(|c| c.get("presence")).map(|v| v.to_string()).unwrap_or_else(|| "-".into());
                let persons = latest.get("persons").and_then(|p| p.as_array()).map(|a| a.len().to_string()).unwrap_or_else(|| jstr(&latest, "estimated_persons"));
                let vit = latest.get("vital_signs");
                let br = vit.and_then(|v| v.get("breathing_rate_bpm")).map(|v| v.to_string()).unwrap_or_else(|| "-".into());
                let hr = vit.and_then(|v| v.get("heart_rate_bpm")).map(|v| v.to_string()).unwrap_or_else(|| "-".into());
                let sq = latest.get("signal_quality_score").map(|v| v.to_string()).unwrap_or_else(|| "-".into());
                println!("  presence={presence}  persons={persons}  breathing={br}bpm  heart={hr}bpm  signal_q={sq}");
                if let Some(f) = latest.get("features") {
                    println!("  features: variance={} motion_band={} spectral={} change_pts={}",
                        jstr(f, "variance"), jstr(f, "motion_band_power"), jstr(f, "spectral_power"), jstr(f, "change_points"));
                }

                println!("\n── ML INFERENCE / STREAM RATE ──");
                let fps_vals: Vec<f64> = nodes.iter().filter_map(|n| n.get("fps").and_then(|x| x.as_f64())).collect();
                let avg_fps = if fps_vals.is_empty() { 0.0 } else { fps_vals.iter().sum::<f64>() / fps_vals.len() as f64 };
                let avg_lat = if infer_lat.is_empty() { 0.0 } else { infer_lat.values().sum::<f64>() / infer_lat.len() as f64 };
                let running = nodes.iter().filter(|n| n.get("neural_source").and_then(|v| v.as_str()).map(|s| !s.is_empty() && s != "none").unwrap_or(false)).count();
                let stream_fps = stream.get("fps").map(|v| v.to_string()).unwrap_or_else(|| jstr(&stream, "rate"));
                println!("  CSI stream: avg {avg_fps:.1} fps/node   server pose stream: {stream_fps} fps");
                println!("  ML inference: avg {avg_lat:.0} µs/frame   nodes running model: {running}/{}", nodes.len());
            }
        }
    }
    Ok(())
}

/// Format a numeric JSON field to 1 decimal (falls back to raw stringify).
fn fmt_num(v: &serde_json::Value, key: &str) -> String {
    match v.get(key).and_then(|x| x.as_f64()) {
        Some(n) => format!("{n:.1}"),
        None => jstr(v, key),
    }
}

/// Honest self-audit of backend coverage.
fn print_coverage(cli: &Cli) {
    // (area, cli command, status)
    let rows: &[(&str, &str, &str)] = &[
        ("server status/info/health", "server status, doctor", "done"),
        ("nodes roster", "node list/get/watch", "done"),
        ("node positions", "node positions", "done"),
        ("sensing latest / vitals", "sensing latest, vitals", "done"),
        ("pose current/stats/zones", "pose current/stats/zones", "done"),
        ("calibration", "calibrate start/stop/status", "done"),
        ("recording", "recording list/start/stop/rm", "done"),
        ("training", "train start/stop/status", "done"),
        ("adaptive classifier", "adaptive train/unload/status", "done"),
        ("FTM ranging", "ranging run/apply/status", "done"),
        ("model mgmt", "model list/active/load/unload/sona/lora/layers/segments/info/bundled", "done"),
        ("server lifecycle", "server start/stop/restart/logs", "done"),
        ("serial + wifi discovery", "serial list, wifi scan", "done"),
        ("ESP32 onboarding", "esp32 flash/provision/serial-check/onboard", "done*"),
        ("OTA (node HTTP)", "ota check/update/batch", "done*"),
        ("WASM (node HTTP)", "wasm list/upload/start/stop", "done*"),
        ("admin (wave.db)", "user/role/tenant/license/plan", "done"),
        ("PATH install", "path install/uninstall/status", "done"),
        ("Raspberry Pi (SSH)", "pi probe/service/health", "core*"),
        ("Pi Nexmon install", "(desktop wizard / nexmon_setup_auto.sh)", "desktop"),
        ("enterprise/cloud/insights", "deployment/cloud/insight/alerts/telemetry/frappe", "desktop"),
    ];
    let value: Vec<serde_json::Value> = rows
        .iter()
        .map(|(a, c, s)| serde_json::json!({ "area": a, "command": c, "status": s }))
        .collect();
    let out = output::render(cli.output, &value, || {
        let mut t = Table::new(&["AREA", "COMMAND", "STATUS"]);
        for (a, c, s) in rows {
            t.row(vec![a.to_string(), c.to_string(), s.to_string()]);
        }
        t
    })
    .unwrap_or_default();
    println!("{out}");
    if !cli.quiet {
        output::note("* code-complete; hardware acceptance owed (needs a node/board). TODO = not yet built.");
    }
}

/// Resolve the WiFi password: --password › $WAVE_WIFI_PASSWORD › saved profile.
fn resolve_password(explicit: Option<String>, ssid: &str) -> anyhow::Result<String> {
    if let Some(p) = explicit {
        return Ok(p);
    }
    if let Ok(p) = std::env::var("WAVE_WIFI_PASSWORD") {
        if !p.is_empty() {
            return Ok(p);
        }
    }
    if let Some(p) = hardware::wifi_saved_password(ssid) {
        eprintln!("using the saved WiFi password for '{ssid}'");
        return Ok(p);
    }
    anyhow::bail!(
        "no WiFi password — pass --password, set WAVE_WIFI_PASSWORD, or connect this PC to '{ssid}' first"
    )
}

/// Resolve the hub target IP: --target-ip › this PC's Wi-Fi LAN IPv4.
fn resolve_target_ip(explicit: Option<String>) -> anyhow::Result<String> {
    if let Some(ip) = explicit {
        return Ok(ip);
    }
    hardware::host_lan_ip()
        .ok_or_else(|| anyhow::anyhow!("could not detect this PC's LAN IP — pass --target-ip"))
}

/// End-to-end onboarding: (flash) → provision → serial-check → watch roster.
async fn esp32_onboard(
    cli: &Cli,
    port: &str,
    chip: &str,
    ssid: &str,
    password: Option<String>,
    target_ip: Option<String>,
    node_id: u8,
    skip_flash: bool,
) -> anyhow::Result<()> {
    let pw = resolve_password(password, ssid)?;
    let ip = resolve_target_ip(target_ip)?;
    if !skip_flash {
        output::note("[1/4] flashing firmware …");
        esp32::flash(port, chip, "v0.8.3-esp32", 460800).await?;
    } else {
        output::note("[1/4] skipping flash");
    }
    output::note("[2/4] provisioning NVS …");
    esp32::provision(port, chip, ssid, &pw, &ip, 5005, node_id, 460800)?;
    output::note("[3/4] verifying WiFi join over serial …");
    let (joined, node_ip, log) = esp32::serial_check(port, 40)?;
    if !joined {
        eprintln!("{}", log.trim());
        return Err(CliError {
            code: exit::USAGE,
            msg: format!("node did not join '{ssid}' — check the password / that it's 2.4GHz"),
        }
        .into());
    }
    output::note(&format!(
        "[3/4] joined WiFi{} — now watching the hub roster for node {node_id}",
        node_ip.map(|i| format!(" (IP {i})")).unwrap_or_default()
    ));
    output::note("[4/4] watching (Ctrl-C to stop) …");
    node_watch(cli, 2).await
}

fn serial_list_cmd(cli: &Cli) -> anyhow::Result<()> {
    let ports = hardware::serial_list()?;
    let out = output::render(cli.output, &ports, || {
        let mut t = Table::new(&["PORT", "ESP32?", "VID", "PID", "DESCRIPTION"]);
        for p in &ports {
            t.row(vec![
                p.name.clone(),
                if p.esp32_compatible { "yes".into() } else { "".into() },
                p.vid.clone().unwrap_or_default(),
                p.pid.clone().unwrap_or_default(),
                p.description.clone(),
            ]);
        }
        t
    })?;
    println!("{out}");
    if ports.is_empty() && !cli.quiet {
        output::note("no serial ports found — plug in the ESP32 with a data cable");
    }
    Ok(())
}

fn wifi_scan_cmd(cli: &Cli) -> anyhow::Result<()> {
    let nets = hardware::wifi_scan()?;
    let out = output::render(cli.output, &nets, || {
        let mut t = Table::new(&["SSID", "BAND", "CHANNEL"]);
        for n in &nets {
            t.row(vec![
                n.ssid.clone(),
                n.band.clone(),
                n.channel.map(|c| c.to_string()).unwrap_or_default(),
            ]);
        }
        t
    })?;
    println!("{out}");
    if !cli.quiet {
        output::note("ESP32 nodes need a 2.4GHz SSID");
    }
    Ok(())
}

/// Locate the bundled models directory relative to the CLI binary.
fn bundled_models_dir() -> Option<std::path::PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let dir = exe.parent()?;
    for cand in [dir.join("resources").join("models"), dir.join("models")] {
        if cand.is_dir() {
            return Some(cand);
        }
    }
    None
}

// ── small helpers ────────────────────────────────────────────────────────────

/// Stringify a JSON field for a table cell (drops surrounding quotes on strings).
fn jstr(v: &serde_json::Value, key: &str) -> String {
    match v.get(key) {
        Some(serde_json::Value::String(s)) => s.clone(),
        Some(other) => other.to_string(),
        None => "-".to_string(),
    }
}

fn yn(b: bool) -> String {
    if b { "ok".into() } else { "MISSING".into() }
}

fn which_ok(bin: &str) -> bool {
    let probe = if cfg!(windows) { "where" } else { "which" };
    std::process::Command::new(probe)
        .arg(bin)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

fn python_esptool() -> bool {
    std::process::Command::new("python")
        .args(["-m", "esptool", "version"])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}
