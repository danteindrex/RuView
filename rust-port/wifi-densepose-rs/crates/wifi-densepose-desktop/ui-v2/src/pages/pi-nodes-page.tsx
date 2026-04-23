import { useEffect, useMemo, useState } from "react";
import { Activity, CheckCircle2, Loader2, PackageCheck, Play, RotateCw, Save, ShieldCheck, Square, UploadCloud, Wrench } from "lucide-react";
import { tauriApi } from "@/lib/tauri-api";
import { PageSection } from "@/components/layout/page-section";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Switch } from "@/components/ui/switch";
import { Textarea } from "@/components/ui/textarea";
import type { PiAgentConfig, PiNodeCommandResult, PiNodeTarget, PiServiceAction } from "@/types";

const DEFAULT_TARGET: PiNodeTarget = {
  host: "",
  user: "pi",
  port: 22,
  identity_file: "",
  connect_timeout_secs: 8,
};

const DEFAULT_CONFIG: PiAgentConfig = {
  listen: "0.0.0.0:5500",
  aggregator: "127.0.0.1:5005",
  node_base: 10,
  tier: 2,
  default_rssi: -55,
  noise_floor: -92,
  mmwave_mock: false,
  enable_wasm: false,
  wasm_path: "",
  wasm_module_id: 1,
};

function toNumber(value: string, fallback: number): number {
  const parsed = Number(value);
  return Number.isFinite(parsed) ? parsed : fallback;
}

function resultText(result: PiNodeCommandResult | null): string {
  if (!result) {
    return "";
  }
  const parts = [
    `$ ${result.command}`,
    result.stdout.trim(),
    result.stderr.trim() ? `stderr:\n${result.stderr.trim()}` : "",
    `exit=${result.exit_code ?? "unknown"} success=${result.success}`,
  ];
  return parts.filter(Boolean).join("\n\n");
}

