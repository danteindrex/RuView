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

export function SettingsPage({ theme, onThemeChange }: SettingsPageProps) {
  const [settings, setSettings] = useState<AppSettings>({
    server_http_port: 8080,
    server_ws_port: 8765,
    server_udp_port: 5005,
    bind_address: "127.0.0.1",
    ui_path: "",
    ota_psk: "",
    auto_discover: true,
    discover_interval_ms: 10000,
    theme,
  });
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
          setSettings(persisted);
          setLoaded(persisted);
        }
      } catch (err) {
        setError(err instanceof Error ? err.message : String(err));
      } finally {
        setLoading(false);
      }
    })();
  }, []);

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
        <Accordion type="multiple" defaultValue={["runtime", "discovery", "security"]}>
          <AccordionItem value="runtime">
            <AccordionTrigger>Runtime and Transport</AccordionTrigger>
            <AccordionContent>
              <div className="grid gap-4 md:grid-cols-2">
                <div className="space-y-2">
                  <Label htmlFor="set-http">HTTP Port</Label>
                  <Input id="set-http" value={String(settings.server_http_port)} onChange={(e) => setSettings((prev) => ({ ...prev, server_http_port: Number(e.target.value) }))} />
                </div>
                <div className="space-y-2">
                  <Label htmlFor="set-ws">WS Port</Label>
                  <Input id="set-ws" value={String(settings.server_ws_port)} onChange={(e) => setSettings((prev) => ({ ...prev, server_ws_port: Number(e.target.value) }))} />
                </div>
                <div className="space-y-2">
                  <Label htmlFor="set-udp">UDP Port</Label>
                  <Input id="set-udp" value={String(settings.server_udp_port)} onChange={(e) => setSettings((prev) => ({ ...prev, server_udp_port: Number(e.target.value) }))} />
                </div>
                <div className="space-y-2">
                  <Label htmlFor="set-bind">Bind Address</Label>
                  <Input id="set-bind" value={settings.bind_address} onChange={(e) => setSettings((prev) => ({ ...prev, bind_address: e.target.value }))} />
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
                    onChange={(e) => setSettings((prev) => ({ ...prev, discover_interval_ms: Number(e.target.value) }))}
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
            <AccordionTrigger>Security and Paths</AccordionTrigger>
            <AccordionContent>
              <div className="grid gap-4 md:grid-cols-2">
                <div className="space-y-2">
                  <Label htmlFor="set-ota-psk">OTA PSK</Label>
                  <Input id="set-ota-psk" value={settings.ota_psk} onChange={(e) => setSettings((prev) => ({ ...prev, ota_psk: e.target.value }))} />
                </div>
                <div className="space-y-2">
                  <Label htmlFor="set-ui-path">UI Path</Label>
                  <Input id="set-ui-path" value={settings.ui_path} onChange={(e) => setSettings((prev) => ({ ...prev, ui_path: e.target.value }))} />
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

