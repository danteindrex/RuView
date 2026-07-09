/**
 * Raspberry Pi first-run setup (W3, step B' — Pi branch).
 *
 * Wraps the existing pi-node command sequence (probe → prereqs → deploy →
 * push_config → install_service → csi_health) with a live checklist, then
 * watches /api/v1/nodes for the Pi (origin nexmon). The aggregator is forced to
 * the hub's LAN IP:udp_port and never a loopback (a Pi with 127.0.0.1 would
 * stream to itself).
 */

import { useEffect, useMemo, useState } from "react";
import { CheckCircle2, Circle, Loader2, XCircle } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { tauriApi } from "@/lib/tauri-api";
import { isLoopbackHostPort } from "@/lib/utils";
import { StatusLine } from "../step-shell";
import { deriveHubLanIp, fetchServerNodes, nodeIdNumber, resolveServerBase } from "../server-api";
import { useNodeWatch } from "../use-node-watch";
import type { PiAgentConfig, PiNodeCommandResult, PiNodeTarget, ServerNode, ServerStatusResponse } from "@/types";

interface PiSetupProps {
  accessToken: string | null;
  serverStatus: ServerStatusResponse | null;
}

type StepStatus = "pending" | "running" | "pass" | "fail";

interface ChecklistItem {
  key: string;
  label: string;
  status: StepStatus;
}

const INITIAL_CHECKLIST: ChecklistItem[] = [
  { key: "probe", label: "Reach the Pi over SSH", status: "pending" },
  { key: "prereqs", label: "Check / install prerequisites", status: "pending" },
  { key: "config", label: "Push agent config (aggregator → hub)", status: "pending" },
  { key: "service", label: "Install + start systemd service", status: "pending" },
  { key: "csi", label: "Verify CSI capture health", status: "pending" },
];

/** Hub LAN IP:port for the aggregator, or empty if only loopback is available. */
function deriveAggregator(status: ServerStatusResponse | null): string {
  const ip = deriveHubLanIp(status);
  if (!ip) return "";
  return `${ip}:${status?.udp_port ?? 5005}`;
}

