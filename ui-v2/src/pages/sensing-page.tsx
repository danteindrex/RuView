import { useCallback, useEffect, useRef, useState } from "react";
import { useAuthStore } from "@/lib/auth-store";
import { Pause, Play, RefreshCw, Trash2 } from "lucide-react";
import { tauriApi } from "@/lib/tauri-api";
import { PageSection } from "@/components/layout/page-section";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Badge } from "@/components/ui/badge";
import { Switch } from "@/components/ui/switch";
import { JsonViewer } from "@/components/layout/json-viewer";
import type { AppSettings, ServerConfig, ServerLogsResponse, ServerStartResult, ServerStatusResponse } from "@/types";
import { useSensingStore, type WsSensingUpdate, type WsNodeInfo } from "@/lib/sensing-store";

interface SensingPageProps {
  status: ServerStatusResponse | null;
  onStatusRefresh: () => Promise<void>;
}

type LogLevel = "INFO" | "WARN" | "ERROR";

interface LogEntry {
  id: number;
  timestamp: string;
  level: LogLevel;
  source: string;
  message: string;
}

interface ActivityEntry {
  timestamp: string;
  node_id: number;
  subcarrier_count: number;
  rssi: number;
  activity: string;
  confidence: number;
}

const DEFAULT_CONFIG: ServerConfig = {
  http_port: 8080,
  ws_port: 8765,
  udp_port: 5005,
  nexmon_port: 5500,
  ui_path: "",
  tick_ms: 100,
  bind_address: "127.0.0.1",
  source: "auto",
  pi_diag: false,
  benchmark: false,
  load_rvf: "",
  save_rvf: "",
  model: "",
  progressive: false,
  export_rvf: "",
  train: false,
  dataset: "",
  dataset_type: "mmfi",
  epochs: 100,
  checkpoint_dir: "",
  pretrain: false,
  pretrain_epochs: 50,
  embed: false,
  build_index: "",
  node_positions: "",
  calibrate: false,
};

const MAX_LOG_ENTRIES = 200;
const MAX_ACTIVITY_ENTRIES = 8;
const WS_RECONNECT_DELAY_MS = 3000;

function formatTimestamp(date: Date) {
  const hh = String(date.getHours()).padStart(2, "0");
  const mm = String(date.getMinutes()).padStart(2, "0");
  const ss = String(date.getSeconds()).padStart(2, "0");
  const ms = String(date.getMilliseconds()).padStart(3, "0");
  return `${hh}:${mm}:${ss}.${ms}`;
}

function logLevelClass(level: LogLevel) {
  if (level === "ERROR") return "text-destructive";
  if (level === "WARN") return "text-amber-400";
  return "text-muted-foreground";
}

let nextLogId = 1;

