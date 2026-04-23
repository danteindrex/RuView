import { useEffect, useState } from "react";
import { tauriApi } from "@/lib/tauri-api";
import { PageSection } from "@/components/layout/page-section";
import { Accordion, AccordionContent, AccordionItem, AccordionTrigger } from "@/components/ui/accordion";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Switch } from "@/components/ui/switch";
import { JsonViewer } from "@/components/layout/json-viewer";
import type { AppSettings } from "@/types";

interface SettingsPageProps {
  theme: "light" | "dark";
  onThemeChange: (theme: "light" | "dark") => void;
}

const DEFAULT_SETTINGS: AppSettings = {
  server_http_port: 8080,
  server_ws_port: 8765,
  server_udp_port: 5005,
  server_nexmon_port: 5500,
  bind_address: "127.0.0.1",
  server_source: "auto",
  server_tick_ms: 100,
  ui_path: "",
  server_pi_diag: false,
  server_model_path: "",
  server_load_rvf_path: "",
  server_save_rvf_path: "",
  server_progressive: false,
  server_node_positions: "",
  server_calibrate_on_boot: false,
  server_dataset_path: "",
  server_dataset_type: "mmfi",
  server_epochs: 100,
  server_pretrain_epochs: 50,
  server_checkpoint_dir: "",
  server_export_rvf_path: "",
  server_build_index: "",
  server_enable_benchmark: false,
  server_enable_train: false,
  server_enable_pretrain: false,
  server_enable_embed: false,
  ota_psk: "",
  auto_discover: true,
  discover_interval_ms: 10000,
  theme: "dark",
  pi_agent_enabled: false,
  pi_agent_listen: "0.0.0.0:5500",
  pi_agent_aggregator: "127.0.0.1:5005",
  pi_agent_node_base: 10,
  pi_agent_tier: 2,
  pi_agent_default_rssi: -55,
  pi_agent_noise_floor: -92,
  pi_agent_mmwave_mock: false,
  pi_agent_enable_wasm: false,
  pi_agent_wasm_path: "",
  pi_agent_wasm_module_id: 1,
};

function toNumber(value: string, fallback: number): number {
  const parsed = Number(value);
  return Number.isFinite(parsed) ? parsed : fallback;
}

