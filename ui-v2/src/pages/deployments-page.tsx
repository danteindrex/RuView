import { useEffect, useState, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import { usePlanStore } from "@/lib/plan-store";
import { UpgradePrompt } from "@/components/upgrade-prompt";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { MapPin, Wifi, WifiOff, AlertTriangle, RefreshCw } from "lucide-react";

// ─── Types ───────────────────────────────────────────────────────────────────

interface DeploymentInfo {
  deployment_id: string;
  deployment_name: string;
  location_name: string;
  latitude?: number | null;
  longitude?: number | null;
  tenant_id?: string | null;
  last_seen?: string | null;
  node_count?: number | null;
  active_risk_level?: string | null;
  online: boolean;
}

interface DeploymentAggregate {
  total: number;
  online: number;
  offline: number;
  high_risk: number;
  avg_risk_score: number;
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

function timeAgo(iso: string | null | undefined): string {
  if (!iso) return "never";
  const diff = Math.floor((Date.now() - new Date(iso).getTime()) / 1000);
  if (diff < 60) return `${diff}s ago`;
  if (diff < 3600) return `${Math.floor(diff / 60)}m ago`;
  return `${Math.floor(diff / 3600)}h ago`;
}

const RISK_COLORS: Record<string, string> = {
  low: "bg-emerald-500/20 text-emerald-400 border-emerald-500/40",
  moderate: "bg-yellow-500/20 text-yellow-400 border-yellow-500/40",
  high: "bg-orange-500/20 text-orange-400 border-orange-500/40",
  critical: "bg-red-500/20 text-red-400 border-red-500/40",
};

// ─── Sub-components ───────────────────────────────────────────────────────────

function DeploymentCard({ dep }: { dep: DeploymentInfo }) {
  const risk = dep.active_risk_level ?? "low";
  const riskClass = RISK_COLORS[risk] ?? RISK_COLORS.low;

  return (
    <Card className="flex flex-col gap-0 border border-border/60">
      <CardHeader className="pb-2">
        <div className="flex items-start justify-between gap-2">
          <div>
            <CardTitle className="text-sm font-semibold leading-tight">{dep.deployment_name}</CardTitle>
            <p className="mt-0.5 flex items-center gap-1 text-xs text-muted-foreground">
              <MapPin className="h-3 w-3 shrink-0" />
              {dep.location_name}
            </p>
          </div>
          {dep.online ? (
            <span className="flex items-center gap-1 text-[10px] font-bold uppercase tracking-widest text-emerald-400">
              <Wifi className="h-3 w-3" /> Online
            </span>
          ) : (
            <span className="flex items-center gap-1 text-[10px] font-bold uppercase tracking-widest text-muted-foreground">
              <WifiOff className="h-3 w-3" /> Offline
            </span>
          )}
        </div>
      </CardHeader>
      <CardContent className="flex flex-col gap-3 pt-0">
        <div className="flex flex-wrap items-center gap-2">
          <span className="text-[10px] font-semibold uppercase tracking-wider text-muted-foreground">
            Nodes: <span className="text-foreground">{dep.node_count ?? "—"}</span>
          </span>
          <Badge
            variant="outline"
            className={`text-[10px] font-bold uppercase tracking-widest ${riskClass}`}
          >
            {risk === "high" || risk === "critical" ? (
              <AlertTriangle className="mr-1 h-3 w-3" />
            ) : null}
            {risk}
          </Badge>
        </div>
        <p className="text-[10px] text-muted-foreground">
          Last seen: {timeAgo(dep.last_seen)}
        </p>
        <div className="flex gap-2">
          <Button size="sm" variant="outline" className="flex-1 text-xs">
            View Details
          </Button>
          <Button size="sm" variant="outline" className="flex-1 text-xs">
            Configure
          </Button>
        </div>
      </CardContent>
    </Card>
  );
}

// ─── Main Page ────────────────────────────────────────────────────────────────

export function DeploymentsPage() {
  const { isEnterprise } = usePlanStore();

  const [thisDeployment, setThisDeployment] = useState<DeploymentInfo | null>(null);
  const [deployments, setDeployments] = useState<DeploymentInfo[]>([]);
  const [aggregate, setAggregate] = useState<DeploymentAggregate>({
    total: 0, online: 0, offline: 0, high_risk: 0, avg_risk_score: 0,
  });
  const [saving, setSaving] = useState(false);
  const [registering, setRegistering] = useState(false);
  const [toast, setToast] = useState<{ msg: string; ok: boolean } | null>(null);

  // Editable "this deployment" fields
  const [name, setName] = useState("");
  const [location, setLocation] = useState("");
  const [lat, setLat] = useState("");
  const [lng, setLng] = useState("");

  function showToast(msg: string, ok: boolean) {
    setToast({ msg, ok });
    setTimeout(() => setToast(null), 4000);
  }

  const loadData = useCallback(async () => {
    // Load this deployment's identity — Rust command added by sibling feature branch
    try {
      const info = await invoke<DeploymentInfo>("get_deployment_info");
      setThisDeployment(info);
      setName(info.deployment_name ?? "");
      setLocation(info.location_name ?? "");
      setLat(info.latitude != null ? String(info.latitude) : "");
      setLng(info.longitude != null ? String(info.longitude) : "");
    } catch {
      // Command not registered in this branch — degrade gracefully
    }

    // Derive tenant from this deployment, list siblings
    const tenantId = thisDeployment?.tenant_id ?? undefined;
    try {
      const list = await invoke<DeploymentInfo[]>("list_deployments", {
        tenantId: tenantId ?? null,
      });
      setDeployments(list);
    } catch {
      // Command not registered — show empty list
    }

    try {
      const agg = await invoke<DeploymentAggregate>("get_deployments_aggregate", {
        tenantId: tenantId ?? null,
      });
      setAggregate(agg);
    } catch {
      // Command not registered — keep zeroed aggregate
    }
  }, [thisDeployment?.tenant_id]);

  useEffect(() => {
    void loadData();
    const interval = setInterval(() => void loadData(), 30_000);
    return () => clearInterval(interval);
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  async function handleSave() {
    setSaving(true);
    try {
      await invoke("set_deployment_info", {
        deploymentName: name,
        locationName: location,
        latitude: lat !== "" ? parseFloat(lat) : null,
        longitude: lng !== "" ? parseFloat(lng) : null,
      });
      showToast("Deployment identity saved.", true);
      void loadData();
    } catch (err) {
      showToast(err instanceof Error ? err.message : String(err), false);
    } finally {
      setSaving(false);
    }
  }

  async function handleRegister() {
    setRegistering(true);
    try {
      await invoke("register_deployment");
      showToast("Deployment registered with cloud.", true);
      void loadData();
    } catch (err) {
      showToast(err instanceof Error ? err.message : String(err), false);
    } finally {
      setRegistering(false);
    }
  }

  // Enterprise gate
  if (!isEnterprise()) {
    return (
      <UpgradePrompt
        feature="Enterprise Deployments"
        requiredPlan="enterprise"
        className="mt-16"
      />
    );
  }

  return (
    <div className="space-y-6">
      {/* Toast */}
      {toast && (
        <div
          className={`rounded-md border px-4 py-2 text-sm font-medium ${
            toast.ok
              ? "border-emerald-500/40 bg-emerald-500/10 text-emerald-400"
              : "border-destructive/40 bg-destructive/10 text-destructive"
          }`}
        >
          {toast.msg}
        </div>
      )}

      {/* Summary bar */}
      <div className="grid grid-cols-2 gap-3 sm:grid-cols-4">
        {(
          [
            { label: "Total Deployments", value: aggregate.total, color: "text-foreground" },
            { label: "Online", value: aggregate.online, color: "text-emerald-400" },
            { label: "Offline", value: aggregate.offline, color: "text-muted-foreground" },
            { label: "High Risk", value: aggregate.high_risk, color: "text-red-400" },
          ] as const
        ).map(({ label, value, color }) => (
          <Card key={label} className="border border-border/60 py-3">
            <CardContent className="flex flex-col items-center gap-0.5 p-0">
              <span className={`text-2xl font-bold tabular-nums ${color}`}>{value}</span>
              <span className="text-[10px] font-bold uppercase tracking-widest text-muted-foreground">
                {label}
              </span>
            </CardContent>
          </Card>
        ))}
      </div>

      {/* This Deployment settings card */}
      <Card className="border border-border/60">
        <CardHeader className="pb-3">
          <CardTitle className="flex items-center gap-2 text-sm font-semibold uppercase tracking-tight">
            <MapPin className="h-4 w-4 text-primary" />
            This Deployment
            {thisDeployment?.deployment_id && (
              <span className="ml-auto font-mono text-[10px] font-normal text-muted-foreground">
                ID: {thisDeployment.deployment_id.slice(0, 8)}…
              </span>
            )}
          </CardTitle>
        </CardHeader>
        <CardContent className="grid gap-4 sm:grid-cols-2">
          <div className="space-y-2">
            <Label htmlFor="dep-name" className="text-[10px] font-bold uppercase tracking-wider">
              Deployment Name
            </Label>
            <Input
              id="dep-name"
              value={name}
              onChange={(e) => setName(e.target.value)}
              placeholder="St. Mary's Hospital — ICU"
            />
          </div>
          <div className="space-y-2">
            <Label htmlFor="dep-location" className="text-[10px] font-bold uppercase tracking-wider">
              Location
            </Label>
            <Input
              id="dep-location"
              value={location}
              onChange={(e) => setLocation(e.target.value)}
              placeholder="Chicago, IL"
            />
          </div>
          <div className="space-y-2">
            <Label htmlFor="dep-lat" className="text-[10px] font-bold uppercase tracking-wider">
              Latitude (optional)
            </Label>
            <Input
              id="dep-lat"
              type="number"
              value={lat}
              onChange={(e) => setLat(e.target.value)}
              placeholder="41.8781"
            />
          </div>
          <div className="space-y-2">
            <Label htmlFor="dep-lng" className="text-[10px] font-bold uppercase tracking-wider">
              Longitude (optional)
            </Label>
            <Input
              id="dep-lng"
              type="number"
              value={lng}
              onChange={(e) => setLng(e.target.value)}
              placeholder="-87.6298"
            />
          </div>
          <div className="flex gap-2 sm:col-span-2">
            <Button disabled={saving} onClick={handleSave} className="flex-1 text-xs uppercase font-bold tracking-widest">
              {saving ? "Saving…" : "Save Identity"}
            </Button>
            <Button
              disabled={registering}
              variant="outline"
              onClick={handleRegister}
              className="flex-1 text-xs uppercase font-bold tracking-widest"
            >
              {registering ? "Registering…" : "Register with Cloud"}
            </Button>
          </div>
        </CardContent>
      </Card>

      {/* Sibling deployment cards */}
      <div>
        <div className="mb-3 flex items-center justify-between">
          <h2 className="text-[10px] font-bold uppercase tracking-[0.15em] text-muted-foreground">
            All Deployments ({deployments.length})
          </h2>
          <Button variant="ghost" size="sm" onClick={() => void loadData()} className="gap-1 text-xs">
            <RefreshCw className="h-3 w-3" /> Refresh
          </Button>
        </div>
        {deployments.length === 0 ? (
          <Card className="border border-dashed border-border/40 py-12">
            <CardContent className="flex flex-col items-center gap-2 p-0 text-center">
              <MapPin className="h-8 w-8 text-muted-foreground/40" />
              <p className="text-sm text-muted-foreground">
                No sibling deployments found. Register this deployment first.
              </p>
            </CardContent>
          </Card>
        ) : (
          <div className="grid gap-4 sm:grid-cols-2 lg:grid-cols-3">
            {deployments.map((dep) => (
              <DeploymentCard key={dep.deployment_id} dep={dep} />
            ))}
          </div>
        )}
      </div>
    </div>
  );
}
