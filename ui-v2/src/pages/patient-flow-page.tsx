import { useState, useEffect, useCallback } from "react";
import { tauriApi } from "@/lib/tauri-api";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { Badge } from "@/components/ui/badge";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import { Dialog, DialogContent, DialogHeader, DialogTitle, DialogTrigger } from "@/components/ui/dialog";
import { Users, Clock, Activity, BarChart3, UserPlus, LogOut, MapPin } from "lucide-react";

// ── Type definitions matching the Rust commands ─────────────────────────────

interface ActivePatient {
  patient: string;
  patient_token: string;
  check_in_time: string;
  current_zone: string;
  zone_since: string | null;
}

interface ZoneAnalytics {
  zone_id: string;
  sample_count: number;
  wait_p50_seconds: number;
  wait_p90_seconds: number;
  wait_mean_seconds: number;
  throughput_per_day: number;
  current_occupancy: number;
}

interface SimScenario {
  utilization: number;
  mean_wait_seconds: number;
  wait_p50_seconds: number;
  wait_p90_seconds: number;
  feasible: boolean;
}

// ── Zone record type ─────────────────────────────────────────────────────────

interface ZoneRecord {
  name: string | null;
  zone_id: string;
  zone_name: string;
  deployment_id: string;
  zone_type: string;
  capacity: number;
  enabled: boolean;
}

// ── Helpers ──────────────────────────────────────────────────────────────────

function formatSeconds(s: number): string {
  if (!s || s <= 0) return "—";
  if (s < 60) return `${s}s`;
  return `${Math.floor(s / 60)}m ${s % 60}s`;
}

function timeSince(isoString: string | null): string {
  if (!isoString) return "—";
  const diff = Math.floor((Date.now() - new Date(isoString).getTime()) / 1000);
  return formatSeconds(diff);
}

// ── Component ────────────────────────────────────────────────────────────────

