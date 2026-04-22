import { useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { Loader2 } from "lucide-react";
import { tauriApi } from "@/lib/tauri-api";
import { PageSection } from "@/components/layout/page-section";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "@/components/ui/table";
import { JsonViewer } from "@/components/layout/json-viewer";
import type { WasmModuleDetail, WasmModuleInfo, WasmRuntimeStats, WasmSupportInfo, WasmUploadResult } from "@/types";

export function ModulesPage() {
  const [nodeIp, setNodeIp] = useState("");
  const [moduleId, setModuleId] = useState("");
  const [action, setAction] = useState("start");
  const [moduleName, setModuleName] = useState("");
  const [wasmPath, setWasmPath] = useState("");
  const [loading, setLoading] = useState(false);
  const [modules, setModules] = useState<WasmModuleInfo[]>([]);
  const [support, setSupport] = useState<WasmSupportInfo | null>(null);
  const [stats, setStats] = useState<WasmRuntimeStats | null>(null);
  const [details, setDetails] = useState<WasmModuleDetail | null>(null);
  const [uploadResult, setUploadResult] = useState<WasmUploadResult | null>(null);
  const [error, setError] = useState<string | null>(null);

  async function pickWasm() {
    const selected = await open({
      multiple: false,
      filters: [{ name: "WASM", extensions: ["wasm"] }],
    });
    if (typeof selected === "string") {
      setWasmPath(selected);
    }
  }

  async function loadRuntime() {
    setLoading(true);
    setError(null);
    try {
      const [moduleList, runtimeStats, supportInfo] = await Promise.all([
        tauriApi.wasmList(nodeIp),
        tauriApi.wasmStats(nodeIp),
        tauriApi.checkWasmSupport(nodeIp),
      ]);
      setModules(moduleList);
      setStats(runtimeStats);
      setSupport(supportInfo);
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setLoading(false);
    }
  }

  async function handleUpload() {
    setLoading(true);
    setError(null);
    setUploadResult(null);
    try {
      const result = await tauriApi.wasmUpload({
        nodeIp,
        wasmPath,
        moduleName: moduleName || undefined,
        autoStart: true,
      });
      setUploadResult(result);
      await loadRuntime();
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setLoading(false);
    }
  }

  async function handleControl() {
    setLoading(true);
    setError(null);
    try {
      await tauriApi.wasmControl({ nodeIp, moduleId, action });
      await loadRuntime();
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setLoading(false);
    }
  }

  async function handleDetails() {
    setLoading(true);
    setError(null);
    setDetails(null);
    try {
      const info = await tauriApi.wasmInfo({ nodeIp, moduleId });
      setDetails(info);
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setLoading(false);
    }
  }

  return (
    <div className="space-y-6">
      <PageSection title="Runtime Connection" description="Query WASM support, loaded modules, and runtime counters for the selected node.">
        <div className="grid gap-4 md:grid-cols-[1fr_auto]">
          <div className="space-y-2">
            <Label htmlFor="wasm-node-ip">Node IP</Label>
            <Input id="wasm-node-ip" value={nodeIp} onChange={(e) => setNodeIp(e.target.value)} />
          </div>
          <div className="flex items-end">
            <Button disabled={loading || !nodeIp} onClick={loadRuntime}>
              {loading ? <Loader2 className="mr-2 h-4 w-4 animate-spin" /> : null}
              Refresh Runtime
            </Button>
          </div>
        </div>
      </PageSection>

      <PageSection title="Upload Module" description="Upload WebAssembly module package to target node runtime.">
        <div className="grid gap-4 md:grid-cols-2">
          <div className="space-y-2">
            <Label htmlFor="module-name">Module Name (optional)</Label>
            <Input id="module-name" value={moduleName} onChange={(e) => setModuleName(e.target.value)} />
          </div>
          <div className="space-y-2">
            <Label htmlFor="wasm-path">WASM Path</Label>
            <div className="flex gap-2">
              <Input id="wasm-path" value={wasmPath} onChange={(e) => setWasmPath(e.target.value)} />
              <Button variant="outline" onClick={pickWasm}>
                Browse
              </Button>
            </div>
          </div>
        </div>
        <div className="mt-4">
          <Button disabled={loading || !nodeIp || !wasmPath} onClick={handleUpload}>
            Upload Module
          </Button>
        </div>
      </PageSection>

      <PageSection title="Module Lifecycle Control" description="Apply lifecycle operations to existing module IDs.">
        <div className="grid gap-4 md:grid-cols-3">
          <div className="space-y-2">
            <Label htmlFor="module-id">Module ID</Label>
            <Input id="module-id" value={moduleId} onChange={(e) => setModuleId(e.target.value)} />
          </div>
          <div className="space-y-2">
            <Label htmlFor="module-action">Action</Label>
            <Select value={action} onValueChange={setAction}>
              <SelectTrigger id="module-action">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="start">Start</SelectItem>
                <SelectItem value="stop">Stop</SelectItem>
                <SelectItem value="restart">Restart</SelectItem>
                <SelectItem value="unload">Unload</SelectItem>
              </SelectContent>
            </Select>
          </div>
          <div className="flex items-end gap-2">
            <Button disabled={loading || !nodeIp || !moduleId} onClick={handleControl}>
              Apply
            </Button>
            <Button disabled={loading || !nodeIp || !moduleId} variant="secondary" onClick={handleDetails}>
              Get Info
            </Button>
          </div>
        </div>
      </PageSection>

      {modules.length > 0 ? (
        <PageSection title="Loaded Modules" description="Runtime module inventory and resource usage.">
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead>ID</TableHead>
                <TableHead>Name</TableHead>
                <TableHead>Status</TableHead>
                <TableHead>Memory (KB)</TableHead>
                <TableHead>CPU %</TableHead>
                <TableHead>Exec Count</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {modules.map((module) => (
                <TableRow key={module.id}>
                  <TableCell>{module.id}</TableCell>
                  <TableCell>{module.name}</TableCell>
                  <TableCell>{module.status}</TableCell>
                  <TableCell>{module.memory_used_kb ?? "-"}</TableCell>
                  <TableCell>{module.cpu_usage_pct ?? "-"}</TableCell>
                  <TableCell>{module.exec_count ?? "-"}</TableCell>
                </TableRow>
              ))}
            </TableBody>
          </Table>
        </PageSection>
      ) : null}

      {support ? (
        <PageSection title="Support Profile" description="Capabilities advertised by node runtime.">
          <JsonViewer value={support} />
        </PageSection>
      ) : null}

      {stats ? (
        <PageSection title="Runtime Metrics" description="Aggregate counters from module runtime subsystem.">
          <JsonViewer value={stats} />
        </PageSection>
      ) : null}

      {uploadResult ? (
        <PageSection title="Upload Result" description="Result payload from last module upload.">
          <JsonViewer value={uploadResult} />
        </PageSection>
      ) : null}

      {details ? (
        <PageSection title="Module Detail" description="Resolved module metadata and export/import information.">
          <JsonViewer value={details} />
        </PageSection>
      ) : null}

      {error ? <p className="rounded-md border border-destructive/40 bg-destructive/10 px-3 py-2 text-sm text-destructive">{error}</p> : null}
    </div>
  );
}

