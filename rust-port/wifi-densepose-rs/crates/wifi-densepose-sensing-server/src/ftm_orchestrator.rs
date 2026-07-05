//! Server-side FTM ranging orchestration.
//!
//! Commands sensing nodes to range each other over the UDP control port
//! (`node_ip:5006`, ASCII `RUVIEW_FTM_RESPONDER|on/off` + `RUVIEW_RANGE|<mac>`),
//! collects the resulting `0xC5110008` range reports from the CSI UDP
//! listener, averages a pairwise distance matrix, solves the node
//! constellation with classical multidimensional scaling, and auto-populates
//! `node_positions` — but only while the positions frame is not `"manual"`
//! (a human's placement always wins; `POST /api/v1/ranging/apply?force=true`
//! overrides).
//!
//! Triggers: `POST /api/v1/ranging/run` (manual) and one automatic attempt
//! 60 s after startup when at least two rangeable (non-Pi) nodes are online.

use std::collections::HashMap;
use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;
use std::time::Duration;

use axum::extract::{Query, State};
use axum::response::Json;
use serde::{Deserialize, Serialize};
use tokio::net::UdpSocket;
use tokio::sync::{mpsc, Mutex};
use tracing::{debug, info, warn};

use crate::protocol::esp32_legacy::{RangeReport, RANGE_STATUS_UNSUPPORTED};
use crate::{SharedState, FRAME_FTM_AUTO, FRAME_MANUAL};

/// UDP control port on the nodes (same port the discovery beacon uses).
const CONTROL_PORT: u16 = 5006;
/// Settle time after enabling the FTM responder before ranging against it.
const RESPONDER_SETTLE: Duration = Duration::from_millis(1000);
/// How long to wait for a range report per attempt.
const REPORT_TIMEOUT: Duration = Duration::from_secs(5);
/// Retries per ordered pair after the first failed attempt.
const RANGE_RETRIES: u32 = 2;
/// Delay before the one-shot automatic ranging run after startup.
const AUTO_RUN_DELAY: Duration = Duration::from_secs(60);
/// How long to collect discovery beacons before each cycle.
const DISCOVERY_WINDOW: Duration = Duration::from_secs(2);
/// A node counts as online if it sent traffic within this window.
const ONLINE_WINDOW: Duration = Duration::from_secs(10);
/// Default node height (meters) assigned by the solver.
const DEFAULT_NODE_Z: f64 = 1.0;

// ── Shared status / channels ────────────────────────────────────────────────

/// One averaged pairwise distance (unordered pair `a < b`).
#[derive(Debug, Clone, Serialize)]
pub struct PairDistance {
    pub a: u8,
    pub b: u8,
    pub distance_m: f64,
    pub samples: u32,
    /// Sample standard deviation of the measured distances (0 for 1 sample).
    pub std_m: f64,
}

#[derive(Debug, Default)]
struct RangingStatus {
    running: bool,
    /// pairs_measured in the REST response is derived: distances.len().
    distances: Vec<PairDistance>,
    last_run: Option<String>,
    last_error: Option<String>,
    /// Positions from the most recent successful solve (room meters), kept
    /// even when not applied so `/ranging/apply?force=true` can override.
    solved: Option<HashMap<u8, [f64; 3]>>,
    /// Whether the last solve was written into node_positions.
    applied: bool,
}

/// Cheap-to-clone handle stored on the app state: lets the UDP receiver feed
/// range reports in and lets the REST handlers trigger runs / read status.
#[derive(Clone)]
pub struct RangingHandle {
    status: Arc<Mutex<RangingStatus>>,
    report_tx: mpsc::UnboundedSender<(RangeReport, SocketAddr)>,
    trigger_tx: mpsc::UnboundedSender<()>,
}

impl RangingHandle {
    /// Feed a decoded `0xC5110008` range report from the UDP receiver.
    pub fn ingest_report(&self, report: RangeReport, src: SocketAddr) {
        let _ = self.report_tx.send((report, src));
    }
}

/// Receiver ends owned by the orchestrator task.
pub struct OrchestratorChannels {
    status: Arc<Mutex<RangingStatus>>,
    report_rx: mpsc::UnboundedReceiver<(RangeReport, SocketAddr)>,
    trigger_rx: mpsc::UnboundedReceiver<()>,
}