export function SettingsPage({ theme, onThemeChange }: SettingsPageProps) {
  const [settings, setSettings] = useState<AppSettings>({ ...DEFAULT_SETTINGS, theme });
  const [loaded, setLoaded] = useState<AppSettings | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [message, setMessage] = useState<string | null>(null);

  useEffect(() => {
    void (async () => {
      setLoading(true);
      try {
        const persisted = await tauriApi.getSettings();
        if (persisted) {
          const merged = { ...DEFAULT_SETTINGS, ...persisted };
          setSettings(merged);
          setLoaded(merged);
          if (merged.theme === "dark" || merged.theme === "light") {
            onThemeChange(merged.theme);
          }
        }
      } catch (err) {
        setError(err instanceof Error ? err.message : String(err));
      } finally {
        setLoading(false);
      }
    })();
  }, [onThemeChange]);

  async function handleSave() {
    setLoading(true);
    setError(null);
    setMessage(null);
    try {
      await tauriApi.saveSettings(settings);
      setLoaded(settings);
      setMessage("Settings saved.");
      if (settings.theme === "dark" || settings.theme === "light") {
        onThemeChange(settings.theme);
      }
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setLoading(false);
    }
  }

  return (
    <div className="space-y-6">
      <PageSection title="Advanced Settings" description="All advanced controls are grouped here to keep operational pages focused and predictable.">
        <Accordion type="multiple" defaultValue={["runtime", "model", "discovery", "security", "pi-agent"]}>
          <AccordionItem value="runtime">
            <AccordionTrigger>Runtime and Transport</AccordionTrigger>
            <AccordionContent>
              <div className="grid gap-4 md:grid-cols-2">
                <div className="space-y-2">
                  <Label htmlFor="set-http">HTTP Port</Label>
                  <Input id="set-http" value={String(settings.server_http_port)} onChange={(e) => setSettings((prev) => ({ ...prev, server_http_port: toNumber(e.target.value, prev.server_http_port) }))} />
                </div>
                <div className="space-y-2">
                  <Label htmlFor="set-ws">WS Port</Label>
                  <Input id="set-ws" value={String(settings.server_ws_port)} onChange={(e) => setSettings((prev) => ({ ...prev, server_ws_port: toNumber(e.target.value, prev.server_ws_port) }))} />
                </div>
                <div className="space-y-2">
                  <Label htmlFor="set-udp">UDP Port</Label>
                  <Input id="set-udp" value={String(settings.server_udp_port)} onChange={(e) => setSettings((prev) => ({ ...prev, server_udp_port: toNumber(e.target.value, prev.server_udp_port) }))} />
                </div>
                <div className="space-y-2">
                  <Label htmlFor="set-nexmon">Nexmon Port</Label>
                  <Input id="set-nexmon" value={String(settings.server_nexmon_port)} onChange={(e) => setSettings((prev) => ({ ...prev, server_nexmon_port: toNumber(e.target.value, prev.server_nexmon_port) }))} />
                </div>
                <div className="space-y-2">
                  <Label htmlFor="set-bind">Bind Address</Label>
                  <Input id="set-bind" value={settings.bind_address} onChange={(e) => setSettings((prev) => ({ ...prev, bind_address: e.target.value }))} />
                </div>
                <div className="space-y-2">
                  <Label htmlFor="set-source">Data Source</Label>
                  <Input id="set-source" value={settings.server_source} onChange={(e) => setSettings((prev) => ({ ...prev, server_source: e.target.value }))} placeholder="auto | wifi | esp32 | nexmon | simulate" />
                </div>
                <div className="space-y-2">
                  <Label htmlFor="set-tick-ms">Tick (ms)</Label>
                  <Input id="set-tick-ms" value={String(settings.server_tick_ms)} onChange={(e) => setSettings((prev) => ({ ...prev, server_tick_ms: toNumber(e.target.value, prev.server_tick_ms) }))} />
                </div>
                <div className="space-y-2">
                  <Label htmlFor="set-ui-path">UI Path</Label>
                  <Input id="set-ui-path" value={settings.ui_path} onChange={(e) => setSettings((prev) => ({ ...prev, ui_path: e.target.value }))} />
                </div>
                <div className="flex items-center gap-3 rounded-md border border-border/60 p-3">
                  <Switch checked={settings.server_pi_diag} onCheckedChange={(checked) => setSettings((prev) => ({ ...prev, server_pi_diag: checked }))} id="set-pi-diag" />
                  <Label htmlFor="set-pi-diag">Enable Pi diagnostics</Label>
                </div>
                <div className="flex items-center gap-3 rounded-md border border-border/60 p-3">
                  <Switch checked={settings.server_calibrate_on_boot} onCheckedChange={(checked) => setSettings((prev) => ({ ...prev, server_calibrate_on_boot: checked }))} id="set-calibrate" />
                  <Label htmlFor="set-calibrate">Calibrate on boot</Label>
                </div>
                <div className="flex items-center gap-3 rounded-md border border-border/60 p-3">
                  <Switch checked={settings.server_progressive} onCheckedChange={(checked) => setSettings((prev) => ({ ...prev, server_progressive: checked }))} id="set-progressive" />
                  <Label htmlFor="set-progressive">Progressive model loading</Label>
                </div>
              </div>
            </AccordionContent>
          </AccordionItem>

          <AccordionItem value="model">
            <AccordionTrigger>Model and Training Modes</AccordionTrigger>
            <AccordionContent>
              <div className="grid gap-4 md:grid-cols-2">
                <div className="space-y-2">
                  <Label htmlFor="set-model">Model Path</Label>
                  <Input id="set-model" value={settings.server_model_path} onChange={(e) => setSettings((prev) => ({ ...prev, server_model_path: e.target.value }))} />
                </div>
                <div className="space-y-2">
                  <Label htmlFor="set-load-rvf">Load RVF Path</Label>
                  <Input id="set-load-rvf" value={settings.server_load_rvf_path} onChange={(e) => setSettings((prev) => ({ ...prev, server_load_rvf_path: e.target.value }))} />
                </div>
                <div className="space-y-2">
                  <Label htmlFor="set-save-rvf">Save RVF Path</Label>
                  <Input id="set-save-rvf" value={settings.server_save_rvf_path} onChange={(e) => setSettings((prev) => ({ ...prev, server_save_rvf_path: e.target.value }))} />
                </div>
                <div className="space-y-2">
                  <Label htmlFor="set-export-rvf">Export RVF Path</Label>
                  <Input id="set-export-rvf" value={settings.server_export_rvf_path} onChange={(e) => setSettings((prev) => ({ ...prev, server_export_rvf_path: e.target.value }))} />
                </div>
                <div className="space-y-2">
                  <Label htmlFor="set-dataset">Dataset Path</Label>
                  <Input id="set-dataset" value={settings.server_dataset_path} onChange={(e) => setSettings((prev) => ({ ...prev, server_dataset_path: e.target.value }))} />
                </div>
                <div className="space-y-2">
                  <Label htmlFor="set-dataset-type">Dataset Type</Label>
                  <Input id="set-dataset-type" value={settings.server_dataset_type} onChange={(e) => setSettings((prev) => ({ ...prev, server_dataset_type: e.target.value }))} placeholder="mmfi | wipose" />
                </div>
                <div className="space-y-2">
                  <Label htmlFor="set-epochs">Epochs</Label>
                  <Input id="set-epochs" value={String(settings.server_epochs)} onChange={(e) => setSettings((prev) => ({ ...prev, server_epochs: toNumber(e.target.value, prev.server_epochs) }))} />
                </div>
                <div className="space-y-2">
                  <Label htmlFor="set-pretrain-epochs">Pretrain Epochs</Label>
                  <Input
                    id="set-pretrain-epochs"
                    value={String(settings.server_pretrain_epochs)}
                    onChange={(e) => setSettings((prev) => ({ ...prev, server_pretrain_epochs: toNumber(e.target.value, prev.server_pretrain_epochs) }))}
                  />
                </div>
                <div className="space-y-2">
                  <Label htmlFor="set-checkpoint-dir">Checkpoint Directory</Label>
                  <Input id="set-checkpoint-dir" value={settings.server_checkpoint_dir} onChange={(e) => setSettings((prev) => ({ ...prev, server_checkpoint_dir: e.target.value }))} />
                </div>
                <div className="space-y-2">
                  <Label htmlFor="set-build-index">Build Index Type</Label>
                  <Input id="set-build-index" value={settings.server_build_index} onChange={(e) => setSettings((prev) => ({ ...prev, server_build_index: e.target.value }))} placeholder="env | activity | temporal | person" />
                </div>
                <div className="space-y-2 md:col-span-2">
                  <Label htmlFor="set-node-positions">Node Positions</Label>
                  <Input id="set-node-positions" value={settings.server_node_positions} onChange={(e) => setSettings((prev) => ({ ...prev, server_node_positions: e.target.value }))} placeholder="x,y,z;x,y,z;..." />
                </div>
                <div className="flex items-center gap-3 rounded-md border border-border/60 p-3">
                  <Switch checked={settings.server_enable_benchmark} onCheckedChange={(checked) => setSettings((prev) => ({ ...prev, server_enable_benchmark: checked }))} id="set-enable-benchmark" />
                  <Label htmlFor="set-enable-benchmark">Enable benchmark mode</Label>
                </div>
                <div className="flex items-center gap-3 rounded-md border border-border/60 p-3">
                  <Switch checked={settings.server_enable_train} onCheckedChange={(checked) => setSettings((prev) => ({ ...prev, server_enable_train: checked }))} id="set-enable-train" />
                  <Label htmlFor="set-enable-train">Enable train mode</Label>
                </div>
                <div className="flex items-center gap-3 rounded-md border border-border/60 p-3">
                  <Switch checked={settings.server_enable_pretrain} onCheckedChange={(checked) => setSettings((prev) => ({ ...prev, server_enable_pretrain: checked }))} id="set-enable-pretrain" />
                  <Label htmlFor="set-enable-pretrain">Enable pretrain mode</Label>
                </div>
                <div className="flex items-center gap-3 rounded-md border border-border/60 p-3">
                  <Switch checked={settings.server_enable_embed} onCheckedChange={(checked) => setSettings((prev) => ({ ...prev, server_enable_embed: checked }))} id="set-enable-embed" />
                  <Label htmlFor="set-enable-embed">Enable embed mode</Label>
                </div>
              </div>
            </AccordionContent>
          </AccordionItem>

          <AccordionItem value="discovery">
            <AccordionTrigger>Discovery Behavior</AccordionTrigger>
            <AccordionContent>
              <div className="grid gap-4 md:grid-cols-2">
                <div className="space-y-2">
                  <Label htmlFor="set-discovery-ms">Auto Discover Interval (ms)</Label>
                  <Input
                    id="set-discovery-ms"
                    value={String(settings.discover_interval_ms)}
                    onChange={(e) => setSettings((prev) => ({ ...prev, discover_interval_ms: toNumber(e.target.value, prev.discover_interval_ms) }))}
                  />
                </div>
                <div className="flex items-center gap-3 rounded-md border border-border/60 p-3">
                  <Switch checked={settings.auto_discover} onCheckedChange={(checked) => setSettings((prev) => ({ ...prev, auto_discover: checked }))} id="set-auto-discover" />
                  <Label htmlFor="set-auto-discover">Enable auto discovery</Label>
                </div>
              </div>
            </AccordionContent>
          </AccordionItem>

          <AccordionItem value="security">
            <AccordionTrigger>Security and OTA</AccordionTrigger>
            <AccordionContent>
              <div className="grid gap-4 md:grid-cols-2">
                <div className="space-y-2">
                  <Label htmlFor="set-ota-psk">OTA PSK</Label>
                  <Input id="set-ota-psk" value={settings.ota_psk} onChange={(e) => setSettings((prev) => ({ ...prev, ota_psk: e.target.value }))} />
                </div>
              </div>
            </AccordionContent>
          </AccordionItem>

          <AccordionItem value="pi-agent">
            <AccordionTrigger>Pi Agent Profile</AccordionTrigger>
            <AccordionContent>
              <div className="grid gap-4 md:grid-cols-2">
                <div className="space-y-2">
                  <Label htmlFor="set-pi-listen">Listen</Label>
                  <Input id="set-pi-listen" value={settings.pi_agent_listen} onChange={(e) => setSettings((prev) => ({ ...prev, pi_agent_listen: e.target.value }))} />
                </div>
                <div className="space-y-2">
                  <Label htmlFor="set-pi-aggregator">Aggregator</Label>
                  <Input id="set-pi-aggregator" value={settings.pi_agent_aggregator} onChange={(e) => setSettings((prev) => ({ ...prev, pi_agent_aggregator: e.target.value }))} />
                </div>
                <div className="space-y-2">
                  <Label htmlFor="set-pi-node-base">Node Base</Label>
                  <Input id="set-pi-node-base" value={String(settings.pi_agent_node_base)} onChange={(e) => setSettings((prev) => ({ ...prev, pi_agent_node_base: toNumber(e.target.value, prev.pi_agent_node_base) }))} />
                </div>
                <div className="space-y-2">
                  <Label htmlFor="set-pi-tier">Tier</Label>
                  <Input id="set-pi-tier" value={String(settings.pi_agent_tier)} onChange={(e) => setSettings((prev) => ({ ...prev, pi_agent_tier: toNumber(e.target.value, prev.pi_agent_tier) }))} />
                </div>
                <div className="space-y-2">
                  <Label htmlFor="set-pi-default-rssi">Default RSSI</Label>
                  <Input
                    id="set-pi-default-rssi"
                    value={String(settings.pi_agent_default_rssi)}
                    onChange={(e) => setSettings((prev) => ({ ...prev, pi_agent_default_rssi: toNumber(e.target.value, prev.pi_agent_default_rssi) }))}
                  />
                </div>
                <div className="space-y-2">
                  <Label htmlFor="set-pi-noise-floor">Noise Floor</Label>
                  <Input
                    id="set-pi-noise-floor"
                    value={String(settings.pi_agent_noise_floor)}
                    onChange={(e) => setSettings((prev) => ({ ...prev, pi_agent_noise_floor: toNumber(e.target.value, prev.pi_agent_noise_floor) }))}
                  />
                </div>
                <div className="space-y-2">
                  <Label htmlFor="set-pi-wasm-path">WASM Path</Label>
                  <Input id="set-pi-wasm-path" value={settings.pi_agent_wasm_path} onChange={(e) => setSettings((prev) => ({ ...prev, pi_agent_wasm_path: e.target.value }))} />
                </div>
                <div className="space-y-2">
                  <Label htmlFor="set-pi-wasm-module-id">WASM Module ID</Label>
                  <Input
                    id="set-pi-wasm-module-id"
                    value={String(settings.pi_agent_wasm_module_id)}
                    onChange={(e) => setSettings((prev) => ({ ...prev, pi_agent_wasm_module_id: toNumber(e.target.value, prev.pi_agent_wasm_module_id) }))}
                  />
                </div>
                <div className="flex items-center gap-3 rounded-md border border-border/60 p-3">
                  <Switch checked={settings.pi_agent_enabled} onCheckedChange={(checked) => setSettings((prev) => ({ ...prev, pi_agent_enabled: checked }))} id="set-pi-enabled" />
                  <Label htmlFor="set-pi-enabled">Enable Pi agent profile</Label>
                </div>
                <div className="flex items-center gap-3 rounded-md border border-border/60 p-3">
                  <Switch checked={settings.pi_agent_mmwave_mock} onCheckedChange={(checked) => setSettings((prev) => ({ ...prev, pi_agent_mmwave_mock: checked }))} id="set-pi-mmwave-mock" />
                  <Label htmlFor="set-pi-mmwave-mock">Enable mmWave mock</Label>
                </div>
                <div className="flex items-center gap-3 rounded-md border border-border/60 p-3">
                  <Switch checked={settings.pi_agent_enable_wasm} onCheckedChange={(checked) => setSettings((prev) => ({ ...prev, pi_agent_enable_wasm: checked }))} id="set-pi-enable-wasm" />
                  <Label htmlFor="set-pi-enable-wasm">Enable WASM</Label>
                </div>
              </div>
            </AccordionContent>
          </AccordionItem>
        </Accordion>

        <div className="mt-4 flex flex-wrap gap-2">
          <Button disabled={loading} onClick={handleSave}>
            Save Settings
          </Button>
          <Button
            disabled={loading}
            variant="secondary"
            onClick={() => {
              const next = settings.theme === "dark" ? "light" : "dark";
              setSettings((prev) => ({ ...prev, theme: next }));
              onThemeChange(next);
            }}
          >
            Toggle Theme
          </Button>
        </div>
      </PageSection>

      <PageSection title="Persisted Snapshot" description="Current in-memory and saved settings state.">
        <JsonViewer value={{ inMemory: settings, loaded }} />
      </PageSection>

      {message ? <p className="rounded-md border border-emerald-500/40 bg-emerald-500/10 px-3 py-2 text-sm text-emerald-400">{message}</p> : null}
      {error ? <p className="rounded-md border border-destructive/40 bg-destructive/10 px-3 py-2 text-sm text-destructive">{error}</p> : null}
    </div>
  );
}
