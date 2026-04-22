import { useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { Loader2 } from "lucide-react";
import { tauriApi } from "@/lib/tauri-api";
import { PageSection } from "@/components/layout/page-section";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import { JsonViewer } from "@/components/layout/json-viewer";
import type { BatchOtaResult, OtaEndpointInfo, OtaResult } from "@/types";

export function OtaPage() {
  const [nodeIp, setNodeIp] = useState("");
  const [nodeIps, setNodeIps] = useState("");
  const [firmwarePath, setFirmwarePath] = useState("");
  const [psk, setPsk] = useState("");
  const [strategy, setStrategy] = useState("sequential");
  const [maxConcurrent, setMaxConcurrent] = useState("2");
  const [loading, setLoading] = useState(false);
  const [singleResult, setSingleResult] = useState<OtaResult | null>(null);
  const [batchResult, setBatchResult] = useState<BatchOtaResult | null>(null);
  const [endpointResult, setEndpointResult] = useState<OtaEndpointInfo | null>(null);
  const [error, setError] = useState<string | null>(null);

  async function pickFirmware() {
    const selected = await open({
      multiple: false,
      filters: [{ name: "Firmware", extensions: ["bin"] }],
    });
    if (typeof selected === "string") {
      setFirmwarePath(selected);
    }
  }

  async function handleSingleUpdate() {
    setLoading(true);
    setError(null);
    setSingleResult(null);
    try {
      const result = await tauriApi.otaUpdate({ nodeIp, firmwarePath, psk: psk || undefined });
      setSingleResult(result);
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setLoading(false);
    }
  }

  async function handleBatchUpdate() {
    setLoading(true);
    setError(null);
    setBatchResult(null);
    try {
      const parsed = nodeIps
        .split(",")
        .map((ip) => ip.trim())
        .filter(Boolean);
      const result = await tauriApi.batchOtaUpdate({
        nodeIps: parsed,
        firmwarePath,
        psk: psk || undefined,
        strategy,
        maxConcurrent: Number(maxConcurrent || "1"),
      });
      setBatchResult(result);
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setLoading(false);
    }
  }

  async function handleEndpointCheck() {
    setLoading(true);
    setError(null);
    setEndpointResult(null);
    try {
      const result = await tauriApi.checkOtaEndpoint(nodeIp);
      setEndpointResult(result);
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setLoading(false);
    }
  }

  return (
    <div className="space-y-6">
      <PageSection title="Single Node OTA" description="Push firmware to one node over OTA endpoint with optional PSK signature.">
        <div className="grid gap-4 md:grid-cols-2">
          <div className="space-y-2">
            <Label htmlFor="ota-node-ip">Node IP</Label>
            <Input id="ota-node-ip" value={nodeIp} onChange={(e) => setNodeIp(e.target.value)} />
          </div>
          <div className="space-y-2">
            <Label htmlFor="ota-psk">PSK (optional)</Label>
            <Input id="ota-psk" value={psk} type="password" onChange={(e) => setPsk(e.target.value)} />
          </div>
          <div className="space-y-2 md:col-span-2">
            <Label htmlFor="ota-firmware">Firmware Path</Label>
            <div className="flex gap-2">
              <Input id="ota-firmware" value={firmwarePath} onChange={(e) => setFirmwarePath(e.target.value)} />
              <Button variant="outline" onClick={pickFirmware}>
                Browse
              </Button>
            </div>
          </div>
        </div>
        <div className="mt-4 flex flex-wrap gap-2">
          <Button disabled={loading || !nodeIp || !firmwarePath} onClick={handleSingleUpdate}>
            {loading ? <Loader2 className="mr-2 h-4 w-4 animate-spin" /> : null}
            OTA Update
          </Button>
          <Button disabled={loading || !nodeIp} variant="secondary" onClick={handleEndpointCheck}>
            Check Endpoint
          </Button>
        </div>
      </PageSection>

      <PageSection title="Batch OTA" description="Run sequential or parallel updates across multiple node IPs.">
        <div className="grid gap-4 md:grid-cols-2">
          <div className="space-y-2 md:col-span-2">
            <Label htmlFor="ota-node-ips">Node IPs (comma-separated)</Label>
            <Input id="ota-node-ips" value={nodeIps} onChange={(e) => setNodeIps(e.target.value)} placeholder="192.168.1.101, 192.168.1.102" />
          </div>
          <div className="space-y-2">
            <Label htmlFor="ota-strategy">Strategy</Label>
            <Select value={strategy} onValueChange={setStrategy}>
              <SelectTrigger id="ota-strategy">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="sequential">Sequential</SelectItem>
                <SelectItem value="parallel">Parallel</SelectItem>
                <SelectItem value="tdm_safe">TDM Safe</SelectItem>
              </SelectContent>
            </Select>
          </div>
          <div className="space-y-2">
            <Label htmlFor="ota-max-concurrent">Max Concurrent</Label>
            <Input id="ota-max-concurrent" value={maxConcurrent} onChange={(e) => setMaxConcurrent(e.target.value)} />
          </div>
        </div>
        <div className="mt-4">
          <Button disabled={loading || !firmwarePath || !nodeIps} onClick={handleBatchUpdate}>
            {loading ? <Loader2 className="mr-2 h-4 w-4 animate-spin" /> : null}
            Run Batch OTA
          </Button>
        </div>
      </PageSection>

      {singleResult ? (
        <PageSection title="Single OTA Result" description="Response from single node OTA action.">
          <JsonViewer value={singleResult} />
        </PageSection>
      ) : null}

      {endpointResult ? (
        <PageSection title="Endpoint Health" description="Current OTA endpoint accessibility and compatibility.">
          <JsonViewer value={endpointResult} />
        </PageSection>
      ) : null}

      {batchResult ? (
        <PageSection title="Batch OTA Result" description="Aggregated outcomes across all target nodes.">
          <JsonViewer value={batchResult} />
        </PageSection>
      ) : null}

      {error ? <p className="rounded-md border border-destructive/40 bg-destructive/10 px-3 py-2 text-sm text-destructive">{error}</p> : null}
    </div>
  );
}