/// Create the handle/channel pair wiring the app state, UDP receiver, REST
/// handlers, and orchestrator task together.
pub fn ranging_channel() -> (RangingHandle, OrchestratorChannels) {
    let status = Arc::new(Mutex::new(RangingStatus::default()));
    let (report_tx, report_rx) = mpsc::unbounded_channel();
    let (trigger_tx, trigger_rx) = mpsc::unbounded_channel();
    (
        RangingHandle { status: status.clone(), report_tx, trigger_tx },
        OrchestratorChannels { status, report_rx, trigger_rx },
    )
}

// ── MAC helpers ──────────────────────────────────────────────────────────────

pub fn format_mac(mac: &[u8; 6]) -> String {
    format!(
        "{:02X}:{:02X}:{:02X}:{:02X}:{:02X}:{:02X}",
        mac[0], mac[1], mac[2], mac[3], mac[4], mac[5]
    )
}

pub fn parse_mac(s: &str) -> Option<[u8; 6]> {
    let mut out = [0u8; 6];
    let mut n = 0;
    for part in s.trim().split(':') {
        if n >= 6 || part.len() != 2 {
            return None;
        }
        out[n] = u8::from_str_radix(part, 16).ok()?;
        n += 1;
    }
    (n == 6).then_some(out)
}

// ── REST handlers ────────────────────────────────────────────────────────────

/// POST /api/v1/ranging/run — trigger a manual ranging cycle.
pub async fn ranging_run(State(state): State<SharedState>) -> Json<serde_json::Value> {
    let handle = state.read().await.ranging.clone();
    {
        let st = handle.status.lock().await;
        if st.running {
            return Json(serde_json::json!({
                "started": false,
                "error": "a ranging cycle is already running"
            }));
        }
    }
    match handle.trigger_tx.send(()) {
        Ok(()) => Json(serde_json::json!({ "started": true })),
        Err(_) => Json(serde_json::json!({
            "started": false,
            "error": "ranging orchestrator is not running"
        })),
    }
}

/// GET /api/v1/ranging/status
pub async fn ranging_status(State(state): State<SharedState>) -> Json<serde_json::Value> {
    let (handle, frame) = {
        let s = state.read().await;
        (s.ranging.clone(), s.positions_frame.clone())
    };
    let st = handle.status.lock().await;
    Json(serde_json::json!({
        "running": st.running,
        "pairs_measured": st.distances.len(),
        "distances": st.distances,
        "last_run": st.last_run,
        "last_error": st.last_error,
        "solved": st.solved.as_ref().map(|m| {
            m.iter().map(|(id, p)| (id.to_string(), *p)).collect::<HashMap<_, _>>()
        }),
        "applied": st.applied,
        "frame": frame,
    }))
}

#[derive(Debug, Deserialize)]
pub struct ApplyParams {
    #[serde(default)]
    pub force: bool,
}

/// POST /api/v1/ranging/apply?force=true — write the last solved constellation
/// into node_positions. Without `force` the manual frame still wins.
pub async fn ranging_apply(
    State(state): State<SharedState>,
    Query(params): Query<ApplyParams>,
) -> Json<serde_json::Value> {
    let handle = state.read().await.ranging.clone();
    let solved = handle.status.lock().await.solved.clone();
    let Some(positions) = solved else {
        return Json(serde_json::json!({
            "success": false,
            "error": "no solved constellation available — run ranging first"
        }));
    };
    match apply_solved_positions(&state, &positions, params.force).await {
        Ok(true) => {
            handle.status.lock().await.applied = true;
            Json(serde_json::json!({ "success": true, "count": positions.len() }))
        }
        Ok(false) => Json(serde_json::json!({
            "success": false,
            "error": "positions frame is 'manual' — pass ?force=true to override"
        })),
        Err(e) => Json(serde_json::json!({ "success": false, "error": e })),
    }
}

