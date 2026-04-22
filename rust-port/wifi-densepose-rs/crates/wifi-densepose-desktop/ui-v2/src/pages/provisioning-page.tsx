import { useMemo, useState } from "react";
import { tauriApi } from "@/lib/tauri-api";
import { PageSection } from "@/components/layout/page-section";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Switch } from "@/components/ui/switch";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { JsonViewer } from "@/components/layout/json-viewer";
import type { MeshNodeConfig, ProvisionResult, ProvisioningConfig, ValidationResult } from "@/types";

function parseCsvChannels(value: string) {
  return value
    .split(",")
    .map((segment) => segment.trim())
    .filter(Boolean)
    .map((segment) => Number(segment))
    .filter((segment) => Number.isFinite(segment));
}

export function ProvisioningPage() {
  const [port, setPort] = useState("");
  const [meshCount, setMeshCount] = useState("4");
  const [channelCsv, setChannelCsv] = useState("1,6,11");
  const [config, setConfig] = useState<ProvisioningConfig>({
    wifi_ssid: "",
    wifi_password: "",
    target_ip: "",
    target_port: 5005,
    node_id: 0,
    tdm_slot: 0,
    tdm_total: 4,
    edge_tier: 1,
    presence_thresh: 550,
    fall_thresh: 800,
    vital_window: 60,
    vital_interval_ms: 1000,
    top_k_count: 3,
    hop_count: 3,
    channel_list: [1, 6, 11],
    power_duty: 70,
    wasm_max_modules: 8,
    wasm_verify: true,
    ota_psk: "",
  });
  const [loading, setLoading] = useState(false);
  const [validation, setValidation] = useState<ValidationResult | null>(null);
  const [result, setResult] = useState<ProvisionResult | null>(null);
  const [nvs, setNvs] = useState<ProvisioningConfig | null>(null);
  const [mesh, setMesh] = useState<MeshNodeConfig[] | null>(null);
  const [error, setError] = useState<string | null>(null);

  const normalizedConfig = useMemo(
    () => ({
      ...config,
      channel_list: parseCsvChannels(channelCsv),
    }),
    [config, channelCsv],
  );

  async function withBusy(task: () => Promise<void>) {
    setLoading(true);
    setError(null);
    try {
      await task();
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setLoading(false);
    }
  }

  return (
    <div className="space-y-6">
      <PageSection title="Provisioning Inputs" description="Define node-level NVS parameters and apply over serial channel.">
        <Tabs defaultValue="network">
          <TabsList>
            <TabsTrigger value="network">Network</TabsTrigger>
            <TabsTrigger value="mesh">Mesh</TabsTrigger>
            <TabsTrigger value="signal">Signal</TabsTrigger>
            <TabsTrigger value="runtime">Runtime</TabsTrigger>
          </TabsList>
          <TabsContent value="network" className="space-y-4">
            <div className="grid gap-4 md:grid-cols-2">
              <div className="space-y-2">
                <Label htmlFor="prov-ssid">WiFi SSID</Label>
                <Input id="prov-ssid" value={config.wifi_ssid ?? ""} onChange={(e) => setConfig((prev) => ({ ...prev, wifi_ssid: e.target.value }))} />
              </div>
              <div className="space-y-2">
                <Label htmlFor="prov-password">WiFi Password</Label>
                <Input id="prov-password" type="password" value={config.wifi_password ?? ""} onChange={(e) => setConfig((prev) => ({ ...prev, wifi_password: e.target.value }))} />
              </div>
              <div className="space-y-2">
                <Label htmlFor="prov-target-ip">Target IP</Label>
                <Input id="prov-target-ip" value={config.target_ip ?? ""} onChange={(e) => setConfig((prev) => ({ ...prev, target_ip: e.target.value }))} />
              </div>
              <div className="space-y-2">
                <Label htmlFor="prov-target-port">Target Port</Label>
                <Input
                  id="prov-target-port"
                  value={String(config.target_port ?? "")}
                  onChange={(e) => setConfig((prev) => ({ ...prev, target_port: Number(e.target.value) }))}
                />
              </div>
            </div>
          </TabsContent>
          <TabsContent value="mesh" className="space-y-4">
            <div className="grid gap-4 md:grid-cols-3">
              <div className="space-y-2">
                <Label htmlFor="prov-node-id">Node ID</Label>
                <Input id="prov-node-id" value={String(config.node_id ?? "")} onChange={(e) => setConfig((prev) => ({ ...prev, node_id: Number(e.target.value) }))} />
              </div>
              <div className="space-y-2">
                <Label htmlFor="prov-slot">TDM Slot</Label>
                <Input id="prov-slot" value={String(config.tdm_slot ?? "")} onChange={(e) => setConfig((prev) => ({ ...prev, tdm_slot: Number(e.target.value) }))} />
              </div>
              <div className="space-y-2">
                <Label htmlFor="prov-total">TDM Total</Label>
                <Input id="prov-total" value={String(config.tdm_total ?? "")} onChange={(e) => setConfig((prev) => ({ ...prev, tdm_total: Number(e.target.value) }))} />
              </div>
              <div className="space-y-2">
                <Label htmlFor="prov-tier">Edge Tier</Label>
                <Input id="prov-tier" value={String(config.edge_tier ?? "")} onChange={(e) => setConfig((prev) => ({ ...prev, edge_tier: Number(e.target.value) }))} />
              </div>
              <div className="space-y-2">
                <Label htmlFor="prov-hop-count">Hop Count</Label>
                <Input id="prov-hop-count" value={String(config.hop_count ?? "")} onChange={(e) => setConfig((prev) => ({ ...prev, hop_count: Number(e.target.value) }))} />
              </div>
              <div className="space-y-2">
                <Label htmlFor="prov-channels">Channel List</Label>
                <Input id="prov-channels" value={channelCsv} onChange={(e) => setChannelCsv(e.target.value)} placeholder="1,6,11" />
              </div>
            </div>
          </TabsContent>
          <TabsContent value="signal" className="space-y-4">
            <div className="grid gap-4 md:grid-cols-3">
              <div className="space-y-2">
                <Label htmlFor="prov-presence">Presence Threshold</Label>
                <Input
                  id="prov-presence"
                  value={String(config.presence_thresh ?? "")}
                  onChange={(e) => setConfig((prev) => ({ ...prev, presence_thresh: Number(e.target.value) }))}
                />
              </div>
              <div className="space-y-2">
                <Label htmlFor="prov-fall">Fall Threshold</Label>
                <Input
                  id="prov-fall"
                  value={String(config.fall_thresh ?? "")}
                  onChange={(e) => setConfig((prev) => ({ ...prev, fall_thresh: Number(e.target.value) }))}
                />
              </div>
              <div className="space-y-2">
                <Label htmlFor="prov-vital-window">Vital Window</Label>
                <Input
                  id="prov-vital-window"
                  value={String(config.vital_window ?? "")}
                  onChange={(e) => setConfig((prev) => ({ ...prev, vital_window: Number(e.target.value) }))}
                />
              </div>
              <div className="space-y-2">
                <Label htmlFor="prov-vital-interval">Vital Interval (ms)</Label>
                <Input
                  id="prov-vital-interval"
                  value={String(config.vital_interval_ms ?? "")}
                  onChange={(e) => setConfig((prev) => ({ ...prev, vital_interval_ms: Number(e.target.value) }))}
                />
              </div>
              <div className="space-y-2">
                <Label htmlFor="prov-top-k">Top K Count</Label>
                <Input
                  id="prov-top-k"
                  value={String(config.top_k_count ?? "")}
                  onChange={(e) => setConfig((prev) => ({ ...prev, top_k_count: Number(e.target.value) }))}
                />
              </div>
              <div className="space-y-2">
                <Label htmlFor="prov-duty">Power Duty</Label>
                <Input id="prov-duty" value={String(config.power_duty ?? "")} onChange={(e) => setConfig((prev) => ({ ...prev, power_duty: Number(e.target.value) }))} />
              </div>
            </div>
          </TabsContent>
          <TabsContent value="runtime" className="space-y-4">
            <div className="grid gap-4 md:grid-cols-3">
              <div className="space-y-2">
                <Label htmlFor="prov-max-modules">WASM Max Modules</Label>
                <Input
                  id="prov-max-modules"
                  value={String(config.wasm_max_modules ?? "")}
                  onChange={(e) => setConfig((prev) => ({ ...prev, wasm_max_modules: Number(e.target.value) }))}
                />
              </div>
              <div className="space-y-2">
                <Label htmlFor="prov-ota-psk">OTA PSK</Label>
                <Input id="prov-ota-psk" value={config.ota_psk ?? ""} onChange={(e) => setConfig((prev) => ({ ...prev, ota_psk: e.target.value }))} />
              </div>
              <div className="flex items-center gap-3 self-end rounded-md border border-border/60 p-3">
                <Switch
                  checked={Boolean(config.wasm_verify)}
                  onCheckedChange={(checked) => setConfig((prev) => ({ ...prev, wasm_verify: checked }))}
                  id="prov-wasm-verify"
                />
                <Label htmlFor="prov-wasm-verify">Verify WASM signatures</Label>
              </div>
            </div>
          </TabsContent>
        </Tabs>
      </PageSection>

      <PageSection title="Provision Operations" description="Validate, write, read, and erase NVS payloads over serial transport.">
        <div className="grid gap-4 md:grid-cols-[1fr_auto]">
          <div className="space-y-2">
            <Label htmlFor="prov-port">Serial Port</Label>
            <Input id="prov-port" value={port} onChange={(e) => setPort(e.target.value)} placeholder="COM3 or /dev/ttyUSB0" />
          </div>
          <div className="flex items-end">
            <Button disabled={loading} onClick={() => withBusy(async () => setValidation(await tauriApi.validateConfig(normalizedConfig)))}>
              Validate Config
            </Button>
          </div>
        </div>

        <div className="mt-4 flex flex-wrap gap-2">
          <Button disabled={loading || !port} onClick={() => withBusy(async () => setResult(await tauriApi.provisionNode(port, normalizedConfig)))}>
            Provision Node
          </Button>
          <Button disabled={loading || !port} variant="secondary" onClick={() => withBusy(async () => setNvs(await tauriApi.readNvs(port)))}>
            Read NVS
          </Button>
          <Button disabled={loading || !port} variant="outline" onClick={() => withBusy(async () => setResult(await tauriApi.eraseNvs(port)))}>
            Erase NVS
          </Button>
        </div>
      </PageSection>

      <PageSection title="Mesh Config Generator" description="Generate per-node mesh config derivatives from current base payload.">
        <div className="grid gap-4 md:grid-cols-[220px_auto]">
          <div className="space-y-2">
            <Label htmlFor="mesh-count">Node Count</Label>
            <Input id="mesh-count" value={meshCount} onChange={(e) => setMeshCount(e.target.value)} />
          </div>
          <div className="flex items-end">
            <Button disabled={loading} onClick={() => withBusy(async () => setMesh(await tauriApi.generateMeshConfigs(normalizedConfig, Number(meshCount || "1"))))}>
              Generate Mesh Configs
            </Button>
          </div>
        </div>
      </PageSection>

      {validation ? (
        <PageSection title="Validation Result" description="Config validation response before provisioning.">
          <JsonViewer value={validation} />
        </PageSection>
      ) : null}

      {result ? (
        <PageSection title="Provision Result" description="Result from latest serial provisioning action.">
          <JsonViewer value={result} />
        </PageSection>
      ) : null}

      {nvs ? (
        <PageSection title="Read NVS" description="Decoded configuration read from target node.">
          <JsonViewer value={nvs} />
        </PageSection>
      ) : null}

      {mesh ? (
        <PageSection title="Generated Mesh Configs" description="Per-node derivatives generated from base config.">
          <JsonViewer value={mesh} />
        </PageSection>
      ) : null}

      {error ? <p className="rounded-md border border-destructive/40 bg-destructive/10 px-3 py-2 text-sm text-destructive">{error}</p> : null}
    </div>
  );
}

