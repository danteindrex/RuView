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

mod client;
mod output;
mod pathcmd;

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
    /// Empty-room field-model calibration.
    Calibrate(GroupArgs<CalibrateAction>),
    /// Inspect models (server-loaded + bundled).
    Model(GroupArgs<ModelAction>),
    /// Add/remove wave-cli from the system PATH (run by the installer, or manually).
    Path(GroupArgs<pathcmd::PathAction>),
    /// Environment + connectivity self-check.
    Doctor,
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
}

#[derive(Subcommand, Debug)]
enum SensingAction {
    /// Show the latest sensing frame (presence, persons, vitals).
    Latest,
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
        Command::Server(g) => match g.action {
            ServerAction::Status => server_status(cli).await,
        },
        Command::Node(g) => match &g.action {
            NodeAction::List => node_list(cli, None).await,
            NodeAction::Get { id } => node_list(cli, Some(*id)).await,
            NodeAction::Watch { interval } => node_watch(cli, *interval).await,
        },
        Command::Sensing(g) => match g.action {
            SensingAction::Latest => sensing_latest(cli).await,
        },
        Command::Calibrate(g) => match g.action {
            CalibrateAction::Start { duration } => calibrate_start(cli, duration).await,
            CalibrateAction::Stop => calibrate_stop(cli).await,
            CalibrateAction::Status => calibrate_status(cli).await,
        },
        Command::Model(g) => match g.action {
            ModelAction::List => model_list(cli).await,
            ModelAction::Bundled => model_bundled(cli).await,
            ModelAction::Info => model_info(cli).await,
        },
        Command::Path(g) => pathcmd::run(&g.action, cli.quiet),
    }
}

/// Map any error to a network-category CLI error (shared by REST handlers).
fn net(e: anyhow::Error) -> anyhow::Error {
    CliError { code: exit::NETWORK, msg: e.to_string() }.into()
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