/// Write solved positions into the live state + persistence file with
/// frame `"ftm-auto"`. Returns Ok(false) when blocked by a manual frame.
async fn apply_solved_positions(
    state: &SharedState,
    positions: &HashMap<u8, [f64; 3]>,
    force: bool,
) -> Result<bool, String> {
    let snapshot = {
        let mut s = state.write().await;
        if !force && s.positions_frame == FRAME_MANUAL {
            return Ok(false);
        }
        // A solved constellation is a fresh, self-consistent frame — replace
        // rather than merge so stale positions from another frame don't mix.
        s.node_positions = positions.clone();
        s.positions_frame = FRAME_FTM_AUTO.to_string();
        s.node_positions.clone()
    };
    crate::save_node_positions_file(&snapshot, FRAME_FTM_AUTO).map_err(|e| e.to_string())?;
    info!(
        "FTM-solved node positions applied ({} nodes, frame={})",
        snapshot.len(),
        FRAME_FTM_AUTO
    );
    Ok(true)
}

// ── Orchestrator task ────────────────────────────────────────────────────────

/// Snapshot of one rangeable node taken from the live app state.
#[derive(Debug, Clone)]
struct NodeSnapshot {
    id: u8,
    ip: IpAddr,
    mac: Option<[u8; 6]>,
}

/// Online, non-Pi nodes with a known source IP (Pi/nexmon nodes cannot FTM).
async fn rangeable_nodes(state: &SharedState) -> Vec<NodeSnapshot> {
    let s = state.read().await;
    let now = std::time::Instant::now();
    let mut nodes: Vec<NodeSnapshot> = s
        .node_states
        .iter()
        .filter(|(_, ns)| {
            ns.origin != Some("nexmon")
                && ns
                    .last_frame_time
                    .is_some_and(|t| now.duration_since(t) < ONLINE_WINDOW)
        })
        .filter_map(|(&id, ns)| {
            ns.last_udp_addr.map(|addr| NodeSnapshot { id, ip: addr.ip(), mac: ns.mac })
        })
        .collect();
    nodes.sort_by_key(|n| n.id);
    nodes
}

/// Background task: waits for manual triggers and fires one automatic run
/// 60 s after startup (skipped when <2 rangeable nodes or a manual frame).
pub async fn orchestrator_task(state: SharedState, mut ch: OrchestratorChannels) {
    info!("FTM ranging orchestrator started (auto run in {}s)", AUTO_RUN_DELAY.as_secs());
    let auto = tokio::time::sleep(AUTO_RUN_DELAY);
    tokio::pin!(auto);
    let mut auto_pending = true;

    loop {
        tokio::select! {
            _ = &mut auto, if auto_pending => {
                auto_pending = false;
                let n = rangeable_nodes(&state).await.len();
                let frame = state.read().await.positions_frame.clone();
                if n < 2 {
                    debug!("Auto FTM ranging skipped: only {n} rangeable node(s) online");
                } else if frame == FRAME_MANUAL {
                    info!("Auto FTM ranging skipped: node positions are manually placed");
                } else {
                    info!("Auto FTM ranging: {n} rangeable nodes online");
                    run_cycle(&state, &mut ch).await;
                }
            }
            trig = ch.trigger_rx.recv() => {
                match trig {
                    Some(()) => run_cycle(&state, &mut ch).await,
                    None => break, // all handles dropped — shutting down
                }
            }
        }
    }
}

/// Run one full ranging cycle, recording status/errors around the core logic.
async fn run_cycle(state: &SharedState, ch: &mut OrchestratorChannels) {
    {
        let mut st = ch.status.lock().await;
        if st.running {
            return;
        }
        st.running = true;
        st.last_error = None;
        st.distances.clear();
    }
    let result = run_cycle_inner(state, ch).await;
    let mut st = ch.status.lock().await;
    st.running = false;
    st.last_run = Some(chrono::Utc::now().to_rfc3339());
    if let Err(e) = result {
        warn!("FTM ranging cycle failed: {e}");
        st.last_error = Some(e);
    }
}