function logsFromUpdate(update: WsSensingUpdate): LogEntry[] {
  const timestamp = formatTimestamp(new Date(update.timestamp * 1000));
  const entries: LogEntry[] = [];

  for (const node of update.nodes ?? []) {
    entries.push({
      id: nextLogId++,
      timestamp,
      level: "INFO",
      source: "csi_receiver",
      message: `Node ${node.node_id}: RSSI ${node.rssi_dbm.toFixed(1)} dBm, ${node.subcarrier_count} subcarriers`,
    });
  }

  if (update.classification) {
    entries.push({
      id: nextLogId++,
      timestamp,
      level: update.classification.confidence < 0.5 ? "WARN" : "INFO",
      source: "classifier",
      message: `Motion ${update.classification.motion_level}, presence=${update.classification.presence}, confidence=${(update.classification.confidence * 100).toFixed(0)}%`,
    });
  }

  if (update.vital_signs) {
    const vs = update.vital_signs;
    entries.push({
      id: nextLogId++,
      timestamp,
      level: (vs.breathing_confidence ?? 0) < 0.5 ? "WARN" : "INFO",
      source: "vital_signs",
      message: `Breathing ${vs.breathing_rate_bpm?.toFixed(1) ?? "--"} BPM, HR ${vs.heart_rate_bpm?.toFixed(0) ?? "--"} BPM`,
    });
  }

  if (update.quality_verdict && update.quality_verdict !== "Permit") {
    entries.push({
      id: nextLogId++,
      timestamp,
      level: update.quality_verdict === "Deny" ? "ERROR" : "WARN",
      source: "quality_gate",
      message: `Signal quality ${update.quality_verdict}, score=${(update.signal_quality_score ?? 0).toFixed(2)}`,
    });
  }

  // Log feature info (RSSI, spectral power, etc.)
  if (update.features) {
    entries.push({
      id: nextLogId++,
      timestamp,
      level: "INFO",
      source: "feature_extractor",
      message: `Mean RSSI: ${update.features.mean_rssi.toFixed(1)} dBm, Variance: ${update.features.variance.toFixed(2)}, Spectral power: ${update.features.spectral_power.toFixed(2)}`,
    });
  }

  // Log person detections (zone, keypoint count)
  if (update.persons && update.persons.length > 0) {
    for (const person of update.persons) {
      entries.push({
        id: nextLogId++,
        timestamp,
        level: "INFO",
        source: "person_detector",
        message: `Person #${person.id}: zone=${person.zone}, confidence=${(person.confidence * 100).toFixed(0)}%, keypoints=${person.keypoints.length}`,
      });
    }
  }

  // Log pose keypoints summary
  if (update.pose_keypoints && update.pose_keypoints.length > 0) {
    entries.push({
      id: nextLogId++,
      timestamp,
      level: "INFO",
      source: "pose_estimator",
      message: `Pose: ${update.pose_keypoints.length} keypoints, posture=${update.posture ?? "unknown"}`,
    });
  }

  // Log signal field grid size
  if (update.signal_field && update.signal_field.values.length > 0) {
    entries.push({
      id: nextLogId++,
      timestamp,
      level: "INFO",
      source: "signal_field",
      message: `Signal field: ${update.signal_field.grid_size.join("x")} grid, ${update.signal_field.values.length} cells`,
    });
  }

  // Log node features (per-node classification)
  if (update.node_features && update.node_features.length > 0) {
    for (const nf of update.node_features) {
      entries.push({
        id: nextLogId++,
        timestamp,
        level: nf.stale ? "WARN" : "INFO",
        source: "node_classifier",
        message: `Node ${nf.node_id}: motion=${nf.classification.motion_level}, presence=${nf.classification.presence}, RSSI=${nf.rssi_dbm.toFixed(1)} dBm${nf.stale ? " [STALE]" : ""}`,
      });
    }
  }

  return entries;
}

function activityFromUpdate(update: WsSensingUpdate): ActivityEntry | null {
  if (!update.classification) return null;
  const node = update.nodes?.[0];
  return {
    timestamp: new Date(update.timestamp * 1000).toISOString(),
    node_id: node?.node_id ?? 0,
    subcarrier_count: node?.subcarrier_count ?? 0,
    rssi: node?.rssi_dbm ?? 0,
    activity: update.posture ?? update.classification.motion_level,
    confidence: update.classification.confidence,
  };
}

function mapSettingsToServerConfig(settings: AppSettings): ServerConfig {
  return {
    http_port: settings.server_http_port,
    ws_port: settings.server_ws_port,
    udp_port: settings.server_udp_port,
    nexmon_port: settings.server_nexmon_port,
    ui_path: settings.ui_path,
    tick_ms: settings.server_tick_ms,
    bind_address: settings.bind_address,
    source: settings.server_source,
    pi_diag: settings.server_pi_diag,
    benchmark: settings.server_enable_benchmark,
    load_rvf: settings.server_load_rvf_path,
    save_rvf: settings.server_save_rvf_path,
    model: settings.server_model_path,
    progressive: settings.server_progressive,
    export_rvf: settings.server_export_rvf_path,
    train: settings.server_enable_train,
    dataset: settings.server_dataset_path,
    dataset_type: settings.server_dataset_type,
    epochs: settings.server_epochs,
    checkpoint_dir: settings.server_checkpoint_dir,
    pretrain: settings.server_enable_pretrain,
    pretrain_epochs: settings.server_pretrain_epochs,
    embed: settings.server_enable_embed,
    build_index: settings.server_build_index,
    node_positions: settings.server_node_positions,
    calibrate: settings.server_calibrate_on_boot,
  };
}