export function PiNodesPage() {
  const [target, setTarget] = useState<PiNodeTarget>(DEFAULT_TARGET);
  const [config, setConfig] = useState<PiAgentConfig>(DEFAULT_CONFIG);
  const [serviceName, setServiceName] = useState("wave-pi-node-agent");
  const [binaryPath, setBinaryPath] = useState("/usr/local/bin/wifi-densepose-pi-node-agent");
  const [localBinaryPath, setLocalBinaryPath] = useState("");
  const [workspacePath, setWorkspacePath] = useState("");
  const [targetTriple, setTargetTriple] = useState("aarch64-unknown-linux-gnu");
  const [envPath, setEnvPath] = useState("/etc/wave/pi-node-agent.env");
  const [installPackages, setInstallPackages] = useState(true);
  const [captureSeconds, setCaptureSeconds] = useState(6);
  const [multiHosts, setMultiHosts] = useState("");
  const [wizardLog, setWizardLog] = useState("");
  const [result, setResult] = useState<PiNodeCommandResult | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    void (async () => {
      const settings = await tauriApi.getSettings().catch(() => null);
      if (!settings) {
        return;
      }
      setConfig({
        listen: settings.pi_agent_listen,
        aggregator: settings.pi_agent_aggregator,
        node_base: settings.pi_agent_node_base,
        tier: settings.pi_agent_tier,
        default_rssi: settings.pi_agent_default_rssi,
        noise_floor: settings.pi_agent_noise_floor,
        mmwave_mock: settings.pi_agent_mmwave_mock,
        enable_wasm: settings.pi_agent_enable_wasm,
        wasm_path: settings.pi_agent_wasm_path,
        wasm_module_id: settings.pi_agent_wasm_module_id,
      });
    })();
  }, []);

  const canRun = useMemo(() => Boolean(target.host.trim()), [target.host]);

  async function runTask(task: () => Promise<PiNodeCommandResult>): Promise<PiNodeCommandResult | null> {
    setLoading(true);
    setError(null);
    try {
      const response = await task();
      setResult(response);
      if (!response.success) {
        setError(response.stderr || response.stdout || "Pi command failed.");
      }
      return response;
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
      return null;
    } finally {
      setLoading(false);
    }
  }

  async function runService(action: PiServiceAction) {
    await runTask(() => tauriApi.piNodeService({ target, action, serviceName }));
  }

  async function runWizard() {
    const hosts = multiHosts
      .split(/\r?\n|,/)
      .map((entry) => entry.trim())
      .filter(Boolean);
    if (hosts.length === 0) {
      setError("Add at least one Pi host for the setup wizard.");
      return;
    }

    setLoading(true);
    setError(null);
    setWizardLog("");
    try {
      for (const host of hosts) {
        const nextTarget = { ...target, host };
        const steps: Array<[string, () => Promise<PiNodeCommandResult>]> = [
          ["check prereqs", () => tauriApi.piNodeCheckPrereqs({ target: nextTarget, installPackages })],
          ["push config", () => tauriApi.piNodePushConfig({ target: nextTarget, config, envPath })],
          ["install service", () => tauriApi.piNodeInstallService({ target: nextTarget, config, serviceName, binaryPath, envPath })],
          ["restart service", () => tauriApi.piNodeService({ target: nextTarget, action: "restart", serviceName })],
          ["csi health", () => tauriApi.piNodeCsiHealth({ target: nextTarget, nexmonPort: Number(config.listen.split(":").pop() || "5500"), captureSeconds, serviceName })],
        ];

        setWizardLog((prev) => `${prev}\n== ${host} ==\n`);
        for (const [label, step] of steps) {
          const response = await step();
          setResult(response);
          setWizardLog((prev) => `${prev}${response.success ? "PASS" : "FAIL"} ${label}\n${response.stdout}${response.stderr ? `\nstderr:\n${response.stderr}` : ""}\n`);
          if (!response.success) {
            setError(`${host}: ${label} failed`);
            break;
          }
        }
      }
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setLoading(false);
    }
  }

  return (
    <div className="space-y-6">
      <PageSection title="Pi Node Target" description="Connect to a Raspberry Pi over local SSH and manage the Wave Pi node agent service.">
        <div className="grid gap-4 md:grid-cols-3">
          <div className="space-y-2">
            <Label htmlFor="pi-host">Host</Label>
            <Input id="pi-host" value={target.host} onChange={(e) => setTarget((prev) => ({ ...prev, host: e.target.value }))} placeholder="192.168.1.42 or pi4.local" />
          </div>
          <div className="space-y-2">
            <Label htmlFor="pi-user">SSH User</Label>
            <Input id="pi-user" value={target.user ?? ""} onChange={(e) => setTarget((prev) => ({ ...prev, user: e.target.value }))} />
          </div>
          <div className="space-y-2">
            <Label htmlFor="pi-port">SSH Port</Label>
            <Input id="pi-port" value={String(target.port ?? "")} onChange={(e) => setTarget((prev) => ({ ...prev, port: toNumber(e.target.value, prev.port ?? 22) }))} />
          </div>
          <div className="space-y-2 md:col-span-2">
            <Label htmlFor="pi-key">Identity File</Label>
            <Input id="pi-key" value={target.identity_file ?? ""} onChange={(e) => setTarget((prev) => ({ ...prev, identity_file: e.target.value }))} placeholder="Optional SSH private key path" />
          </div>
          <div className="space-y-2">
            <Label htmlFor="pi-timeout">Connect Timeout (s)</Label>
            <Input
              id="pi-timeout"
              value={String(target.connect_timeout_secs ?? "")}
              onChange={(e) => setTarget((prev) => ({ ...prev, connect_timeout_secs: toNumber(e.target.value, prev.connect_timeout_secs ?? 8) }))}
            />
          </div>
        </div>
      </PageSection>

      <PageSection title="Agent Configuration" description="These values become the Pi-side environment file used by the systemd service.">
        <div className="grid gap-4 md:grid-cols-3">
          <div className="space-y-2">
            <Label htmlFor="agent-listen">Listen</Label>
            <Input id="agent-listen" value={config.listen} onChange={(e) => setConfig((prev) => ({ ...prev, listen: e.target.value }))} />
          </div>
          <div className="space-y-2">
            <Label htmlFor="agent-aggregator">Aggregator</Label>
            <Input id="agent-aggregator" value={config.aggregator} onChange={(e) => setConfig((prev) => ({ ...prev, aggregator: e.target.value }))} />
          </div>
          <div className="space-y-2">
            <Label htmlFor="agent-node-base">Node Base</Label>
            <Input id="agent-node-base" value={String(config.node_base)} onChange={(e) => setConfig((prev) => ({ ...prev, node_base: toNumber(e.target.value, prev.node_base) }))} />
          </div>
          <div className="space-y-2">
            <Label htmlFor="agent-tier">Tier</Label>
            <Input id="agent-tier" value={String(config.tier)} onChange={(e) => setConfig((prev) => ({ ...prev, tier: toNumber(e.target.value, prev.tier) }))} />
          </div>
          <div className="space-y-2">
            <Label htmlFor="agent-rssi">Default RSSI</Label>
            <Input id="agent-rssi" value={String(config.default_rssi)} onChange={(e) => setConfig((prev) => ({ ...prev, default_rssi: toNumber(e.target.value, prev.default_rssi) }))} />
          </div>
          <div className="space-y-2">
            <Label htmlFor="agent-noise">Noise Floor</Label>
            <Input id="agent-noise" value={String(config.noise_floor)} onChange={(e) => setConfig((prev) => ({ ...prev, noise_floor: toNumber(e.target.value, prev.noise_floor) }))} />
          </div>
          <div className="space-y-2">
            <Label htmlFor="agent-wasm-path">WASM Path</Label>
            <Input id="agent-wasm-path" value={config.wasm_path ?? ""} onChange={(e) => setConfig((prev) => ({ ...prev, wasm_path: e.target.value }))} />
          </div>
          <div className="space-y-2">
            <Label htmlFor="agent-wasm-module">WASM Module ID</Label>
            <Input
              id="agent-wasm-module"
              value={String(config.wasm_module_id)}
              onChange={(e) => setConfig((prev) => ({ ...prev, wasm_module_id: toNumber(e.target.value, prev.wasm_module_id) }))}
            />
          </div>
          <div className="flex items-center gap-3 rounded-md border border-border/60 p-3">
            <Switch checked={config.mmwave_mock} onCheckedChange={(checked) => setConfig((prev) => ({ ...prev, mmwave_mock: checked }))} id="agent-mmwave" />
            <Label htmlFor="agent-mmwave">mmWave mock</Label>
          </div>
          <div className="flex items-center gap-3 rounded-md border border-border/60 p-3">
            <Switch checked={config.enable_wasm} onCheckedChange={(checked) => setConfig((prev) => ({ ...prev, enable_wasm: checked }))} id="agent-wasm" />
            <Label htmlFor="agent-wasm">WASM events</Label>
          </div>
        </div>
      </PageSection>

      <PageSection title="Service Control" description="Push config, install the systemd wrapper, and control the Pi-side service.">
        <div className="grid gap-4 md:grid-cols-3">
          <div className="space-y-2">
            <Label htmlFor="service-name">Service Name</Label>
            <Input id="service-name" value={serviceName} onChange={(e) => setServiceName(e.target.value)} />
          </div>
          <div className="space-y-2">
            <Label htmlFor="binary-path">Binary Path</Label>
            <Input id="binary-path" value={binaryPath} onChange={(e) => setBinaryPath(e.target.value)} />
          </div>
          <div className="space-y-2">
            <Label htmlFor="env-path">Environment Path</Label>
            <Input id="env-path" value={envPath} onChange={(e) => setEnvPath(e.target.value)} />
          </div>
        </div>
        <div className="mt-4 grid gap-4 md:grid-cols-3">
          <div className="space-y-2">
            <Label htmlFor="local-binary-path">Local Binary Path</Label>
            <Input id="local-binary-path" value={localBinaryPath} onChange={(e) => setLocalBinaryPath(e.target.value)} placeholder="target/.../wifi-densepose-pi-node-agent" />
          </div>
          <div className="space-y-2">
            <Label htmlFor="target-triple">Build Target</Label>
            <Input id="target-triple" value={targetTriple} onChange={(e) => setTargetTriple(e.target.value)} placeholder="aarch64-unknown-linux-gnu" />
          </div>
          <div className="space-y-2">
            <Label htmlFor="capture-seconds">CSI Capture Seconds</Label>
            <Input id="capture-seconds" value={String(captureSeconds)} onChange={(e) => setCaptureSeconds(toNumber(e.target.value, captureSeconds))} />
          </div>
          <div className="space-y-2 md:col-span-3">
            <Label htmlFor="workspace-path">Rust Workspace Path</Label>
            <Input id="workspace-path" value={workspacePath} onChange={(e) => setWorkspacePath(e.target.value)} placeholder="C:\\Users\\user\\Documents\\RuView\\rust-port\\wifi-densepose-rs" />
          </div>
        </div>

        <div className="mt-4 flex flex-wrap gap-2">
          <Button disabled={loading || !canRun} onClick={() => runTask(() => tauriApi.piNodeProbe(target))}>
            {loading ? <Loader2 className="mr-2 h-4 w-4 animate-spin" /> : <Activity className="mr-2 h-4 w-4" />}
            Check Pi
          </Button>
          <Button disabled={loading || !canRun} variant="secondary" onClick={() => runTask(() => tauriApi.piNodeCheckPrereqs({ target, installPackages }))}>
            <ShieldCheck className="mr-2 h-4 w-4" />
            Check Prereqs
          </Button>
          <Button disabled={loading} variant="secondary" onClick={() => runTask(() => tauriApi.piNodeBuildAgent({ workspace_path: workspacePath, target_triple: targetTriple, release: true }))}>
            <PackageCheck className="mr-2 h-4 w-4" />
            Build Agent
          </Button>
          <Button disabled={loading || !canRun || !localBinaryPath} variant="secondary" onClick={() => runTask(() => tauriApi.piNodeDeployBinary({ target, localBinaryPath, remoteBinaryPath: binaryPath }))}>
            <UploadCloud className="mr-2 h-4 w-4" />
            Deploy Binary
          </Button>
          <Button disabled={loading || !canRun} variant="secondary" onClick={() => runTask(() => tauriApi.piNodePushConfig({ target, config, envPath }))}>
            <Save className="mr-2 h-4 w-4" />
            Push Config
          </Button>
          <Button disabled={loading || !canRun} variant="secondary" onClick={() => runTask(() => tauriApi.piNodeInstallService({ target, config, serviceName, binaryPath, envPath }))}>
            <Wrench className="mr-2 h-4 w-4" />
            Install Service
          </Button>
          <Button disabled={loading || !canRun} variant="outline" onClick={() => runService("start")}>
            <Play className="mr-2 h-4 w-4" />
            Start
          </Button>
          <Button disabled={loading || !canRun} variant="outline" onClick={() => runService("restart")}>
            <RotateCw className="mr-2 h-4 w-4" />
            Restart
          </Button>
          <Button disabled={loading || !canRun} variant="outline" onClick={() => runService("stop")}>
            <Square className="mr-2 h-4 w-4" />
            Stop
          </Button>
          <Button disabled={loading || !canRun} variant="ghost" onClick={() => runService("status")}>
            Status
          </Button>
          <Button disabled={loading || !canRun} variant="ghost" onClick={() => runTask(() => tauriApi.piNodeCsiHealth({ target, nexmonPort: Number(config.listen.split(":").pop() || "5500"), captureSeconds, serviceName }))}>
            <CheckCircle2 className="mr-2 h-4 w-4" />
            CSI Health
          </Button>
          <div className="flex items-center gap-3 rounded-md border border-border/60 px-3 py-2">
            <Switch checked={installPackages} onCheckedChange={setInstallPackages} id="install-packages" />
            <Label htmlFor="install-packages">Install apt tools</Label>
          </div>
        </div>
      </PageSection>

      <PageSection title="Multi-Pi Setup" description="Run the setup sequence against several Pi hosts using the current target auth and agent profile.">
        <div className="space-y-3">
          <Label htmlFor="multi-hosts">Hosts</Label>
          <Textarea
            id="multi-hosts"
            className="min-h-[90px] font-mono text-xs"
            value={multiHosts}
            onChange={(e) => setMultiHosts(e.target.value)}
            placeholder="192.168.1.42&#10;192.168.1.43&#10;pi4-lab.local"
          />
          <div className="flex flex-wrap gap-2">
            <Button disabled={loading} onClick={runWizard}>
              {loading ? <Loader2 className="mr-2 h-4 w-4 animate-spin" /> : <Wrench className="mr-2 h-4 w-4" />}
              Run Setup Wizard
            </Button>
            <Button disabled={loading || !target.host} variant="secondary" onClick={() => setMultiHosts((prev) => `${prev}${prev.trim() ? "\n" : ""}${target.host}`)}>
              Add Current Host
            </Button>
          </div>
          <Textarea className="min-h-[220px] font-mono text-xs" readOnly value={wizardLog.trim()} />
        </div>
      </PageSection>

      <PageSection title="Pi Command Output" description="Most recent SSH command result.">
        <div className="mb-3 flex flex-wrap gap-2">
          <Badge variant={result?.success ? "success" : result ? "danger" : "outline"}>{result ? (result.success ? "Success" : "Failed") : "No Result"}</Badge>
          <Badge variant="outline">Exit: {result?.exit_code ?? "N/A"}</Badge>
        </div>
        <Textarea className="min-h-[260px] font-mono text-xs" readOnly value={resultText(result)} />
      </PageSection>

      {error ? <p className="rounded-md border border-destructive/40 bg-destructive/10 px-3 py-2 text-sm text-destructive">{error}</p> : null}
    </div>
  );
}