async fn run_cycle_inner(
    state: &SharedState,
    ch: &mut OrchestratorChannels,
) -> Result<(), String> {
    let socket = UdpSocket::bind("0.0.0.0:0")
        .await
        .map_err(|e| format!("control socket bind failed: {e}"))?;
    socket
        .set_broadcast(true)
        .map_err(|e| format!("control socket broadcast failed: {e}"))?;

    // 1. Discovery sweep: learn node_id <-> MAC (and IP) from beacons.
    let discovered = discover_macs(&socket).await;
    if !discovered.is_empty() {
        let mut s = state.write().await;
        for (id, (mac, _ip)) in &discovered {
            if let Some(ns) = s.node_states.get_mut(id) {
                ns.mac = Some(*mac);
            }
        }
    }

    // 2. Snapshot rangeable nodes, filling MACs/IPs from discovery.
    let mut nodes = rangeable_nodes(state).await;
    for n in &mut nodes {
        if let Some((mac, ip)) = discovered.get(&n.id) {
            n.mac = Some(*mac);
            n.ip = *ip;
        }
    }
    if nodes.len() < 2 {
        return Err(format!("need >=2 rangeable nodes, found {}", nodes.len()));
    }
    let with_mac = nodes.iter().filter(|n| n.mac.is_some()).count();
    if with_mac == 0 {
        return Err("no node MAC addresses known (no discovery beacons answered)".into());
    }

    // 3. Drain any stale reports queued from a previous run.
    while ch.report_rx.try_recv().is_ok() {}

    // 4. Range every ordered pair (initiator -> target with known MAC).
    let mut samples: HashMap<(u8, u8), Vec<f64>> = HashMap::new();
    let mut unsupported: Vec<u8> = Vec::new();
    for target in &nodes {
        let Some(target_mac) = target.mac else {
            debug!("Skipping target node {}: MAC unknown", target.id);
            continue;
        };
        send_control(&socket, target.ip, "RUVIEW_FTM_RESPONDER|on").await;
        tokio::time::sleep(RESPONDER_SETTLE).await;

        for initiator in &nodes {
            if initiator.id == target.id || unsupported.contains(&initiator.id) {
                continue;
            }
            match range_pair(&socket, ch, initiator, target.id, &target_mac).await {
                Ok(distance_m) => {
                    let key = pair_key(initiator.id, target.id);
                    samples.entry(key).or_default().push(distance_m);
                    // Live status while the (minutes-long) cycle runs.
                    ch.status.lock().await.distances = summarize_pairs(&samples);
                }
                Err(RangeError::Unsupported) => {
                    info!("Node {} reports FTM unsupported — skipping as initiator", initiator.id);
                    unsupported.push(initiator.id);
                }
                Err(RangeError::Failed(msg)) => {
                    debug!("Ranging {} -> {} failed: {msg}", initiator.id, target.id);
                }
            }
        }
        send_control(&socket, target.ip, "RUVIEW_FTM_RESPONDER|off").await;
    }

    let distances = summarize_pairs(&samples);
    ch.status.lock().await.distances = distances.clone();
    if distances.is_empty() {
        return Err("no successful range measurements".into());
    }

    // 5. Solve the constellation and auto-apply (manual frame wins).
    let mut ids: Vec<u8> = Vec::new();
    for d in &distances {
        if !ids.contains(&d.a) {
            ids.push(d.a);
        }
        if !ids.contains(&d.b) {
            ids.push(d.b);
        }
    }
    ids.sort_unstable();
    let matrix = build_distance_matrix(&ids, &distances)
        .ok_or_else(|| "distance graph is disconnected — cannot solve constellation".to_string())?;
    let coords = solve_constellation(&matrix)
        .ok_or_else(|| "constellation solver failed (degenerate distances)".to_string())?;
    let positions: HashMap<u8, [f64; 3]> =
        ids.iter().copied().zip(coords.into_iter()).collect();

    {
        let mut st = ch.status.lock().await;
        st.solved = Some(positions.clone());
        st.applied = false;
    }
    match apply_solved_positions(state, &positions, false).await {
        Ok(true) => {
            ch.status.lock().await.applied = true;
        }
        Ok(false) => {
            info!("FTM constellation solved but not applied: positions frame is 'manual'");
        }
        Err(e) => warn!("FTM constellation solved but persist failed: {e}"),
    }
    Ok(())
}

enum RangeError {
    /// Initiator firmware cannot FTM — do not retry this node.
    Unsupported,
    Failed(String),
}

