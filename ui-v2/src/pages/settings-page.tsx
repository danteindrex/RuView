import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { tauriApi } from "@/lib/tauri-api";
import { PageSection } from "@/components/layout/page-section";
import { Accordion, AccordionContent, AccordionItem, AccordionTrigger } from "@/components/ui/accordion";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Switch } from "@/components/ui/switch";
import { JsonViewer } from "@/components/layout/json-viewer";
import { useEnterpriseSettings } from "@/lib/enterprise-store";
import { useAuthStore } from "@/lib/auth-store";
import { QrCode, MessageSquare, ShieldCheck, Database, Compass, MapPin } from "lucide-react";
import { isLoopbackHostPort } from "@/lib/utils";
import { useOnboardingStore, STEP_COUNT } from "@/lib/onboarding-store";
import type { AppSettings } from "@/types";

const ONBOARDING_STEP_LABELS = ["Welcome", "Hub check", "Add node", "Place in 3D", "Calibrate", "Tour"];

interface SettingsPageProps {
  theme: "light" | "dark";
  onThemeChange: (theme: "light" | "dark") => void;
}

const DEFAULT_SETTINGS: AppSettings = {
  server_http_port: 4000,
  server_ws_port: 4001,
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
  // Where the Pi agent streams CSI — must be this machine's LAN address, not a
  // loopback default (a Pi with 127.0.0.1 would stream to its own loopback).
  pi_agent_aggregator: "",
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

interface LangfuseState {
  enabled: boolean;
  host: string;
  public_key: string;
  secret_key: string;
}

export function SettingsPage({ theme, onThemeChange }: SettingsPageProps) {
  const { accessToken } = useAuthStore();
  const openOnboarding = useOnboardingStore((s) => s.openAt);
  const { settings: entSettings, saveSettings: saveEnt, getWhapiQR, sendTestMessage, loading: entLoading, error: entError } = useEnterpriseSettings();
  const [localEnt, setLocalEnt] = useState(entSettings);
  const [qrCode, setQrCode] = useState<string | null>(null);

  const [langfuse, setLangfuse] = useState<LangfuseState>({
    enabled: false,
    host: "https://cloud.langfuse.com",
    public_key: "",
    secret_key: "",
  });
  const [langfuseHint, setLangfuseHint] = useState<string>("");
  const [langfuseSaving, setLangfuseSaving] = useState(false);
  const [langfuseMessage, setLangfuseMessage] = useState<string | null>(null);

  useEffect(() => {
    void tauriApi.getLangfuseConfig().then((cfg) => {
      setLangfuse((prev) => ({ ...prev, enabled: cfg.enabled, host: cfg.host }));
      setLangfuseHint(cfg.public_key_hint);
    }).catch(() => {/* command may not be registered in dev */});
  }, []);

  async function handleSaveLangfuse() {
    setLangfuseSaving(true);
    setLangfuseMessage(null);
    try {
      await tauriApi.setLangfuseConfig(langfuse);
      setLangfuseMessage(langfuse.enabled ? "Langfuse tracing enabled." : "Langfuse tracing disabled.");
    } catch (err) {
      setLangfuseMessage(err instanceof Error ? err.message : String(err));
    } finally {
      setLangfuseSaving(false);
    }
  }

  useEffect(() => {
    if (entSettings) setLocalEnt(entSettings);
  }, [entSettings]);

  const handleSaveEnt = async () => {
    if (localEnt) await saveEnt(localEnt);
  };

  const [qrLoading, setQrLoading] = useState(false);
  const handleGetQR = async () => {
    setQrCode(null);
    setQrLoading(true);
    // Save current local changes (especially the token) before requesting QR
    if (localEnt) {
      const success = await saveEnt(localEnt);
      if (!success) {
        setQrLoading(false);
        return;
      }
    }
    try {
      const qr = await getWhapiQR();
      if (qr) setQrCode(qr);
    } finally {
      setQrLoading(false);
    }
  };

  // ── Deployment Identity ───────────────────────────────────────────────────
  const [depName, setDepName] = useState("");
  const [depLocation, setDepLocation] = useState("");
  const [depLat, setDepLat] = useState("");
  const [depLng, setDepLng] = useState("");
  const [depSaving, setDepSaving] = useState(false);
  const [depRegistering, setDepRegistering] = useState(false);
  const [depMessage, setDepMessage] = useState<string | null>(null);

  useEffect(() => {
    void invoke<{ deployment_name: string; location_name: string; latitude?: number | null; longitude?: number | null }>(
      "get_deployment_info"
    ).then((info) => {
      setDepName(info.deployment_name ?? "");
      setDepLocation(info.location_name ?? "");
      setDepLat(info.latitude != null ? String(info.latitude) : "");
      setDepLng(info.longitude != null ? String(info.longitude) : "");
    }).catch(() => {/* command not yet registered in this branch */});
  }, []);

  async function handleSaveDeployment() {
    setDepSaving(true);
    setDepMessage(null);
    try {
      await invoke("set_deployment_info", {
        deploymentName: depName,
        locationName: depLocation,
        latitude: depLat !== "" ? parseFloat(depLat) : null,
        longitude: depLng !== "" ? parseFloat(depLng) : null,
      });
      setDepMessage("Deployment identity saved.");
    } catch (err) {
      setDepMessage(err instanceof Error ? err.message : String(err));
    } finally {
      setDepSaving(false);
    }
  }

  async function handleRegisterDeployment() {
    setDepRegistering(true);
    setDepMessage(null);
    try {
      await invoke("register_deployment");
      setDepMessage("Registered with cloud successfully.");
    } catch (err) {
      setDepMessage(err instanceof Error ? err.message : String(err));
    } finally {
      setDepRegistering(false);
    }
  }
  // ─────────────────────────────────────────────────────────────────────────

  const [settings, setSettings] = useState<AppSettings>({ ...DEFAULT_SETTINGS, theme });
  const [loaded, setLoaded] = useState<AppSettings | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [message, setMessage] = useState<string | null>(null);

  useEffect(() => {
    void (async () => {
      if (!accessToken) return;
      setLoading(true);
      try {
        const persisted = await tauriApi.getSettings(accessToken);
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
    if (!accessToken) return;
    setLoading(true);
    setError(null);
    setMessage(null);
    try {
      await tauriApi.saveSettings(accessToken, settings);
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
      <PageSection title="Setup / Onboarding" description="Re-run the guided first-run setup, or jump straight to any step.">
        <div className="flex flex-wrap items-center gap-2">
          <Button onClick={() => openOnboarding(0)}>
            <Compass className="mr-2 h-4 w-4" /> Launch guided setup
          </Button>
          {Array.from({ length: STEP_COUNT }).map((_, index) => (
            <Button key={index} variant="outline" size="sm" onClick={() => openOnboarding(index)}>
              {index + 1}. {ONBOARDING_STEP_LABELS[index] ?? `Step ${index + 1}`}
            </Button>
          ))}
        </div>
      </PageSection>

      <PageSection
        title="Deployment Identity"
        description="Configure which hospital site or facility this instance represents. Saved here and registered with the cloud so enterprise dashboards can list all active sites."
      >
        <Accordion type="multiple" defaultValue={["deployment-id"]}>
          <AccordionItem value="deployment-id" className="border rounded-lg px-4 mb-4 bg-secondary/10">
            <AccordionTrigger className="hover:no-underline">
              <div className="flex items-center gap-3">
                <MapPin className="h-4 w-4 text-primary" />
                <div className="text-left">
                  <p className="text-sm font-semibold uppercase tracking-tight">Site Identity</p>
                  <p className="text-xs text-muted-foreground font-normal">Name, location, and coordinates for this deployment.</p>
                </div>
              </div>
            </AccordionTrigger>
            <AccordionContent className="pt-4 pb-6">
              <div className="grid gap-4 sm:grid-cols-2">
                <div className="space-y-2">
                  <Label htmlFor="dep-settings-name" className="text-[10px] font-bold uppercase tracking-wider">Deployment Name</Label>
                  <Input
                    id="dep-settings-name"
                    value={depName}
                    onChange={(e) => setDepName(e.target.value)}
                    placeholder="St. Mary's Hospital — ICU"
                  />
                </div>
                <div className="space-y-2">
                  <Label htmlFor="dep-settings-location" className="text-[10px] font-bold uppercase tracking-wider">Location</Label>
                  <Input
                    id="dep-settings-location"
                    value={depLocation}
                    onChange={(e) => setDepLocation(e.target.value)}
                    placeholder="Chicago, IL"
                  />
                </div>
                <div className="space-y-2">
                  <Label htmlFor="dep-settings-lat" className="text-[10px] font-bold uppercase tracking-wider">Latitude (optional)</Label>
                  <Input
                    id="dep-settings-lat"
                    type="number"
                    value={depLat}
                    onChange={(e) => setDepLat(e.target.value)}
                    placeholder="41.8781"
                  />
                </div>
                <div className="space-y-2">
                  <Label htmlFor="dep-settings-lng" className="text-[10px] font-bold uppercase tracking-wider">Longitude (optional)</Label>
                  <Input
                    id="dep-settings-lng"
                    type="number"
                    value={depLng}
                    onChange={(e) => setDepLng(e.target.value)}
                    placeholder="-87.6298"
                  />
                </div>
                <div className="flex gap-2 sm:col-span-2">
                  <Button
                    disabled={depSaving}
                    onClick={handleSaveDeployment}
                    className="flex-1 text-xs uppercase font-bold tracking-widest"
                  >
                    {depSaving ? "Saving…" : "Save Identity"}
                  </Button>
                  <Button
                    disabled={depRegistering}
                    variant="outline"
                    onClick={handleRegisterDeployment}
                    className="flex-1 text-xs uppercase font-bold tracking-widest"
                  >
                    {depRegistering ? "Registering…" : "Register with Cloud"}
                  </Button>
                </div>
                {depMessage && (
                  <p className="sm:col-span-2 rounded-md border border-emerald-500/40 bg-emerald-500/10 px-3 py-2 text-sm text-emerald-400">
                    {depMessage}
                  </p>
                )}
              </div>
            </AccordionContent>
          </AccordionItem>
        </Accordion>
      </PageSection>

      <PageSection title="Enterprise Infrastructure" description="Manage multi-tenant settings and communication integrations.">
        <Accordion type="multiple" defaultValue={["whatsapp", "runtime"]}>
          <AccordionItem value="protocols" className="border rounded-lg px-4 mb-4 bg-secondary/10">
            <AccordionTrigger className="hover:no-underline">
              <div className="flex items-center gap-3">
                <div className="text-left">
                  <p className="text-sm font-semibold uppercase tracking-tight">Medical & Security Protocols</p>
                  <p className="text-xs text-muted-foreground font-normal">Configure emergency thresholds and automated alerting logic.</p>
                </div>
              </div>
            </AccordionTrigger>
            <AccordionContent className="pt-4 pb-6 space-y-6">
              <div className="grid gap-6 md:grid-cols-2">
                <div className="space-y-4">
                  <h4 className="text-[10px] font-bold uppercase tracking-wider text-muted-foreground">Alert Toggles</h4>
                  <div className="flex items-center justify-between p-3 bg-background/50 rounded-lg border">
                    <Label className="text-sm uppercase tracking-tighter font-semibold">Fall Detection Alerts</Label>
                    <Switch 
                      checked={localEnt?.fall_alert_enabled} 
                      onCheckedChange={(val) => setLocalEnt(s => s ? {...s, fall_alert_enabled: val} : null)}
                    />
                  </div>
                  <div className="flex items-center justify-between p-3 bg-background/50 rounded-lg border">
                    <Label className="text-sm uppercase tracking-tighter font-semibold">Vitals Monitoring Alerts</Label>
                    <Switch 
                      checked={localEnt?.vitals_alert_enabled} 
                      onCheckedChange={(val) => setLocalEnt(s => s ? {...s, vitals_alert_enabled: val} : null)}
                    />
                  </div>
                </div>

                <div className="space-y-4">
                  <h4 className="text-[10px] font-bold uppercase tracking-wider text-muted-foreground">Thresholds</h4>
                  <div className="grid grid-cols-2 gap-4">
                    <div className="space-y-2">
                      <Label className="text-[9px] uppercase font-bold text-muted-foreground">Min HR (BPM)</Label>
                      <Input 
                        type="number" 
                        value={localEnt?.hr_min || 0} 
                        onChange={(e) => setLocalEnt(s => s ? {...s, hr_min: Number(e.target.value)} : null)}
                      />
                    </div>
                    <div className="space-y-2">
                      <Label className="text-[9px] uppercase font-bold text-muted-foreground">Max HR (BPM)</Label>
                      <Input 
                        type="number" 
                        value={localEnt?.hr_max || 0} 
                        onChange={(e) => setLocalEnt(s => s ? {...s, hr_max: Number(e.target.value)} : null)}
                      />
                    </div>
                    <div className="space-y-2">
                      <Label className="text-[9px] uppercase font-bold text-muted-foreground">Min Resp (RPM)</Label>
                      <Input 
                        type="number" 
                        value={localEnt?.br_min || 0} 
                        onChange={(e) => setLocalEnt(s => s ? {...s, br_min: Number(e.target.value)} : null)}
                      />
                    </div>
                    <div className="space-y-2">
                      <Label className="text-[9px] uppercase font-bold text-muted-foreground">Max Resp (RPM)</Label>
                      <Input 
                        type="number" 
                        value={localEnt?.br_max || 0} 
                        onChange={(e) => setLocalEnt(s => s ? {...s, br_max: Number(e.target.value)} : null)}
                      />
                    </div>
                    <div className="space-y-2 col-span-2">
                      <Label className="text-[9px] uppercase font-bold text-muted-foreground">Crowd Capacity Limit</Label>
                      <Input 
                        type="number" 
                        value={localEnt?.crowd_threshold || 0} 
                        onChange={(e) => setLocalEnt(s => s ? {...s, crowd_threshold: Number(e.target.value)} : null)}
                      />
                    </div>
                  </div>
                </div>
              </div>
              <Button onClick={handleSaveEnt} disabled={entLoading} className="w-full uppercase font-bold text-xs tracking-widest">
                Apply Protocols
              </Button>
            </AccordionContent>
          </AccordionItem>

          <AccordionItem value="whatsapp" className="border rounded-lg px-4 mb-4 bg-secondary/10">
            <AccordionTrigger className="hover:no-underline">
              <div className="flex items-center gap-3">
                <div className="text-left">
                  <p className="text-sm font-semibold uppercase tracking-tight">WhatsApp (Whapi) Integration</p>
                  <p className="text-xs text-muted-foreground font-normal">Connect your phone to dispatch emergency alerts.</p>
                </div>
              </div>
            </AccordionTrigger>
            <AccordionContent className="pt-4 pb-6 space-y-6">
              <div className="grid gap-6 md:grid-cols-2">
                <div className="space-y-2">
                  <Label className="uppercase text-[10px] font-bold text-muted-foreground">Whapi API Token</Label>
                  <Input 
                    type="password" 
                    placeholder="ENTER BEARER TOKEN"
                    value={localEnt?.whapi_token || ""} 
                    onChange={(e) => setLocalEnt(s => s ? {...s, whapi_token: e.target.value} : null)}
                  />
                </div>
                <div className="space-y-2">
                  <Label className="uppercase text-[10px] font-bold text-muted-foreground">Emergency Number</Label>
                  <Input 
                    placeholder="E.G. 256700000000" 
                    value={localEnt?.alert_target_number || ""} 
                    onChange={(e) => setLocalEnt(s => s ? {...s, alert_target_number: e.target.value} : null)}
                  />
                </div>
              </div>

              <div className="flex flex-wrap gap-3">
                <Button onClick={handleSaveEnt} disabled={entLoading} className="uppercase font-bold text-xs tracking-tighter px-6">
                  Persist Configuration
                </Button>
                <Button variant="secondary" onClick={handleGetQR} disabled={entLoading || qrLoading} className="uppercase font-bold text-xs tracking-tighter px-6">
                  {qrLoading ? "Generating QR..." : "Pair Device (QR)"}
                </Button>
                <Button variant="outline" onClick={() => sendTestMessage()} disabled={entLoading} className="uppercase font-bold text-xs tracking-tighter px-6">
                  Send Test Alert
                </Button>
              </div>

              {qrLoading && (
                <div className="mt-4 p-8 bg-secondary/20 rounded-xl border-2 border-dashed border-secondary flex flex-col items-center justify-center animate-pulse w-full max-w-[300px] mx-auto">
                  <div className="w-48 h-48 bg-secondary/40 rounded-lg mb-4" />
                  <p className="text-[10px] text-muted-foreground font-bold uppercase tracking-widest">Generating Secure QR...</p>
                </div>
              )}

              {qrCode && !qrLoading && (
                <div className="mt-4 p-4 bg-white rounded-xl border-4 border-secondary shadow-inner inline-flex flex-col items-center mx-auto w-full max-w-[300px]">
                  <img src={qrCode} alt="WhatsApp QR" className="w-full h-auto" />
                  <p className="text-[10px] text-black font-bold uppercase mt-2 tracking-widest">Scan with WhatsApp</p>
                </div>
              )}
            </AccordionContent>
          </AccordionItem>
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
                  <Input
                    id="set-pi-aggregator"
                    value={settings.pi_agent_aggregator}
                    onChange={(e) => setSettings((prev) => ({ ...prev, pi_agent_aggregator: e.target.value }))}
                    placeholder="this PC's LAN IP:5005 (e.g. 192.168.1.20:5005)"
                    required
                  />
                  {settings.pi_agent_aggregator && isLoopbackHostPort(settings.pi_agent_aggregator) ? (
                    <p className="text-xs text-amber-500">
                      Loopback address: a remote Pi would stream CSI to its own loopback, not to this PC. Use this PC&apos;s LAN IP (e.g. 192.168.1.20:5005).
                    </p>
                  ) : null}
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
          <AccordionItem value="telemetry">
            <AccordionTrigger>Telemetry and Tracing</AccordionTrigger>
            <AccordionContent>
              <div className="grid gap-4 md:grid-cols-2">
                <div className="flex items-center gap-3 rounded-md border border-border/60 p-3 md:col-span-2">
                  <Switch
                    id="set-langfuse-enabled"
                    checked={langfuse.enabled}
                    onCheckedChange={(checked) => setLangfuse((prev) => ({ ...prev, enabled: checked }))}
                  />
                  <Label htmlFor="set-langfuse-enabled">Enable Langfuse Tracing</Label>
                </div>
                <div className="space-y-2">
                  <Label htmlFor="set-langfuse-host">Langfuse Host</Label>
                  <Input
                    id="set-langfuse-host"
                    value={langfuse.host}
                    onChange={(e) => setLangfuse((prev) => ({ ...prev, host: e.target.value }))}
                    placeholder="https://cloud.langfuse.com"
                  />
                </div>
                <div className="space-y-2">
                  <Label htmlFor="set-langfuse-pk">Public Key{langfuseHint ? ` (current: ${langfuseHint})` : ""}</Label>
                  <Input
                    id="set-langfuse-pk"
                    value={langfuse.public_key}
                    onChange={(e) => setLangfuse((prev) => ({ ...prev, public_key: e.target.value }))}
                    placeholder="pk-lf-..."
                  />
                </div>
                <div className="space-y-2 md:col-span-2">
                  <Label htmlFor="set-langfuse-sk">Secret Key</Label>
                  <Input
                    id="set-langfuse-sk"
                    type="password"
                    value={langfuse.secret_key}
                    onChange={(e) => setLangfuse((prev) => ({ ...prev, secret_key: e.target.value }))}
                    placeholder="sk-lf-..."
                  />
                </div>
                <div className="md:col-span-2 flex flex-col gap-2">
                  <Button disabled={langfuseSaving} onClick={handleSaveLangfuse} className="w-full uppercase font-bold text-xs tracking-widest">
                    {langfuseSaving ? "Saving..." : "Save Langfuse Config"}
                  </Button>
                  {langfuseMessage && (
                    <p className="rounded-md border border-emerald-500/40 bg-emerald-500/10 px-3 py-2 text-sm text-emerald-400">
                      {langfuseMessage}
                    </p>
                  )}
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
      {entError ? <p className="rounded-md border border-amber-500/40 bg-amber-500/10 px-3 py-2 text-sm text-amber-500 uppercase font-mono tracking-tighter">Enterprise Error: {entError}</p> : null}
    </div>
  );
}