export function PiSetup({ accessToken, serverStatus }: PiSetupProps) {
  const [target, setTarget] = useState<PiNodeTarget>({ host: "", user: "pi", port: 22, identity_file: "", connect_timeout_secs: 8 });
  const [aggregator, setAggregator] = useState("");
  const [nodeBase, setNodeBase] = useState(10);
  const [checklist, setChecklist] = useState<ChecklistItem[]>(INITIAL_CHECKLIST);
  const [running, setRunning] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [done, setDone] = useState(false);
  const [watching, setWatching] = useState(false);

  const base = useMemo(() => resolveServerBase(serverStatus), [serverStatus]);

  useEffect(() => {
    setAggregator((prev) => prev || deriveAggregator(serverStatus));
  }, [serverStatus]);

  // Pi node ids stream at node_base by convention. Watch that id with a
  // nexmon-origin predicate.
  const piPredicate = (n: ServerNode) => (n.origin ?? "").toLowerCase().includes("nexmon");
  const watch = useNodeWatch({ base, targetId: nodeBase, enabled: watching, predicate: piPredicate, timeoutMs: 120000 });

  useEffect(() => {
    if (watching && watch.state === "found") setDone(true);
  }, [watching, watch.state]);

  function setStatus(key: string, status: StepStatus) {
    setChecklist((prev) => prev.map((item) => (item.key === key ? { ...item, status } : item)));
  }

  const config: PiAgentConfig = useMemo(
    () => ({
      listen: "0.0.0.0:5500",
      aggregator,
      node_base: nodeBase,
      tier: 2,
      default_rssi: -55,
      noise_floor: -92,
      mmwave_mock: false,
      enable_wasm: false,
      wasm_path: "",
      wasm_module_id: 1,
    }),
    [aggregator, nodeBase],
  );

  const aggregatorInvalid = !aggregator || isLoopbackHostPort(aggregator);

  async function runSequence() {
    if (!accessToken) {
      setError("Not authenticated — sign in to run Pi setup.");
      return;
    }
    if (!target.host.trim()) {
      setError("Enter the Pi's SSH host.");
      return;
    }
    if (aggregatorInvalid) {
      setError("Set the aggregator to this PC's LAN IP:port (not loopback) so the Pi streams here.");
      return;
    }

    setRunning(true);
    setError(null);
    setDone(false);
    setChecklist(INITIAL_CHECKLIST.map((i) => ({ ...i, status: "pending" })));

    const steps: Array<[string, () => Promise<PiNodeCommandResult>]> = [
      ["probe", () => tauriApi.piNodeProbe(accessToken, target)],
      ["prereqs", () => tauriApi.piNodeCheckPrereqs(accessToken, { target, installPackages: true })],
      ["config", () => tauriApi.piNodePushConfig(accessToken, { target, config })],
      ["service", () => tauriApi.piNodeInstallService(accessToken, { target, config, serviceName: "wave-pi-node-agent" })],
      ["csi", () => tauriApi.piNodeCsiHealth(accessToken, { target, nexmonPort: 5500, captureSeconds: 6, serviceName: "wave-pi-node-agent" })],
    ];

    try {
      for (const [key, run] of steps) {
        setStatus(key, "running");
        const result = await run();
        if (result.success) {
          setStatus(key, "pass");
        } else {
          setStatus(key, "fail");
          setError(`${key}: ${result.stderr || result.stdout || "command failed"}`);
          setRunning(false);
          return;
        }
      }
      // All commands passed — begin watching for the node to stream.
      if (base) {
        setWatching(true);
      } else {
        setDone(true);
      }
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setRunning(false);
    }
  }

  return (
    <div className="space-y-4">
      <div className="rounded-lg border border-border/60 bg-secondary/20 p-3">
        <div className="grid gap-3 sm:grid-cols-2">
          <div className="space-y-1.5">
            <Label htmlFor="pi-host">SSH Host</Label>
            <Input id="pi-host" value={target.host} onChange={(e) => setTarget((p) => ({ ...p, host: e.target.value }))} placeholder="192.168.1.42 or pi4.local" />
          </div>
          <div className="space-y-1.5">
            <Label htmlFor="pi-user">SSH User</Label>
            <Input id="pi-user" value={target.user ?? ""} onChange={(e) => setTarget((p) => ({ ...p, user: e.target.value }))} />
          </div>
          <div className="space-y-1.5">
            <Label htmlFor="pi-key">Identity file (optional)</Label>
            <Input id="pi-key" value={target.identity_file ?? ""} onChange={(e) => setTarget((p) => ({ ...p, identity_file: e.target.value }))} placeholder="~/.ssh/id_ed25519" />
          </div>
          <div className="space-y-1.5">
            <Label htmlFor="pi-nodebase">Node ID (base)</Label>
            <Input
              id="pi-nodebase"
              inputMode="numeric"
              value={String(nodeBase)}
              onChange={(e) => {
                const raw = e.target.value.trim();
                const n = Number(raw);
                if (raw !== "" && Number.isInteger(n) && n >= 0) setNodeBase(n);
              }}
            />
          </div>
          <div className="space-y-1.5 sm:col-span-2">
            <Label htmlFor="pi-agg">Aggregator (this PC)</Label>
            <Input id="pi-agg" value={aggregator} onChange={(e) => setAggregator(e.target.value)} placeholder="192.168.1.20:5005" />
            {aggregatorInvalid ? (
              <p className="text-xs text-amber-500">Must be this PC's LAN IP:port — a loopback would make the Pi stream to itself.</p>
            ) : null}
          </div>
        </div>
      </div>

      <ul className="space-y-1.5">
        {checklist.map((item) => (
          <li key={item.key} className="flex items-center gap-2 text-sm">
            {item.status === "pass" ? (
              <CheckCircle2 className="h-4 w-4 text-emerald-400" />
            ) : item.status === "fail" ? (
              <XCircle className="h-4 w-4 text-destructive" />
            ) : item.status === "running" ? (
              <Loader2 className="h-4 w-4 animate-spin text-primary" />
            ) : (
              <Circle className="h-4 w-4 text-muted-foreground" />
            )}
            <span className={item.status === "pending" ? "text-muted-foreground" : ""}>{item.label}</span>
          </li>
        ))}
      </ul>

      {watching && watch.state === "watching" ? (
        <StatusLine tone="muted">
          <Loader2 className="mr-2 inline h-4 w-4 animate-spin" /> Watching for the Pi (node {nodeBase}) to stream… ({watch.elapsedSecs}s)
        </StatusLine>
      ) : null}
      {watching && watch.state === "timeout" ? (
        <StatusLine tone="warn">The Pi hasn't started streaming yet. Check the service status on the Pi and its network path to this hub.</StatusLine>
      ) : null}
      {done ? (
        <StatusLine tone="ok">
          <CheckCircle2 className="mr-2 inline h-4 w-4" /> Pi node is set up. Continue to place it in 3D.
        </StatusLine>
      ) : null}
      {error ? <StatusLine tone="error">{error}</StatusLine> : null}

      <div className="flex flex-wrap gap-2">
        <Button onClick={runSequence} disabled={running || !accessToken}>
          {running ? <Loader2 className="mr-2 h-4 w-4 animate-spin" /> : null}
          Run Pi setup
        </Button>
      </div>
    </div>
  );
}