/// One ordered-pair measurement with retries. Returns the distance in meters.
async fn range_pair(
    socket: &UdpSocket,
    ch: &mut OrchestratorChannels,
    initiator: &NodeSnapshot,
    target_id: u8,
    target_mac: &[u8; 6],
) -> Result<f64, RangeError> {
    let cmd = format!("RUVIEW_RANGE|{}", format_mac(target_mac));
    let mut last_err = String::from("no report received");
    for attempt in 0..=RANGE_RETRIES {
        if attempt > 0 {
            debug!(
                "Retrying range {} -> {} (attempt {}/{})",
                initiator.id, target_id, attempt + 1, RANGE_RETRIES + 1
            );
        }
        send_control(socket, initiator.ip, &cmd).await;
        match wait_for_report(ch, initiator.id, target_mac).await {
            Ok(report) if report.is_ok() => {
                debug!(
                    "Range {} -> {}: {:.2} m over {} frames",
                    initiator.id, target_id, report.distance_m(), report.num_frames
                );
                return Ok(report.distance_m());
            }
            Ok(report) if report.status == RANGE_STATUS_UNSUPPORTED => {
                return Err(RangeError::Unsupported);
            }
            Ok(report) => last_err = format!("status {}", report.status),
            Err(e) => last_err = e,
        }
    }
    Err(RangeError::Failed(last_err))
}

/// Wait up to REPORT_TIMEOUT for a report from `initiator_id` about
/// `target_mac`; unrelated reports are ignored (stale/late traffic).
async fn wait_for_report(
    ch: &mut OrchestratorChannels,
    initiator_id: u8,
    target_mac: &[u8; 6],
) -> Result<RangeReport, String> {
    let deadline = tokio::time::Instant::now() + REPORT_TIMEOUT;
    loop {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        if remaining.is_zero() {
            return Err("timeout waiting for range report".into());
        }
        match tokio::time::timeout(remaining, ch.report_rx.recv()).await {
            Ok(Some((report, _src))) => {
                if report.node_id == initiator_id && &report.peer_mac == target_mac {
                    return Ok(report);
                }
                debug!(
                    "Ignoring unrelated range report: node={} peer={}",
                    report.node_id,
                    format_mac(&report.peer_mac)
                );
            }
            Ok(None) => return Err("report channel closed".into()),
            Err(_) => return Err("timeout waiting for range report".into()),
        }
    }
}

async fn send_control(socket: &UdpSocket, ip: IpAddr, msg: &str) {
    let addr = SocketAddr::new(ip, CONTROL_PORT);
    if let Err(e) = socket.send_to(msg.as_bytes(), addr).await {
        warn!("Control send to {addr} failed ({msg:?}): {e}");
    }
}

/// Broadcast RUVIEW_DISCOVER and collect RUVIEW_BEACON|mac|node_id|... replies
/// for DISCOVERY_WINDOW, yielding node_id -> (mac, ip).
async fn discover_macs(socket: &UdpSocket) -> HashMap<u8, ([u8; 6], IpAddr)> {
    let mut found = HashMap::new();
    let broadcast: SocketAddr = SocketAddr::from(([255, 255, 255, 255], CONTROL_PORT));
    if let Err(e) = socket.send_to(b"RUVIEW_DISCOVER", broadcast).await {
        warn!("Discovery broadcast failed: {e}");
        return found;
    }
    let deadline = tokio::time::Instant::now() + DISCOVERY_WINDOW;
    let mut buf = [0u8; 256];
    loop {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        if remaining.is_zero() {
            break;
        }
        match tokio::time::timeout(remaining, socket.recv_from(&mut buf)).await {
            Ok(Ok((len, src))) => {
                if let Some((id, mac)) = parse_beacon(&buf[..len]) {
                    debug!("Discovery beacon: node {id} mac {} at {}", format_mac(&mac), src.ip());
                    found.insert(id, (mac, src.ip()));
                }
            }
            Ok(Err(e)) => {
                warn!("Discovery recv failed: {e}");
                break;
            }
            Err(_) => break,
        }
    }
    found
}

/// Parse "RUVIEW_BEACON|<mac>|<node_id>|..." into (node_id, mac).
fn parse_beacon(data: &[u8]) -> Option<(u8, [u8; 6])> {
    let text = std::str::from_utf8(data).ok()?;
    let mut parts = text.split('|');
    if parts.next()? != "RUVIEW_BEACON" {
        return None;
    }
    let mac = parse_mac(parts.next()?)?;
    let id: u8 = parts.next()?.trim().parse().ok()?;
    Some((id, mac))
}

fn pair_key(a: u8, b: u8) -> (u8, u8) {
    if a < b { (a, b) } else { (b, a) }
}