export default function PatientFlowPage() {
  // Active patients state
  const [activePatients, setActivePatients] = useState<ActivePatient[]>([]);
  const [activeCount, setActiveCount] = useState(0);
  const [loadingActive, setLoadingActive] = useState(false);

  // Registration state
  const [registerOpen, setRegisterOpen] = useState(false);
  const [patientId, setPatientId] = useState("");
  const [nodeId, setNodeId] = useState("main-node");
  const [sensingUrl, setSensingUrl] = useState("http://localhost:3000");
  const [enrolling, setEnrolling] = useState(false);
  const [enrollCountdown, setEnrollCountdown] = useState(0);
  const [enrollResult, setEnrollResult] = useState<{ patient_token: string; enrollment_status: string } | null>(null);
  const [enrollError, setEnrollError] = useState<string | null>(null);

  // Analytics state
  const [selectedZone, setSelectedZone] = useState("waiting-room");
  const [selectedDays, setSelectedDays] = useState(7);
  const [analytics, setAnalytics] = useState<ZoneAnalytics | null>(null);
  const [journeyLinks, setJourneyLinks] = useState<Array<{ from: string; to: string; value: number }>>([]);
  const [loadingAnalytics, setLoadingAnalytics] = useState(false);

  // Simulation state
  const [simZone, setSimZone] = useState("");
  const [simServers, setSimServers] = useState(2);
  const [simArrival, setSimArrival] = useState(20);
  const [simService, setSimService] = useState(8);
  const [simResults, setSimResults] = useState<Record<string, SimScenario> | null>(null);
  const [simRecommendation, setSimRecommendation] = useState("");
  const [simLoading, setSimLoading] = useState(false);

  // Zone management state
  const [zones, setZones] = useState<ZoneRecord[]>([]);
  const [zonesLoading, setZonesLoading] = useState(false);
  const [showAddZone, setShowAddZone] = useState(false);
  const [newZone, setNewZone] = useState({ zone_id: "", zone_name: "", deployment_id: "", zone_type: "Waiting", capacity: 20, enabled: true });
  const [zoneError, setZoneError] = useState<string | null>(null);
  const [zoneSaving, setZoneSaving] = useState(false);

  // ── Data loading ─────────────────────────────────────────────────────────

  const loadActivePatients = useCallback(async () => {
    setLoadingActive(true);
    try {
      const result = await tauriApi.getActivePatients();
      setActivePatients(result.patients);
      setActiveCount(result.active_count);
    } catch {
      // Frappe not configured — show empty state gracefully
    } finally {
      setLoadingActive(false);
    }
  }, []);

  const loadZones = useCallback(async () => {
    setZonesLoading(true);
    try {
      const result = await tauriApi.listZones();
      setZones(result);
      const first = result.find((z) => z.enabled);
      if (first && !simZone) setSimZone(first.zone_id);
    } catch {
      // Frappe not configured
    } finally {
      setZonesLoading(false);
    }
  }, [simZone]);

  useEffect(() => {
    void loadActivePatients();
    void loadZones();
    const interval = setInterval(() => void loadActivePatients(), 10_000);
    return () => clearInterval(interval);
  }, [loadActivePatients, loadZones]);

  // ── Registration ─────────────────────────────────────────────────────────

  async function handleRegister() {
    if (!patientId.trim()) return;
    setEnrolling(true);
    setEnrollError(null);
    setEnrollResult(null);
    setEnrollCountdown(10);
    const countdown = setInterval(() => {
      setEnrollCountdown((c) => Math.max(0, c - 1));
    }, 1000);
    try {
      const result = await tauriApi.registerPatientArrival({
        patientId: patientId.trim(),
        nodeId,
        sensingServerUrl: sensingUrl,
        enrollmentDurationSeconds: 10,
      });
      setEnrollResult(result);
      void loadActivePatients();
    } catch (e) {
      setEnrollError(String(e));
    } finally {
      clearInterval(countdown);
      setEnrolling(false);
      setEnrollCountdown(0);
    }
  }

  // ── Analytics ────────────────────────────────────────────────────────────

  async function loadAnalytics() {
    setLoadingAnalytics(true);
    try {
      const [a, j] = await Promise.all([
        tauriApi.getZoneAnalytics(selectedZone, selectedDays),
        tauriApi.getPatientJourney(),
      ]);
      setAnalytics(a);
      setJourneyLinks(j.sankey_links);
    } catch {
      // Frappe not configured — show empty state gracefully
    } finally {
      setLoadingAnalytics(false);
    }
  }

  // ── Simulation ───────────────────────────────────────────────────────────

  async function runSimulation() {
    setSimLoading(true);
    try {
      const result = await tauriApi.simulateQueueCapacity({
        zoneId: simZone,
        currentServers: simServers,
        arrivalRatePerHour: simArrival,
        meanServiceMinutes: simService,
      });
      setSimResults(result.scenarios);
      setSimRecommendation(result.recommendation);
    } catch {
      // show error inline
    } finally {
      setSimLoading(false);
    }
  }

  // ── Zone grid ────────────────────────────────────────────────────────────

  const byZone = zones.filter((z) => z.enabled).map((z) => ({
    ...z,
    patients: activePatients.filter(
      (p) => p.current_zone === z.zone_name || p.current_zone === z.zone_id
    ),
  }));

  // ── Render ───────────────────────────────────────────────────────────────

  return (
    <div className="p-6 space-y-6 max-w-6xl mx-auto">
      <div className="flex items-center justify-between">
        <div>
          <h1 className="text-2xl font-bold tracking-tight">Patient Flow</h1>
          <p className="text-sm text-muted-foreground mt-1">
            CSI-based real-time patient tracking and queue analytics
          </p>
        </div>
        <div className="flex items-center gap-3">
          <Badge variant="outline" className="gap-1">
            <Users className="h-3 w-3" />
            {activeCount} active
          </Badge>
          <Dialog open={registerOpen} onOpenChange={setRegisterOpen}>
            <DialogTrigger asChild>
              <Button size="sm" className="gap-2">
                <UserPlus className="h-4 w-4" />
                Register Arrival
              </Button>
            </DialogTrigger>
            <DialogContent className="sm:max-w-md">
              <DialogHeader>
                <DialogTitle>Register Patient Arrival</DialogTitle>
              </DialogHeader>
              <div className="space-y-4 pt-2">
                <div className="space-y-1">
                  <Label className="text-xs font-bold uppercase tracking-wider">Patient ID</Label>
                  <Input
                    placeholder="e.g. PAT-00123"
                    value={patientId}
                    onChange={(e) => setPatientId(e.target.value)}
                    disabled={enrolling}
                  />
                </div>
                <div className="space-y-1">
                  <Label className="text-xs font-bold uppercase tracking-wider">Node ID</Label>
                  <Input value={nodeId} onChange={(e) => setNodeId(e.target.value)} disabled={enrolling} />
                </div>
                <div className="space-y-1">
                  <Label className="text-xs font-bold uppercase tracking-wider">Sensing Server URL</Label>
                  <Input value={sensingUrl} onChange={(e) => setSensingUrl(e.target.value)} disabled={enrolling} />
                </div>
                {enrolling && (
                  <div className="rounded-md bg-blue-500/10 border border-blue-500/30 px-3 py-2 text-sm text-blue-400">
                    Capturing CSI fingerprint... {enrollCountdown}s remaining
                  </div>
                )}
                {enrollResult && (
                  <div className="rounded-md bg-emerald-500/10 border border-emerald-500/30 px-3 py-2 text-sm text-emerald-400 space-y-1">
                    <p className="font-semibold">Patient registered</p>
                    <p className="font-mono text-xs">Token: {enrollResult.patient_token}</p>
                    <p>{enrollResult.enrollment_status}</p>
                  </div>
                )}
                {enrollError && (
                  <p className="text-sm text-destructive">{enrollError}</p>
                )}
                <Button
                  className="w-full"
                  onClick={handleRegister}
                  disabled={enrolling || !patientId.trim()}
                >
                  {enrolling ? "Enrolling..." : "Register & Enroll (10s)"}
                </Button>
              </div>
            </DialogContent>
          </Dialog>
        </div>
      </div>

      <Tabs defaultValue="active">
        <TabsList className="grid w-full grid-cols-4">
          <TabsTrigger value="active" className="gap-2">
            <Activity className="h-4 w-4" /> Live Floor
          </TabsTrigger>
          <TabsTrigger value="analytics" className="gap-2">
            <Clock className="h-4 w-4" /> Analytics
          </TabsTrigger>
          <TabsTrigger value="simulation" className="gap-2">
            <BarChart3 className="h-4 w-4" /> Simulation
          </TabsTrigger>
          <TabsTrigger value="zones" className="gap-2">
            <MapPin className="h-4 w-4" /> Zones
          </TabsTrigger>
        </TabsList>

        {/* ── LIVE FLOOR ─────────────────────────────────────────────────────── */}
        <TabsContent value="active" className="mt-4">
          {activePatients.length === 0 && !loadingActive ? (
            <div className="text-center py-16 text-muted-foreground">
              <Users className="h-10 w-10 mx-auto mb-3 opacity-30" />
              <p className="text-sm">No active patients. Register an arrival to begin tracking.</p>
            </div>
          ) : (
            <div className="grid gap-4 md:grid-cols-2 lg:grid-cols-3">
              {byZone.filter((z) => z.patients.length > 0 || z.zone_type !== "Exit").map((zone) => (
                <Card key={zone.zone_id} className={zone.patients.length > 0 ? "border-primary/40" : ""}>
                  <CardHeader className="pb-2">
                    <CardTitle className="text-sm font-semibold flex justify-between">
                      {zone.zone_name}
                      <Badge variant={zone.patients.length > 0 ? "default" : "outline"}>
                        {zone.patients.length}
                      </Badge>
                    </CardTitle>
                  </CardHeader>
                  <CardContent>
                    {zone.patients.length === 0 ? (
                      <p className="text-xs text-muted-foreground">Empty</p>
                    ) : (
                      <ul className="space-y-1">
                        {zone.patients.map((p) => (
                          <li key={p.patient_token} className="flex justify-between items-center text-xs">
                            <span className="font-mono">{p.patient_token.slice(0, 8)}...</span>
                            <div className="flex items-center gap-2">
                              <span className="text-muted-foreground">{timeSince(p.zone_since)}</span>
                              <Button
                                variant="ghost"
                                size="icon"
                                className="h-5 w-5"
                                title="Check out"
                                onClick={async () => {
                                  await tauriApi.checkoutPatientVisit(p.patient_token);
                                  void loadActivePatients();
                                }}
                              >
                                <LogOut className="h-3 w-3" />
                              </Button>
                            </div>
                          </li>
                        ))}
                      </ul>
                    )}
                  </CardContent>
                </Card>
              ))}
            </div>
          )}
        </TabsContent>

        {/* ── ANALYTICS ──────────────────────────────────────────────────────── */}
        <TabsContent value="analytics" className="mt-4 space-y-4">
          <div className="flex gap-3 items-end">
            <div className="space-y-1 flex-1">
              <Label className="text-xs font-bold uppercase tracking-wider">Zone</Label>
              <Select value={selectedZone} onValueChange={setSelectedZone}>
                <SelectTrigger><SelectValue placeholder="Select zone" /></SelectTrigger>
                <SelectContent>
                  {zones.map((z) => (
                    <SelectItem key={z.zone_id} value={z.zone_id}>{z.zone_name}</SelectItem>
                  ))}
                </SelectContent>
              </Select>
            </div>
            <div className="space-y-1">
              <Label className="text-xs font-bold uppercase tracking-wider">Period</Label>
              <Select value={String(selectedDays)} onValueChange={(v) => setSelectedDays(Number(v))}>
                <SelectTrigger className="w-36"><SelectValue /></SelectTrigger>
                <SelectContent>
                  <SelectItem value="1">Today</SelectItem>
                  <SelectItem value="7">Last 7 days</SelectItem>
                  <SelectItem value="30">Last 30 days</SelectItem>
                </SelectContent>
              </Select>
            </div>
            <Button onClick={loadAnalytics} disabled={loadingAnalytics}>
              {loadingAnalytics ? "Loading..." : "Load"}
            </Button>
          </div>

          {analytics && (
            <>
              <div className="grid grid-cols-3 gap-4">
                {[
                  { label: "Median Wait", value: formatSeconds(analytics.wait_p50_seconds) },
                  { label: "90th % Wait", value: formatSeconds(analytics.wait_p90_seconds) },
                  { label: "Daily Throughput", value: `${analytics.throughput_per_day} pts/day` },
                ].map((s) => (
                  <Card key={s.label}>
                    <CardContent className="pt-5">
                      <p className="text-xs text-muted-foreground uppercase tracking-wider">{s.label}</p>
                      <p className="text-2xl font-bold mt-1">{s.value}</p>
                    </CardContent>
                  </Card>
                ))}
              </div>
              <p className="text-xs text-muted-foreground">Based on {analytics.sample_count} completed visits</p>
            </>
          )}

          {journeyLinks.length > 0 && (
            <Card>
              <CardHeader><CardTitle className="text-sm">Patient Journeys (today)</CardTitle></CardHeader>
              <CardContent>
                <ul className="space-y-1 text-sm">
                  {journeyLinks.map((l, i) => (
                    <li key={i} className="flex justify-between">
                      <span>{l.from} → {l.to}</span>
                      <Badge variant="secondary">{l.value} patients</Badge>
                    </li>
                  ))}
                </ul>
              </CardContent>
            </Card>
          )}
        </TabsContent>

        {/* ── SIMULATION ─────────────────────────────────────────────────────── */}
        <TabsContent value="simulation" className="mt-4 space-y-4">
          <Card>
            <CardHeader>
              <CardTitle className="text-sm">What-if Capacity Planning</CardTitle>
              <p className="text-xs text-muted-foreground">
                Estimate the impact of adding service stations or staff using queuing theory (Erlang-C model)
              </p>
            </CardHeader>
            <CardContent className="space-y-4">
              <div className="grid gap-4 md:grid-cols-2 lg:grid-cols-4">
                <div className="space-y-1">
                  <Label className="text-xs font-bold uppercase tracking-wider">Zone</Label>
                  <Select value={simZone} onValueChange={setSimZone}>
                    <SelectTrigger><SelectValue placeholder="Select zone" /></SelectTrigger>
                    <SelectContent>
                      {zones.map((z) => (
                        <SelectItem key={z.zone_id} value={z.zone_id}>{z.zone_name}</SelectItem>
                      ))}
                    </SelectContent>
                  </Select>
                </div>
                <div className="space-y-1">
                  <Label className="text-xs font-bold uppercase tracking-wider">Current Stations</Label>
                  <Input
                    type="number"
                    min={1}
                    max={10}
                    value={simServers}
                    onChange={(e) => setSimServers(Number(e.target.value))}
                  />
                </div>
                <div className="space-y-1">
                  <Label className="text-xs font-bold uppercase tracking-wider">Arrivals / Hour</Label>
                  <Input
                    type="number"
                    min={1}
                    value={simArrival}
                    onChange={(e) => setSimArrival(Number(e.target.value))}
                  />
                </div>
                <div className="space-y-1">
                  <Label className="text-xs font-bold uppercase tracking-wider">Avg Service (min)</Label>
                  <Input
                    type="number"
                    min={1}
                    value={simService}
                    onChange={(e) => setSimService(Number(e.target.value))}
                  />
                </div>
              </div>
              <Button onClick={runSimulation} disabled={simLoading} className="w-full">
                {simLoading ? "Calculating..." : "Run Simulation"}
              </Button>
            </CardContent>
          </Card>

          {simResults && (
            <Card>
              <CardHeader>
                <CardTitle className="text-sm">Simulation Results</CardTitle>
                {simRecommendation && (
                  <p className="text-xs text-emerald-400">
                    Recommended: {simRecommendation} station{Number(simRecommendation) !== 1 ? "s" : ""}
                  </p>
                )}
              </CardHeader>
              <CardContent>
                <table className="w-full text-sm">
                  <thead>
                    <tr className="border-b">
                      <th className="text-left py-2 text-xs font-bold uppercase tracking-wider">Stations</th>
                      <th className="text-right py-2 text-xs font-bold uppercase tracking-wider">Utilization</th>
                      <th className="text-right py-2 text-xs font-bold uppercase tracking-wider">Median Wait</th>
                      <th className="text-right py-2 text-xs font-bold uppercase tracking-wider">90th % Wait</th>
                      <th className="text-right py-2 text-xs font-bold uppercase tracking-wider">Status</th>
                    </tr>
                  </thead>
                  <tbody>
                    {Object.entries(simResults).map(([c, s]) => (
                      <tr
                        key={c}
                        className={`border-b ${c === simRecommendation ? "bg-emerald-500/10 text-emerald-400" : ""}`}
                      >
                        <td className="py-2 font-semibold">{c}</td>
                        <td className="py-2 text-right">
                          {s.feasible ? `${(s.utilization * 100).toFixed(0)}%` : "—"}
                        </td>
                        <td className="py-2 text-right">
                          {s.feasible ? formatSeconds(Math.round(s.wait_p50_seconds)) : "Overloaded"}
                        </td>
                        <td className="py-2 text-right">
                          {s.feasible ? formatSeconds(Math.round(s.wait_p90_seconds)) : "Overloaded"}
                        </td>
                        <td className="py-2 text-right">
                          <Badge
                            variant={
                              s.feasible
                                ? c === simRecommendation
                                  ? "default"
                                  : "outline"
                                : "danger"
                            }
                          >
                            {s.feasible
                              ? c === simRecommendation
                                ? "Recommended"
                                : "Feasible"
                              : "Infeasible"}
                          </Badge>
                        </td>
                      </tr>
                    ))}
                  </tbody>
                </table>
                {simResults && simRecommendation !== String(simServers) && (
                  <p className="mt-3 text-xs text-muted-foreground">
                    Adding {Number(simRecommendation) - simServers} station(s) would reduce the 90th percentile
                    wait from{" "}
                    {formatSeconds(Math.round(simResults[String(simServers)]?.wait_p90_seconds ?? 0))} to{" "}
                    {formatSeconds(Math.round(simResults[simRecommendation]?.wait_p90_seconds ?? 0))}.
                  </p>
                )}
              </CardContent>
            </Card>
          )}
        </TabsContent>

        {/* ── ZONES MANAGEMENT ───────────────────────────────────────────────── */}
        <TabsContent value="zones" className="mt-4 space-y-4">
          <div className="flex items-center justify-between">
            <p className="text-sm text-muted-foreground">
              Map sensing nodes to named clinic zones. The deployment ID must match the node IP or
              identifier configured on the sensing server.
            </p>
            <Button size="sm" className="gap-2" onClick={() => setShowAddZone((v) => !v)}>
              <MapPin className="h-4 w-4" />
              {showAddZone ? "Cancel" : "Add Zone"}
            </Button>
          </div>

          {showAddZone && (
            <Card>
              <CardHeader><CardTitle className="text-sm">New Zone</CardTitle></CardHeader>
              <CardContent className="space-y-3">
                <div className="grid gap-3 md:grid-cols-2">
                  <div className="space-y-1">
                    <Label className="text-xs font-bold uppercase tracking-wider">Zone ID</Label>
                    <Input
                      placeholder="e.g. waiting-room"
                      value={newZone.zone_id}
                      onChange={(e) => setNewZone((z) => ({ ...z, zone_id: e.target.value }))}
                    />
                  </div>
                  <div className="space-y-1">
                    <Label className="text-xs font-bold uppercase tracking-wider">Display Name</Label>
                    <Input
                      placeholder="e.g. Waiting Room"
                      value={newZone.zone_name}
                      onChange={(e) => setNewZone((z) => ({ ...z, zone_name: e.target.value }))}
                    />
                  </div>
                  <div className="space-y-1">
                    <Label className="text-xs font-bold uppercase tracking-wider">Node IP / Deployment ID</Label>
                    <Input
                      placeholder="e.g. 192.168.1.42"
                      value={newZone.deployment_id}
                      onChange={(e) => setNewZone((z) => ({ ...z, deployment_id: e.target.value }))}
                    />
                  </div>
                  <div className="space-y-1">
                    <Label className="text-xs font-bold uppercase tracking-wider">Zone Type</Label>
                    <Select value={newZone.zone_type} onValueChange={(v) => setNewZone((z) => ({ ...z, zone_type: v }))}>
                      <SelectTrigger><SelectValue /></SelectTrigger>
                      <SelectContent>
                        <SelectItem value="Waiting">Waiting</SelectItem>
                        <SelectItem value="Consultation">Consultation</SelectItem>
                        <SelectItem value="Procedure">Procedure</SelectItem>
                        <SelectItem value="Pharmacy">Pharmacy</SelectItem>
                        <SelectItem value="Exit">Exit</SelectItem>
                      </SelectContent>
                    </Select>
                  </div>
                  <div className="space-y-1">
                    <Label className="text-xs font-bold uppercase tracking-wider">Capacity</Label>
                    <Input
                      type="number"
                      min={1}
                      value={newZone.capacity}
                      onChange={(e) => setNewZone((z) => ({ ...z, capacity: Number(e.target.value) }))}
                    />
                  </div>
                </div>
                {zoneError && <p className="text-sm text-destructive">{zoneError}</p>}
                <Button
                  className="w-full"
                  disabled={zoneSaving || !newZone.zone_id.trim() || !newZone.zone_name.trim()}
                  onClick={async () => {
                    setZoneSaving(true);
                    setZoneError(null);
                    try {
                      await tauriApi.createZone(newZone);
                      setNewZone({ zone_id: "", zone_name: "", deployment_id: "", zone_type: "Waiting", capacity: 20, enabled: true });
                      setShowAddZone(false);
                      void loadZones();
                    } catch (e) {
                      setZoneError(String(e));
                    } finally {
                      setZoneSaving(false);
                    }
                  }}
                >
                  {zoneSaving ? "Saving..." : "Create Zone"}
                </Button>
              </CardContent>
            </Card>
          )}

          {zonesLoading ? (
            <p className="text-sm text-muted-foreground py-4">Loading zones...</p>
          ) : zones.length === 0 ? (
            <div className="text-center py-16 text-muted-foreground">
              <MapPin className="h-10 w-10 mx-auto mb-3 opacity-30" />
              <p className="text-sm">No zones configured. Add a zone to start tracking patient flow.</p>
            </div>
          ) : (
            <div className="grid gap-3 md:grid-cols-2 lg:grid-cols-3">
              {zones.map((z) => (
                <Card key={z.zone_id}>
                  <CardHeader className="pb-2">
                    <CardTitle className="text-sm flex justify-between items-start">
                      <span>{z.zone_name}</span>
                      <Badge variant="outline" className="text-xs shrink-0 ml-2">{z.zone_type}</Badge>
                    </CardTitle>
                  </CardHeader>
                  <CardContent className="space-y-2">
                    <div className="text-xs text-muted-foreground space-y-0.5">
                      <p><span className="font-semibold">ID:</span> {z.zone_id}</p>
                      <p><span className="font-semibold">Node:</span> {z.deployment_id || "—"}</p>
                      <p><span className="font-semibold">Capacity:</span> {z.capacity}</p>
                    </div>
                    <Button
                      variant="ghost"
                      size="sm"
                      className="w-full text-destructive hover:text-destructive hover:bg-destructive/10 text-xs mt-1"
                      onClick={async () => {
                        if (!z.name) return;
                        try {
                          await tauriApi.deleteZone(z.name);
                          void loadZones();
                        } catch (e) {
                          setZoneError(String(e));
                        }
                      }}
                    >
                      Delete
                    </Button>
                  </CardContent>
                </Card>
              ))}
            </div>
          )}
        </TabsContent>
      </Tabs>
    </div>
  );
}
