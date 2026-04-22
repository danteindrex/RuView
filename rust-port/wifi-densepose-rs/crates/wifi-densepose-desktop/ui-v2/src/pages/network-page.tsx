import { useMemo, useState } from "react";
import { Loader2, Wifi } from "lucide-react";
import { tauriApi } from "@/lib/tauri-api";
import { PageSection } from "@/components/layout/page-section";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Badge } from "@/components/ui/badge";
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "@/components/ui/table";
import type { DiscoveredNode, SerialPortInfo } from "@/types";

interface NetworkPageProps {
  nodes: DiscoveredNode[];
  onNodesUpdate: (nodes: DiscoveredNode[]) => void;
}

export function NetworkPage({ nodes, onNodesUpdate }: NetworkPageProps) {
  const [loading, setLoading] = useState(false);
  const [ports, setPorts] = useState<SerialPortInfo[]>([]);
  const [timeoutMs, setTimeoutMs] = useState("3000");
  const [wifiPort, setWifiPort] = useState("");
  const [ssid, setSsid] = useState("");
  const [password, setPassword] = useState("");
  const [message, setMessage] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  const nodeSummary = useMemo(() => {
    const online = nodes.filter((n) => n.health === "online").length;
    return `${online}/${nodes.length} online`;
  }, [nodes]);

  async function handleDiscover() {
    setLoading(true);
    setError(null);
    setMessage(null);
    try {
      const discovered = await tauriApi.discoverNodes(Number(timeoutMs || "3000"));
      onNodesUpdate(discovered);
      setMessage(`Discovery completed: ${discovered.length} nodes found.`);
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setLoading(false);
    }
  }

  async function handleLoadPorts() {
    setLoading(true);
    setError(null);
    setMessage(null);
    try {
      const found = await tauriApi.listSerialPorts();
      setPorts(found);
      if (!wifiPort && found.length > 0) {
        setWifiPort(found[0].name);
      }
      setMessage(`Loaded ${found.length} serial ports.`);
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setLoading(false);
    }
  }

  async function handleConfigureWifi() {
    setLoading(true);
    setError(null);
    setMessage(null);
    try {
      const response = await tauriApi.configureEsp32Wifi(wifiPort, ssid, password);
      setMessage(response);
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setLoading(false);
    }
  }

  return (
    <div className="space-y-6">
      <PageSection title="Discovery Control" description="Discover ESP32 nodes and inspect serial devices for provisioning readiness.">
        <div className="grid gap-4 md:grid-cols-3">
          <div className="space-y-2">
            <Label htmlFor="discovery-timeout">Timeout (ms)</Label>
            <Input id="discovery-timeout" value={timeoutMs} onChange={(e) => setTimeoutMs(e.target.value)} />
          </div>
          <div className="flex items-end gap-2">
            <Button disabled={loading} onClick={handleDiscover}>
              {loading ? <Loader2 className="mr-2 h-4 w-4 animate-spin" /> : null}
              Discover Nodes
            </Button>
            <Button disabled={loading} variant="secondary" onClick={handleLoadPorts}>
              Load Serial Ports
            </Button>
          </div>
          <div className="flex items-end">
            <Badge variant="outline">{nodeSummary}</Badge>
          </div>
        </div>
      </PageSection>

      <PageSection title="Node Registry" description="Current nodes in scope with health, firmware, and capability indicators.">
        <Table>
          <TableHeader>
            <TableRow>
              <TableHead>IP</TableHead>
              <TableHead>Host</TableHead>
              <TableHead>Health</TableHead>
              <TableHead>Chip</TableHead>
              <TableHead>Role</TableHead>
              <TableHead>Firmware</TableHead>
            </TableRow>
          </TableHeader>
          <TableBody>
            {nodes.map((node) => (
              <TableRow key={`${node.ip}-${node.mac ?? "macless"}`}>
                <TableCell>{node.ip}</TableCell>
                <TableCell>{node.hostname ?? "N/A"}</TableCell>
                <TableCell>
                  <Badge variant={node.health === "online" ? "success" : node.health === "degraded" ? "warning" : "danger"}>
                    {node.health}
                  </Badge>
                </TableCell>
                <TableCell>{node.chip}</TableCell>
                <TableCell>{node.mesh_role}</TableCell>
                <TableCell>{node.firmware_version ?? "unknown"}</TableCell>
              </TableRow>
            ))}
            {nodes.length === 0 ? (
              <TableRow>
                <TableCell colSpan={6} className="text-center text-muted-foreground">
                  No nodes discovered yet.
                </TableCell>
              </TableRow>
            ) : null}
          </TableBody>
        </Table>
      </PageSection>

      <PageSection title="WiFi Provisioning Shortcut" description="Push WiFi credentials to a selected serial device using firmware provisioning command.">
        <div className="grid gap-4 md:grid-cols-2">
          <div className="space-y-2">
            <Label htmlFor="wifi-port">Serial Port</Label>
            <Input id="wifi-port" value={wifiPort} onChange={(e) => setWifiPort(e.target.value)} placeholder="COM3 or /dev/ttyUSB0" />
          </div>
          <div className="space-y-2">
            <Label htmlFor="wifi-ssid">WiFi SSID</Label>
            <Input id="wifi-ssid" value={ssid} onChange={(e) => setSsid(e.target.value)} />
          </div>
          <div className="space-y-2 md:col-span-2">
            <Label htmlFor="wifi-password">WiFi Password</Label>
            <Input id="wifi-password" value={password} type="password" onChange={(e) => setPassword(e.target.value)} />
          </div>
        </div>
        <div className="mt-4 flex flex-wrap gap-2">
          <Button disabled={loading || !wifiPort || !ssid} onClick={handleConfigureWifi}>
            <Wifi className="mr-2 h-4 w-4" />
            Configure WiFi
          </Button>
        </div>
      </PageSection>

      {ports.length > 0 ? (
        <PageSection title="Serial Ports" description="Enumerated ports with ESP32 compatibility detection.">
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead>Name</TableHead>
                <TableHead>Manufacturer</TableHead>
                <TableHead>VID</TableHead>
                <TableHead>PID</TableHead>
                <TableHead>Compatible</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {ports.map((port) => (
                <TableRow key={port.name}>
                  <TableCell>{port.name}</TableCell>
                  <TableCell>{port.manufacturer ?? "unknown"}</TableCell>
                  <TableCell>{port.vid ?? "-"}</TableCell>
                  <TableCell>{port.pid ?? "-"}</TableCell>
                  <TableCell>
                    <Badge variant={port.is_esp32_compatible ? "success" : "outline"}>
                      {port.is_esp32_compatible ? "ESP32-ready" : "Unknown"}
                    </Badge>
                  </TableCell>
                </TableRow>
              ))}
            </TableBody>
          </Table>
        </PageSection>
      ) : null}

      {message ? <p className="rounded-md border border-emerald-500/40 bg-emerald-500/10 px-3 py-2 text-sm text-emerald-400">{message}</p> : null}
      {error ? <p className="rounded-md border border-destructive/40 bg-destructive/10 px-3 py-2 text-sm text-destructive">{error}</p> : null}
    </div>
  );
}