/// Collapse per-pair samples into averaged distances with error estimates.
fn summarize_pairs(samples: &HashMap<(u8, u8), Vec<f64>>) -> Vec<PairDistance> {
    let mut out: Vec<PairDistance> = samples
        .iter()
        .filter(|(_, v)| !v.is_empty())
        .map(|(&(a, b), v)| {
            let n = v.len() as f64;
            let mean = v.iter().sum::<f64>() / n;
            let std_m = if v.len() > 1 {
                (v.iter().map(|x| (x - mean) * (x - mean)).sum::<f64>() / (n - 1.0)).sqrt()
            } else {
                0.0
            };
            PairDistance { a, b, distance_m: mean, samples: v.len() as u32, std_m }
        })
        .collect();
    out.sort_by_key(|p| (p.a, p.b));
    out
}

/// Build a complete symmetric distance matrix over `ids`, filling unmeasured
/// pairs by shortest path (Floyd-Warshall). None when the graph is
/// disconnected (some pair has no path at all).
fn build_distance_matrix(ids: &[u8], distances: &[PairDistance]) -> Option<Vec<Vec<f64>>> {
    let n = ids.len();
    let idx: HashMap<u8, usize> = ids.iter().enumerate().map(|(i, &id)| (id, i)).collect();
    let mut d = vec![vec![f64::INFINITY; n]; n];
    for i in 0..n {
        d[i][i] = 0.0;
    }
    for p in distances {
        let (i, j) = (*idx.get(&p.a)?, *idx.get(&p.b)?);
        d[i][j] = p.distance_m;
        d[j][i] = p.distance_m;
    }
    for k in 0..n {
        for i in 0..n {
            for j in 0..n {
                let via = d[i][k] + d[k][j];
                if via < d[i][j] {
                    d[i][j] = via;
                    d[j][i] = via;
                }
            }
        }
    }
    if d.iter().flatten().any(|v| !v.is_finite()) {
        return None;
    }
    Some(d)
}

// ── Constellation solver (classical MDS) ────────────────────────────────────

/// Solve node coordinates from a complete pairwise distance matrix.
///
/// N=2: nodes at [0,0,z] and [d,0,z]. N>=3: classical multidimensional
/// scaling in 2D — double-center the squared-distance matrix
/// (B = -1/2 J D^2 J) and take the top-2 eigenpairs via shifted power
/// iteration with deflation (hand-rolled; no linear-algebra dependency).
/// The solution is unique up to rotation/reflection/translation, which is
/// fine: only the shape (pairwise distances) matters. z defaults to 1.0 m.
pub fn solve_constellation(dist: &[Vec<f64>]) -> Option<Vec<[f64; 3]>> {
    let n = dist.len();
    if n < 2 || dist.iter().any(|row| row.len() != n) {
        return None;
    }
    if dist.iter().flatten().any(|v| !v.is_finite() || *v < 0.0) {
        return None;
    }
    if n == 2 {
        return Some(vec![
            [0.0, 0.0, DEFAULT_NODE_Z],
            [dist[0][1], 0.0, DEFAULT_NODE_Z],
        ]);
    }

    // B = -1/2 * J * D^2 * J with J = I - 1/n (double centering).
    let sq = |i: usize, j: usize| dist[i][j] * dist[i][j];
    let row_mean: Vec<f64> = (0..n)
        .map(|i| (0..n).map(|j| sq(i, j)).sum::<f64>() / n as f64)
        .collect();
    let grand_mean = row_mean.iter().sum::<f64>() / n as f64;
    let mut b = vec![vec![0.0f64; n]; n];
    for i in 0..n {
        for j in 0..n {
            b[i][j] = -0.5 * (sq(i, j) - row_mean[i] - row_mean[j] + grand_mean);
        }
    }

    let (l1, v1) = dominant_eigenpair(&b)?;
    // Deflate: B' = B - l1 * v1 v1^T.
    let mut b2 = b;
    for i in 0..n {
        for j in 0..n {
            b2[i][j] -= l1 * v1[i] * v1[j];
        }
    }
    let (l2, v2) = dominant_eigenpair(&b2)?;

    let s1 = l1.max(0.0).sqrt();
    let s2 = l2.max(0.0).sqrt();
    Some((0..n).map(|i| [s1 * v1[i], s2 * v2[i], DEFAULT_NODE_Z]).collect())
}