function toNumber(value: string, fallback?: number | null): number | null {
  const parsed = Number(value);
  if (!Number.isFinite(parsed)) {
    return fallback ?? null;
  }
  return parsed;
}

export function SensingPage({ status, onStatusRefresh }: SensingPageProps) {
  const { accessToken } = useAuthStore();
  const { latestUpdate, connected: wsConnected, edgeVitals, wasmEvents, featureStates } = useSensingStore();
  const [config, setConfig] = useState<ServerConfig>(DEFAULT_CONFIG);
  const [logs, setLogs] = useState<ServerLogsResponse | null>(null);
  const [streamLogs, setStreamLogs] = useState<LogEntry[]>([]);
  const [activities, setActivities] = useState<ActivityEntry[]>([]);
  const [paused, setPaused] = useState(false);
  const [result, setResult] = useState<ServerStartResult | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [settingsApplied, setSettingsApplied] = useState(false);

  useEffect(() => {
    if (paused || !latestUpdate) return;
    
    const nextLogs = logsFromUpdate(latestUpdate);
    if (nextLogs.length > 0) {
      setStreamLogs((prev) => [...prev, ...nextLogs].slice(-MAX_LOG_ENTRIES));
    }
    const activity = activityFromUpdate(latestUpdate);
    if (activity) {
      setActivities((prev) => [activity, ...prev].slice(0, MAX_ACTIVITY_ENTRIES));
    }
  }, [latestUpdate, paused]);

  useEffect(() => {
    void (async () => {
      try {
        if (!accessToken) return;
        const settings = await tauriApi.getSettings(accessToken);
        if (settings) {
          setConfig((prev) => ({ ...prev, ...mapSettingsToServerConfig(settings) }));
          setSettingsApplied(true);
        }
      } catch {
        setSettingsApplied(false);
      }
    })();
  }, []);

  async function withBusy(task: () => Promise<void>) {
    setLoading(true);
    setError(null);
    try {
      await task();
      await onStatusRefresh();
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setLoading(false);
    }
  }

  const clearStream = useCallback(() => {
    setStreamLogs([]);
    setActivities([]);
  }, []);

  return (
    <div className="space-y-6">
      <PageSection title="Server Runtime" description="Start, stop, and restart sensing service with explicit transport and model configuration.">
        <div className="mb-4 flex flex-wrap items-center gap-2 text-xs text-muted-foreground">
          <Badge variant={settingsApplied ? "success" : "outline"}>{settingsApplied ? "Settings Applied" : "Defaults Applied"}</Badge>
          <span>All backend startup arguments are available here.</span>
        </div>

        <div className="grid gap-4 md:grid-cols-3">
          <div className="space-y-2">
            <Label htmlFor="server-http">HTTP Port</Label>
            <Input
              id="server-http"
              value={String(config.http_port ?? "")}
              onChange={(e) => setConfig((prev) => ({ ...prev, http_port: toNumber(e.target.value, prev.http_port) }))}
            />
          </div>
          <div className="space-y-2">
            <Label htmlFor="server-ws">WS Port</Label>
            <Input
              id="server-ws"
              value={String(config.ws_port ?? "")}
              onChange={(e) => setConfig((prev) => ({ ...prev, ws_port: toNumber(e.target.value, prev.ws_port) }))}
            />
          </div>
          <div className="space-y-2">
            <Label htmlFor="server-udp">UDP Port</Label>
            <Input
              id="server-udp"
              value={String(config.udp_port ?? "")}
              onChange={(e) => setConfig((prev) => ({ ...prev, udp_port: toNumber(e.target.value, prev.udp_port) }))}
            />
          </div>
          <div className="space-y-2">
            <Label htmlFor="server-nexmon">Nexmon UDP Port</Label>
            <Input
              id="server-nexmon"
              value={String(config.nexmon_port ?? "")}
              onChange={(e) => setConfig((prev) => ({ ...prev, nexmon_port: toNumber(e.target.value, prev.nexmon_port) }))}
            />
          </div>
          <div className="space-y-2">
            <Label htmlFor="server-tick">Tick (ms)</Label>
            <Input
              id="server-tick"
              value={String(config.tick_ms ?? "")}
              onChange={(e) => setConfig((prev) => ({ ...prev, tick_ms: toNumber(e.target.value, prev.tick_ms) }))}
            />
          </div>
          <div className="space-y-2">
            <Label htmlFor="server-source">Data Source</Label>
            <Input id="server-source" value={config.source ?? ""} onChange={(e) => setConfig((prev) => ({ ...prev, source: e.target.value }))} placeholder="auto | wifi | esp32 | nexmon | simulate" />
          </div>
          <div className="space-y-2">
            <Label htmlFor="server-bind">Bind Address</Label>
            <Input id="server-bind" value={config.bind_address ?? ""} onChange={(e) => setConfig((prev) => ({ ...prev, bind_address: e.target.value }))} />
          </div>
          <div className="space-y-2">
            <Label htmlFor="server-ui-path">UI Path</Label>
            <Input id="server-ui-path" value={config.ui_path ?? ""} onChange={(e) => setConfig((prev) => ({ ...prev, ui_path: e.target.value }))} />
          </div>
          <div className="space-y-2">
            <Label htmlFor="server-binary-path">Server Binary Path (optional override)</Label>
            <Input id="server-binary-path" value={config.server_path ?? ""} onChange={(e) => setConfig((prev) => ({ ...prev, server_path: e.target.value }))} />
          </div>
          <div className="space-y-2">
            <Label htmlFor="server-model">Model Path</Label>
            <Input id="server-model" value={config.model ?? ""} onChange={(e) => setConfig((prev) => ({ ...prev, model: e.target.value }))} />
          </div>
          <div className="space-y-2">
            <Label htmlFor="server-load-rvf">Load RVF</Label>
            <Input id="server-load-rvf" value={config.load_rvf ?? ""} onChange={(e) => setConfig((prev) => ({ ...prev, load_rvf: e.target.value }))} />
          </div>
          <div className="space-y-2">
            <Label htmlFor="server-save-rvf">Save RVF</Label>
            <Input id="server-save-rvf" value={config.save_rvf ?? ""} onChange={(e) => setConfig((prev) => ({ ...prev, save_rvf: e.target.value }))} />
          </div>
          <div className="space-y-2">
            <Label htmlFor="server-export-rvf">Export RVF</Label>
            <Input id="server-export-rvf" value={config.export_rvf ?? ""} onChange={(e) => setConfig((prev) => ({ ...prev, export_rvf: e.target.value }))} />
          </div>
          <div className="space-y-2">
            <Label htmlFor="server-dataset">Dataset Path</Label>
            <Input id="server-dataset" value={config.dataset ?? ""} onChange={(e) => setConfig((prev) => ({ ...prev, dataset: e.target.value }))} />
          </div>
          <div className="space-y-2">
            <Label htmlFor="server-dataset-type">Dataset Type</Label>
            <Input id="server-dataset-type" value={config.dataset_type ?? ""} onChange={(e) => setConfig((prev) => ({ ...prev, dataset_type: e.target.value }))} placeholder="mmfi | wipose" />
          </div>
          <div className="space-y-2">
            <Label htmlFor="server-epochs">Epochs</Label>
            <Input
              id="server-epochs"
              value={String(config.epochs ?? "")}
              onChange={(e) => setConfig((prev) => ({ ...prev, epochs: toNumber(e.target.value, prev.epochs) }))}
            />
          </div>
          <div className="space-y-2">
            <Label htmlFor="server-pretrain-epochs">Pretrain Epochs</Label>
            <Input
              id="server-pretrain-epochs"
              value={String(config.pretrain_epochs ?? "")}
              onChange={(e) => setConfig((prev) => ({ ...prev, pretrain_epochs: toNumber(e.target.value, prev.pretrain_epochs) }))}
            />
          </div>
          <div className="space-y-2">
            <Label htmlFor="server-checkpoint-dir">Checkpoint Directory</Label>
            <Input id="server-checkpoint-dir" value={config.checkpoint_dir ?? ""} onChange={(e) => setConfig((prev) => ({ ...prev, checkpoint_dir: e.target.value }))} />
          </div>
          <div className="space-y-2">
            <Label htmlFor="server-build-index">Build Index Type</Label>
            <Input id="server-build-index" value={config.build_index ?? ""} onChange={(e) => setConfig((prev) => ({ ...prev, build_index: e.target.value }))} placeholder="env | activity | temporal | person" />
          </div>
          <div className="space-y-2 md:col-span-3">
            <Label htmlFor="server-node-positions">Node Positions</Label>
            <Input id="server-node-positions" value={config.node_positions ?? ""} onChange={(e) => setConfig((prev) => ({ ...prev, node_positions: e.target.value }))} placeholder="x,y,z;x,y,z;..." />
          </div>
          <div className="flex items-center gap-3 rounded-md border border-border/60 p-3">
            <Switch checked={Boolean(config.pi_diag)} onCheckedChange={(checked) => setConfig((prev) => ({ ...prev, pi_diag: checked }))} id="server-pi-diag" />
            <Label htmlFor="server-pi-diag">Pi diagnostics</Label>
          </div>
          <div className="flex items-center gap-3 rounded-md border border-border/60 p-3">
            <Switch checked={Boolean(config.progressive)} onCheckedChange={(checked) => setConfig((prev) => ({ ...prev, progressive: checked }))} id="server-progressive" />
            <Label htmlFor="server-progressive">Progressive loading</Label>
          </div>
          <div className="flex items-center gap-3 rounded-md border border-border/60 p-3">
            <Switch checked={Boolean(config.calibrate)} onCheckedChange={(checked) => setConfig((prev) => ({ ...prev, calibrate: checked }))} id="server-calibrate" />
            <Label htmlFor="server-calibrate">Calibrate on startup</Label>
          </div>
          <div className="flex items-center gap-3 rounded-md border border-border/60 p-3">
            <Switch checked={Boolean(config.benchmark)} onCheckedChange={(checked) => setConfig((prev) => ({ ...prev, benchmark: checked }))} id="server-benchmark" />
            <Label htmlFor="server-benchmark">Benchmark mode</Label>
          </div>
          <div className="flex items-center gap-3 rounded-md border border-border/60 p-3">
            <Switch checked={Boolean(config.train)} onCheckedChange={(checked) => setConfig((prev) => ({ ...prev, train: checked }))} id="server-train" />
            <Label htmlFor="server-train">Train mode</Label>
          </div>
          <div className="flex items-center gap-3 rounded-md border border-border/60 p-3">
            <Switch checked={Boolean(config.pretrain)} onCheckedChange={(checked) => setConfig((prev) => ({ ...prev, pretrain: checked }))} id="server-pretrain" />
            <Label htmlFor="server-pretrain">Pretrain mode</Label>
          </div>
          <div className="flex items-center gap-3 rounded-md border border-border/60 p-3">
            <Switch checked={Boolean(config.embed)} onCheckedChange={(checked) => setConfig((prev) => ({ ...prev, embed: checked }))} id="server-embed" />
            <Label htmlFor="server-embed">Embedding mode</Label>
          </div>
        </div>

        <div className="mt-4 flex flex-wrap gap-2">
          <Button disabled={loading || !accessToken} onClick={() => withBusy(async () => {
            if (accessToken) setResult(await tauriApi.startServer(accessToken, config));
          })}>
            Start Server
          </Button>
          <Button disabled={loading || !accessToken} variant="secondary" onClick={() => withBusy(async () => {
            if (accessToken) await tauriApi.stopServer(accessToken);
          })}>
            Stop Server
          </Button>
          <Button disabled={loading || !accessToken} variant="outline" onClick={() => withBusy(async () => {
            if (accessToken) setResult(await tauriApi.restartServer(accessToken, config));
          })}>
            Restart Server
          </Button>
          <Button disabled={loading} variant="ghost" onClick={() => withBusy(onStatusRefresh)}>
            <RefreshCw className="mr-2 h-4 w-4" />
            Refresh Status
          </Button>
        </div>
      </PageSection>

      <PageSection title="Runtime Status" description="Live status from command handler.">
        <div className="mb-3 flex flex-wrap gap-2">
          <Badge variant={status?.running ? "success" : "danger"}>{status?.running ? "Running" : "Stopped"}</Badge>
          <Badge variant={wsConnected ? "success" : status?.running ? "warning" : "outline"}>{wsConnected ? "Stream Live" : status?.running ? "Stream Connecting" : "Stream Offline"}</Badge>
          <Badge variant="outline">PID: {status?.pid ?? "N/A"}</Badge>
          <Badge variant="outline">CPU: {status?.cpu_percent?.toFixed(1) ?? "N/A"}%</Badge>
          <Badge variant="outline">RAM: {status?.memory_mb?.toFixed(1) ?? "N/A"} MB</Badge>
          <Badge variant="outline">Uptime: {status?.uptime_secs ?? 0}s</Badge>
          <Badge variant="outline">Source: {status?.source ?? "N/A"}</Badge>
          <Badge variant="outline">Nexmon: {status?.nexmon_port ?? "N/A"}</Badge>
        </div>
        <JsonViewer value={status ?? { running: false }} />
      </PageSection>

      <PageSection title="Live Sensing Stream" description="Real-time WebSocket activity, signal quality, and vital-sign-derived events.">
        <div className="mb-3 flex flex-wrap gap-2">
          <Button variant="secondary" size="sm" onClick={() => setPaused((current) => !current)}>
            {paused ? <Play className="mr-2 h-4 w-4" /> : <Pause className="mr-2 h-4 w-4" />}
            {paused ? "Resume" : "Pause"}
          </Button>
          <Button variant="outline" size="sm" onClick={clearStream}>
            <Trash2 className="mr-2 h-4 w-4" />
            Clear
          </Button>
        </div>
        <div className="grid gap-4 xl:grid-cols-[minmax(0,1.2fr)_minmax(320px,0.8fr)]">
          <div className="h-80 overflow-auto rounded-md border border-border/60 bg-background/70 p-3 font-mono text-xs">
            {streamLogs.length === 0 ? (
              <div className="flex h-full items-center justify-center text-muted-foreground">Waiting for sensing stream data.</div>
            ) : (
              streamLogs.map((entry) => (
                <div key={entry.id} className="whitespace-nowrap leading-6">
                  <span className="text-muted-foreground">{entry.timestamp}</span>{" "}
                  <span className={logLevelClass(entry.level)}>{entry.level.padEnd(5, " ")}</span>{" "}
                  <span className="text-primary">{entry.source}</span>{" "}
                  <span className={logLevelClass(entry.level)}>{entry.message}</span>
                </div>
              ))
            )}
          </div>
          <div className="space-y-2">
            {activities.length === 0 ? (
              <div className="rounded-md border border-border/60 bg-background/70 p-4 text-sm text-muted-foreground">No activity events received.</div>
            ) : (
              activities.map((activity, index) => (
                <div key={`${activity.timestamp}-${index}`} className="rounded-md border border-border/60 bg-background/70 p-3">
                  <div className="mb-2 flex flex-wrap items-center gap-2">
                    <Badge variant="outline">Node {activity.node_id}</Badge>
                    <Badge variant={activity.confidence >= 0.7 ? "success" : activity.confidence >= 0.5 ? "warning" : "danger"}>
                      {(activity.confidence * 100).toFixed(0)}%
                    </Badge>
                    <span className="text-sm font-medium capitalize">{activity.activity}</span>
                  </div>
                  <div className="h-2 overflow-hidden rounded-full bg-muted">
                    <div className="h-full bg-primary" style={{ width: `${Math.max(0, Math.min(100, activity.confidence * 100))}%` }} />
                  </div>
                  <div className="mt-2 flex justify-between text-xs text-muted-foreground">
                    <span>{formatTimestamp(new Date(activity.timestamp))}</span>
                    <span>{activity.subcarrier_count} subcarriers, RSSI {activity.rssi.toFixed(1)} dBm</span>
                  </div>
                </div>
              ))
            )}
          </div>
        </div>
      </PageSection>

      <PageSection title="Edge Events" description="Per-node feature state, edge vitals, and WASM runtime events from sensing nodes.">
        <div className="grid gap-4 xl:grid-cols-2">
          <div className="space-y-2">
            <p className="text-xs font-semibold uppercase tracking-wide text-muted-foreground">Node Feature State</p>
            {Object.keys(featureStates).length === 0 ? (
              <div className="rounded-md border border-border/60 bg-background/70 p-3 text-sm text-muted-foreground">No feature_state messages received.</div>
            ) : (
              Object.values(featureStates).map((fs) => (
                <div key={fs.node_id} className="rounded-md border border-border/60 bg-background/70 p-3">
                  <div className="mb-1 flex flex-wrap items-center gap-2">
                    <Badge variant="outline">Node {fs.node_id}</Badge>
                    <span className="text-[10px] font-mono text-muted-foreground">
                      {fs.received_at ? formatTimestamp(new Date(fs.received_at)) : ""}
                    </span>
                  </div>
                  <pre className="max-h-24 overflow-auto whitespace-pre-wrap break-all font-mono text-[11px] text-muted-foreground">
                    {JSON.stringify({ ...fs, type: undefined, received_at: undefined })}
                  </pre>
                </div>
              ))
            )}
            {edgeVitals ? (
              <div className="rounded-md border border-border/60 bg-background/70 p-3">
                <div className="mb-1 flex flex-wrap items-center gap-2">
                  <Badge variant="outline">Edge Vitals — Node {edgeVitals.node_id}</Badge>
                  <Badge variant={edgeVitals.fall_detected ? "danger" : edgeVitals.presence ? "success" : "outline"}>
                    {edgeVitals.fall_detected ? "FALL" : edgeVitals.presence ? "Presence" : "Idle"}
                  </Badge>
                </div>
                <p className="text-xs text-muted-foreground">
                  BR {edgeVitals.breathing_rate_bpm?.toFixed(1) ?? "--"} rpm, HR {(edgeVitals.heartrate_bpm ?? edgeVitals.heart_rate_bpm)?.toFixed(0) ?? "--"} bpm,
                  motion {edgeVitals.motion_energy?.toFixed(1) ?? edgeVitals.motion ?? "--"}, RSSI {edgeVitals.rssi ?? "--"} dBm
                </p>
              </div>
            ) : null}
          </div>
          <div className="space-y-2">
            <p className="text-xs font-semibold uppercase tracking-wide text-muted-foreground">WASM Events (last {wasmEvents.length})</p>
            <div className="max-h-64 overflow-auto rounded-md border border-border/60 bg-background/70 p-3 font-mono text-[11px]">
              {wasmEvents.length === 0 ? (
                <div className="text-sm text-muted-foreground">No wasm_event messages received.</div>
              ) : (
                [...wasmEvents].reverse().map((evt, index) => (
                  <div key={`${evt.received_at}-${index}`} className="leading-5">
                    <span className="text-muted-foreground">{evt.received_at ? formatTimestamp(new Date(evt.received_at)) : ""}</span>{" "}
                    <span className="text-primary">node {evt.node_id}</span>{" "}
                    <span className="text-muted-foreground">module {evt.module_id ?? "?"}</span>{" "}
                    <span className="break-all">{JSON.stringify(evt.events)}</span>
                  </div>
                ))
              )}
            </div>
          </div>
        </div>
      </PageSection>

      <PageSection title="Server Logs" description="Retrieve current command-level server logs.">
        <div className="mb-3">
          <Button
            disabled={loading || !accessToken}
            onClick={() =>
              withBusy(async () => {
                if (accessToken) {
                  const response = await tauriApi.serverLogs(accessToken, 200);
                  setLogs(response);
                }
              })
            }
          >
            Fetch Logs
          </Button>
        </div>
        <JsonViewer value={logs ?? { stdout: [], stderr: [] }} />
      </PageSection>

      {result ? (
        <PageSection title="Last Operation" description="Most recent start/restart payload returned by server command.">
          <JsonViewer value={result} />
        </PageSection>
      ) : null}

      {error ? <p className="rounded-md border border-destructive/40 bg-destructive/10 px-3 py-2 text-sm text-destructive">{error}</p> : null}
    </div>
  );
}
