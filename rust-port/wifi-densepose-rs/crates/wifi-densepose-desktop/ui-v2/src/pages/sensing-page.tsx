import { useState } from "react";
import { RefreshCw } from "lucide-react";
import { tauriApi } from "@/lib/tauri-api";
import { PageSection } from "@/components/layout/page-section";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Badge } from "@/components/ui/badge";
import { JsonViewer } from "@/components/layout/json-viewer";
import type { ServerConfig, ServerLogsResponse, ServerStartResult, ServerStatusResponse } from "@/types";

interface SensingPageProps {
  status: ServerStatusResponse | null;
  onStatusRefresh: () => Promise<void>;
}

export function SensingPage({ status, onStatusRefresh }: SensingPageProps) {
  const [config, setConfig] = useState<ServerConfig>({
    http_port: 8080,
    ws_port: 8765,
    udp_port: 5005,
    bind_address: "127.0.0.1",
    log_level: "info",
    source: "auto",
  });
  const [logs, setLogs] = useState<ServerLogsResponse | null>(null);
  const [result, setResult] = useState<ServerStartResult | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

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

  return (
    <div className="space-y-6">
      <PageSection title="Server Runtime" description="Start, stop, and restart sensing service with explicit transport configuration.">
        <div className="grid gap-4 md:grid-cols-3">
          <div className="space-y-2">
            <Label htmlFor="server-http">HTTP Port</Label>
            <Input
              id="server-http"
              value={String(config.http_port ?? "")}
              onChange={(e) => setConfig((prev) => ({ ...prev, http_port: Number(e.target.value) }))}
            />
          </div>
          <div className="space-y-2">
            <Label htmlFor="server-ws">WS Port</Label>
            <Input
              id="server-ws"
              value={String(config.ws_port ?? "")}
              onChange={(e) => setConfig((prev) => ({ ...prev, ws_port: Number(e.target.value) }))}
            />
          </div>
          <div className="space-y-2">
            <Label htmlFor="server-udp">UDP Port</Label>
            <Input
              id="server-udp"
              value={String(config.udp_port ?? "")}
              onChange={(e) => setConfig((prev) => ({ ...prev, udp_port: Number(e.target.value) }))}
            />
          </div>
          <div className="space-y-2">
            <Label htmlFor="server-bind">Bind Address</Label>
            <Input id="server-bind" value={config.bind_address ?? ""} onChange={(e) => setConfig((prev) => ({ ...prev, bind_address: e.target.value }))} />
          </div>
          <div className="space-y-2">
            <Label htmlFor="server-level">Log Level</Label>
            <Input id="server-level" value={config.log_level ?? ""} onChange={(e) => setConfig((prev) => ({ ...prev, log_level: e.target.value }))} />
          </div>
          <div className="space-y-2">
            <Label htmlFor="server-source">Data Source</Label>
            <Input id="server-source" value={config.source ?? ""} onChange={(e) => setConfig((prev) => ({ ...prev, source: e.target.value }))} placeholder="auto | wifi | esp32" />
          </div>
        </div>
        <div className="mt-4 flex flex-wrap gap-2">
          <Button disabled={loading} onClick={() => withBusy(async () => setResult(await tauriApi.startServer(config)))}>
            Start Server
          </Button>
          <Button disabled={loading} variant="secondary" onClick={() => withBusy(async () => await tauriApi.stopServer())}>
            Stop Server
          </Button>
          <Button disabled={loading} variant="outline" onClick={() => withBusy(async () => setResult(await tauriApi.restartServer(config)))}>
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
          <Badge variant="outline">PID: {status?.pid ?? "N/A"}</Badge>
          <Badge variant="outline">CPU: {status?.cpu_percent?.toFixed(1) ?? "N/A"}%</Badge>
          <Badge variant="outline">RAM: {status?.memory_mb?.toFixed(1) ?? "N/A"} MB</Badge>
          <Badge variant="outline">Uptime: {status?.uptime_secs ?? 0}s</Badge>
        </div>
        <JsonViewer value={status ?? { running: false }} />
      </PageSection>

      <PageSection title="Server Logs" description="Retrieve current command-level server logs.">
        <div className="mb-3">
          <Button
            disabled={loading}
            onClick={() =>
              withBusy(async () => {
                const response = await tauriApi.serverLogs(200);
                setLogs(response);
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