/// Largest-eigenvalue pair of a symmetric matrix via shifted power iteration.
///
/// A Gershgorin shift (A + sigma*I) makes the iteration converge to the
/// algebraically largest eigenvalue even when A is indefinite (the centered
/// Gram matrix can have small negative eigenvalues under measurement noise).
fn dominant_eigenpair(a: &[Vec<f64>]) -> Option<(f64, Vec<f64>)> {
    let n = a.len();
    let sigma: f64 = (0..n)
        .map(|i| (0..n).map(|j| a[i][j].abs()).sum::<f64>())
        .fold(0.0, f64::max);
    // Deterministic non-constant start vector (the all-ones vector is in the
    // null space of a double-centered matrix, so avoid it).
    let mut v: Vec<f64> = (0..n).map(|i| ((i as f64) * 0.7 + 0.3).sin() + 0.01).collect();
    normalize(&mut v)?;
    let mut lambda_shifted = 0.0;
    for _ in 0..600 {
        // w = (A + sigma I) v
        let mut w: Vec<f64> = (0..n)
            .map(|i| (0..n).map(|j| a[i][j] * v[j]).sum::<f64>() + sigma * v[i])
            .collect();
        let norm = normalize(&mut w)?;
        let delta = (norm - lambda_shifted).abs();
        lambda_shifted = norm;
        v = w;
        if delta < 1e-14 * sigma.max(1.0) {
            break;
        }
    }
    Some((lambda_shifted - sigma, v))
}

/// Normalize in place; returns the pre-normalization L2 norm.
/// None when the vector collapsed to (numerical) zero.
fn normalize(v: &mut [f64]) -> Option<f64> {
    let norm = v.iter().map(|x| x * x).sum::<f64>().sqrt();
    if !norm.is_finite() || norm < 1e-30 {
        return None;
    }
    for x in v.iter_mut() {
        *x /= norm;
    }
    Some(norm)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dist_between(a: &[f64; 3], b: &[f64; 3]) -> f64 {
        let dx = a[0] - b[0];
        let dy = a[1] - b[1];
        let dz = a[2] - b[2];
        (dx * dx + dy * dy + dz * dz).sqrt()
    }

    fn matrix_from_points(pts: &[[f64; 2]]) -> Vec<Vec<f64>> {
        let n = pts.len();
        let mut d = vec![vec![0.0; n]; n];
        for i in 0..n {
            for j in 0..n {
                let dx = pts[i][0] - pts[j][0];
                let dy = pts[i][1] - pts[j][1];
                d[i][j] = (dx * dx + dy * dy).sqrt();
            }
        }
        d
    }

    /// Assert the solution's pairwise distances match the input matrix — the
    /// solve is only defined up to rotation/reflection/translation.
    fn assert_pairwise(coords: &[[f64; 3]], d: &[Vec<f64>], tol: f64) {
        for i in 0..coords.len() {
            for j in (i + 1)..coords.len() {
                let got = dist_between(&coords[i], &coords[j]);
                assert!(
                    (got - d[i][j]).abs() <= tol,
                    "pair ({i},{j}): got {got}, want {} (tol {tol})",
                    d[i][j]
                );
            }
        }
    }

    #[test]
    fn two_nodes_form_a_line() {
        let d = vec![vec![0.0, 3.5], vec![3.5, 0.0]];
        let coords = solve_constellation(&d).expect("must solve");
        assert_eq!(coords[0], [0.0, 0.0, 1.0]);
        assert_eq!(coords[1], [3.5, 0.0, 1.0]);
    }

    #[test]
    fn perfect_triangle_recovers_shape() {
        // 3-4-5 right triangle.
        let d = matrix_from_points(&[[0.0, 0.0], [3.0, 0.0], [0.0, 4.0]]);
        let coords = solve_constellation(&d).expect("must solve");
        assert_pairwise(&coords, &d, 1e-6);
        // z defaults to 1.0 for every node.
        assert!(coords.iter().all(|c| (c[2] - 1.0).abs() < 1e-12));
    }

    #[test]
    fn collinear_three_nodes_recover_shape() {
        let d = matrix_from_points(&[[0.0, 0.0], [1.0, 0.0], [2.5, 0.0]]);
        let coords = solve_constellation(&d).expect("must solve");
        assert_pairwise(&coords, &d, 1e-6);
    }

    #[test]
    fn noisy_square_recovers_within_tolerance() {
        // 4m x 4m square with deterministic +/-5% multiplicative noise.
        let clean = matrix_from_points(&[[0.0, 0.0], [4.0, 0.0], [4.0, 4.0], [0.0, 4.0]]);
        let n = clean.len();
        let mut noisy = clean.clone();
        let mut k = 0u32;
        for i in 0..n {
            for j in (i + 1)..n {
                // Deterministic pseudo-noise in [-0.05, 0.05].
                let eps = 0.05 * ((k as f64 * 2.399_963).sin());
                k += 1;
                noisy[i][j] = clean[i][j] * (1.0 + eps);
                noisy[j][i] = noisy[i][j];
            }
        }
        let coords = solve_constellation(&noisy).expect("must solve");
        // Recovered pairwise distances stay close to the true square:
        // MDS averages the noise, so allow ~2x the injected 5% of the 4-5.7m
        // edges (about 0.6 m absolute).
        assert_pairwise(&coords, &clean, 0.6);
    }

    #[test]
    fn missing_pair_is_completed_by_shortest_path() {
        // Square with one diagonal unmeasured — Floyd-Warshall fills it and
        // the solve still succeeds.
        let ids = [1u8, 2, 3, 4];
        let mut pairs = Vec::new();
        let pts = [[0.0, 0.0], [4.0, 0.0], [4.0, 4.0], [0.0, 4.0]];
        for i in 0..4 {
            for j in (i + 1)..4 {
                if (i, j) == (1, 3) {
                    continue; // drop one diagonal
                }
                let dx: f64 = pts[i][0] - pts[j][0];
                let dy: f64 = pts[i][1] - pts[j][1];
                pairs.push(PairDistance {
                    a: ids[i],
                    b: ids[j],
                    distance_m: (dx * dx + dy * dy).sqrt(),
                    samples: 1,
                    std_m: 0.0,
                });
            }
        }
        let matrix = build_distance_matrix(&ids, &pairs).expect("graph is connected");
        assert!(matrix[1][3].is_finite());
        assert!(solve_constellation(&matrix).is_some());
    }

    #[test]
    fn disconnected_graph_is_rejected() {
        let ids = [1u8, 2, 3];
        let pairs = [PairDistance { a: 1, b: 2, distance_m: 2.0, samples: 1, std_m: 0.0 }];
        assert!(build_distance_matrix(&ids, &pairs).is_none());
    }

    #[test]
    fn mac_roundtrip() {
        let mac = [0xAA, 0x1B, 0x00, 0xDD, 0xEE, 0x0F];
        assert_eq!(format_mac(&mac), "AA:1B:00:DD:EE:0F");
        assert_eq!(parse_mac("AA:1B:00:DD:EE:0F"), Some(mac));
        assert_eq!(parse_mac("aa:1b:00:dd:ee:0f"), Some(mac));
        assert_eq!(parse_mac("AA:1B:00:DD:EE"), None);
        assert_eq!(parse_mac("AA:1B:00:DD:EE:0F:11"), None);
        assert_eq!(parse_mac("zz:1B:00:DD:EE:0F"), None);
    }

    #[test]
    fn beacon_parsing() {
        assert_eq!(
            parse_beacon(b"RUVIEW_BEACON|AA:BB:CC:DD:EE:FF|7|0.3.0|esp32s3|node|0|4"),
            Some((7, [0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0xFF]))
        );
        assert_eq!(parse_beacon(b"RUVIEW_BEACON|AA:BB:CC:DD:EE:FF"), None);
        assert_eq!(parse_beacon(b"OTHER|AA:BB:CC:DD:EE:FF|7"), None);
    }

    #[test]
    fn summarize_pairs_averages_and_estimates_error() {
        let mut samples = HashMap::new();
        samples.insert((1u8, 2u8), vec![4.0, 4.2, 3.8]);
        samples.insert((2u8, 3u8), vec![2.5]);
        let out = summarize_pairs(&samples);
        assert_eq!(out.len(), 2);
        let p12 = out.iter().find(|p| p.a == 1 && p.b == 2).unwrap();
        assert!((p12.distance_m - 4.0).abs() < 1e-12);
        assert_eq!(p12.samples, 3);
        assert!((p12.std_m - 0.2).abs() < 1e-9);
        let p23 = out.iter().find(|p| p.a == 2 && p.b == 3).unwrap();
        assert_eq!(p23.samples, 1);
        assert_eq!(p23.std_m, 0.0);
    }
}
