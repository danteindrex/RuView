import React, { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { tauriApi } from "@/lib/tauri-api";
import { useSensingStore } from "@/lib/sensing-store";
import { usePlanStore } from "@/lib/plan-store";
import { hasCloudConsent, langfuseTraceUrl } from "@/lib/integration";
import { Card, CardContent, CardHeader, CardTitle, CardDescription } from "@/components/ui/card";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Progress } from "@/components/ui/progress";
import { Tabs, TabsList, TabsTrigger, TabsContent } from "@/components/ui/tabs";
import { RiskGauge } from "@/components/analytics/risk-gauge";
import { TrendChart } from "@/components/analytics/trend-chart";
import { RiskDistribution } from "@/components/analytics/risk-distribution";
import { AgentAccordion } from "@/components/analytics/agent-accordion";
import { UpgradePrompt } from "@/components/upgrade-prompt";

// ---- types ----

interface InsightResult {
  vitals_analysis?: {
    hr_classification?: string;
    br_classification?: string;
    hrv_assessment?: string;
    observations?: string[];
  };
  anomaly_analysis?: {
    detected?: boolean;
    fall_risk_score?: number;
    mobility_score?: number;
  };
  clinical_interpretation?: {
    primary_findings?: string;
    differential?: string;
    recommended_actions?: string[];
    urgency?: "routine" | "urgent" | "emergency";
  };
  risk_assessment?: {
    composite_score?: number;
    fall_score?: number;
    cardiovascular_score?: number;
    respiratory_score?: number;
  };
  trend_analysis?: {
    direction?: "improving" | "stable" | "deteriorating";
    significant_changes?: string[];
  };
  synthesis?: { summary?: string; action_items?: string[] };
  trace_id?: string;
}

interface TrendPoint { label: string; hr: number; br: number; }
type RiskBucket = { label: "Low" | "Moderate" | "High" | "Critical"; count: number };

// ---- helpers ----

function Sparkline({ data, color = "currentColor" }: { data: number[]; color?: string }) {
  const max = Math.max(...data, 1);
  const min = Math.min(...data, 0);
  const range = max - min || 1;
  const pts = data.map((d, i) => `${(i / (data.length - 1)) * 100},${100 - ((d - min) / range) * 100}`).join(" ");
  return (
    <svg viewBox="0 0 100 100" className="w-full h-12 overflow-visible">
      <polyline fill="none" stroke={color} strokeWidth="4" strokeLinecap="round" strokeLinejoin="round" points={pts} />
    </svg>
  );
}

function directionIcon(dir?: string) {
  if (dir === "improving") return "Improving";
  if (dir === "deteriorating") return "Deteriorating";
  return "Stable";
}

// ---- component ----

export function MedicalPage() {
  const { latestUpdate, edgeVitals } = useSensingStore();
  const planStore = usePlanStore();
  const isCloud = planStore.isCloud();

  const [hrHistory, setHrHistory] = useState<number[]>(new Array(20).fill(0));
  const [brHistory, setBrHistory] = useState<number[]>(new Array(20).fill(0));
  const [trendPoints, setTrendPoints] = useState<TrendPoint[]>([]);
  const [insightResult, setInsightResult] = useState<InsightResult | null>(null);
  const [insightLoading, setInsightLoading] = useState(false);
  const [insightError, setInsightError] = useState<string | null>(null);
  const [riskDist, setRiskDist] = useState<RiskBucket[]>([
    { label: "Low", count: 0 },
    { label: "Moderate", count: 0 },
    { label: "High", count: 0 },
    { label: "Critical", count: 0 },
  ]);
  const [deploymentId, setDeploymentId] = useState<string>("");

  useEffect(() => { planStore.load(); }, []);

  useEffect(() => {
    void tauriApi.getDeploymentInfo().then((info) => {
      setDeploymentId(info.deployment_id);
    }).catch(() => {});
  }, []);

  useEffect(() => {
    const hr = latestUpdate?.vital_signs?.heart_rate_bpm;
    const br = latestUpdate?.vital_signs?.breathing_rate_bpm;
    if (hr) {
      setHrHistory(p => [...p.slice(1), hr]);
      setTrendPoints(p => [...p.slice(-19), { label: new Date().toLocaleTimeString(), hr, br: br ?? 0 }]);
    }
    if (br) setBrHistory(p => [...p.slice(1), br]);
  }, [latestUpdate]);

  const vitals = latestUpdate?.vital_signs;
  const isFallDetected = latestUpdate?.posture === "lying_down" || edgeVitals?.fall_detected === true;
  const synced = hasCloudConsent();
  const composite = insightResult?.risk_assessment?.composite_score ?? 0;

  async function runAnalysis() {
    setInsightLoading(true);
    setInsightError(null);
    const sessionId = `session-${Date.now()}`;
    try {
      // Submit to Frappe (returns immediately, pipeline runs async in RQ worker)
      await invoke<{ status: string; session_name: string; insight_queued: boolean }>(
        "run_insight_pipeline",
        {
          request: {
            session_id: sessionId,
            deployment_id: deploymentId || "default",
            vital_summary: vitals ?? {},
            pose_anomalies: isFallDetected ? ["lying_down"] : [],
            duration_seconds: 60,
            csi_snr_db: 15.0,
          },
        }
      );

      // Poll for result — Frappe RQ worker typically completes in 2-10s
      let attempts = 0;
      const maxAttempts = 20;
      const poll = async (): Promise<void> => {
        if (attempts >= maxAttempts) {
          setInsightError("Analysis timed out — pipeline may still be running in background.");
          return;
        }
        attempts++;
        const raw = await invoke<Record<string, unknown> | null>(
          "get_session_insight",
          { sessionId }
        ).catch(() => null);

        if (!raw) {
          // Not ready yet — wait 2s and retry
          await new Promise<void>((res) => setTimeout(res, 2000));
          return poll();
        }

        // Map flat Frappe Insight Report → nested InsightResult
        const result: InsightResult = {
          vitals_analysis: {
            hr_classification: raw.hr_classification as string | undefined,
            br_classification: raw.br_classification as string | undefined,
          },
          anomaly_analysis: {
            fall_risk_score: typeof raw.fall_risk_score === "number"
              ? raw.fall_risk_score / 100 : undefined,
          },
          clinical_interpretation: {
            primary_findings: raw.summary as string | undefined,
            recommended_actions: typeof raw.action_items === "string"
              ? (raw.action_items as string).split("\n").filter(Boolean)
              : [],
            urgency: (raw.risk_level === "critical" ? "emergency"
              : raw.risk_level === "high" ? "urgent"
              : raw.risk_level === "moderate" ? "routine"
              : "routine") as "routine" | "urgent" | "emergency",
          },
          risk_assessment: {
            composite_score: typeof raw.risk_score === "number"
              ? raw.risk_score * 100 : 0,
            fall_score: typeof raw.fall_risk_score === "number"
              ? raw.fall_risk_score : 0,
          },
          trend_analysis: {
            direction: raw.trend_direction as "improving" | "stable" | "deteriorating" | undefined,
          },
          synthesis: {
            summary: raw.summary as string | undefined,
            action_items: typeof raw.action_items === "string"
              ? (raw.action_items as string).split("\n").filter(Boolean)
              : [],
          },
        };
        setInsightResult(result);

        if (isCloud) {
          const dist = await invoke<{ distribution: { low: number; moderate: number; high: number; critical: number } }>(
            "get_risk_distribution"
          ).then((r) => r.distribution).catch(() => ({ low: 0, moderate: 0, high: 0, critical: 0 }));
          setRiskDist([
            { label: "Low", count: dist.low },
            { label: "Moderate", count: dist.moderate },
            { label: "High", count: dist.high },
            { label: "Critical", count: dist.critical },
          ]);
        }
      };
      await poll();
    } catch (e) {
      setInsightError(String(e));
    } finally {
      setInsightLoading(false);
    }
  }

  return (
    <div className="space-y-6 p-6 max-w-7xl mx-auto">
      <div className="flex justify-between items-center flex-wrap gap-3" data-tour="medical-header">
        <div>
          <h1 className="text-3xl font-bold tracking-tight">Medical Monitoring</h1>
          <p className="text-muted-foreground">Real-time WiFi CSI vital signs, fall detection, and AI insights.</p>
        </div>
        <div className="flex items-center gap-2 flex-wrap">
          <Badge variant="outline" className={synced ? "text-emerald-600 border-emerald-400" : "text-muted-foreground"}>
            {synced ? "Synced to Cloud" : "Local Only"}
          </Badge>
          <Badge variant={isFallDetected ? "danger" : "outline"} className="px-4 py-1 text-sm animate-pulse">
            {isFallDetected ? "EMERGENCY: FALL DETECTED" : "SYSTEM NORMAL"}
          </Badge>
        </div>
      </div>

      {/* Vitals row */}
      <div className="grid grid-cols-1 md:grid-cols-2 xl:grid-cols-4 gap-6">
        <Card className="bg-gradient-to-br from-background to-rose-50/10 border-rose-500/20">
          <CardHeader className="pb-2"><CardTitle className="text-sm font-medium">Heart Rate</CardTitle></CardHeader>
          <CardContent>
            <div className="text-4xl font-bold">{vitals?.heart_rate_bpm?.toFixed(0) ?? "--"}</div>
            <p className="text-xs text-muted-foreground mt-1">BPM</p>
            <div className="mt-4"><Sparkline data={hrHistory} color="#f43f5e" /></div>
            <div className="mt-2 h-1.5 w-full bg-secondary rounded-full overflow-hidden">
              <div className="h-full bg-rose-500 transition-all duration-500" style={{ width: `${Math.min(100, vitals?.heart_rate_bpm ?? 0)}%` }} />
            </div>
          </CardContent>
        </Card>
        <Card className="bg-gradient-to-br from-background to-blue-50/10 border-blue-500/20">
          <CardHeader className="pb-2"><CardTitle className="text-sm font-medium">Respiration</CardTitle></CardHeader>
          <CardContent>
            <div className="text-4xl font-bold">{vitals?.breathing_rate_bpm?.toFixed(1) ?? "--"}</div>
            <p className="text-xs text-muted-foreground mt-1">RPM</p>
            <div className="mt-4"><Sparkline data={brHistory} color="#3b82f6" /></div>
            <div className="mt-2 h-1.5 w-full bg-secondary rounded-full overflow-hidden">
              <div className="h-full bg-blue-500 transition-all duration-500" style={{ width: `${Math.min(100, (vitals?.breathing_rate_bpm ?? 0) * 3)}%` }} />
            </div>
          </CardContent>
        </Card>
        <Card className="bg-gradient-to-br from-background to-emerald-50/10 border-emerald-500/20">
          <CardHeader className="pb-2"><CardTitle className="text-sm font-medium">Posture / Activity</CardTitle></CardHeader>
          <CardContent>
            <div className="text-2xl font-bold capitalize">{latestUpdate?.posture ?? "Stationary"}</div>
            <p className="text-xs text-muted-foreground mt-1">WiFi Pose Estimation active</p>
            <div className="mt-4"><span className="text-xs font-medium text-emerald-500 uppercase tracking-tighter">Continuous Monitoring</span></div>
          </CardContent>
        </Card>
        <Card className="bg-gradient-to-br from-background to-violet-50/10 border-violet-500/20">
          <CardHeader className="pb-2">
            <CardTitle className="text-sm font-medium">Edge Node Vitals</CardTitle>
            {edgeVitals && <Badge variant="outline">Node {edgeVitals.node_id}</Badge>}
          </CardHeader>
          <CardContent>
            {edgeVitals ? (
              <div className="space-y-1 text-sm">
                <div className="flex justify-between"><span className="text-muted-foreground">Heart Rate</span><span className="font-semibold">{(edgeVitals.heartrate_bpm ?? edgeVitals.heart_rate_bpm)?.toFixed(0) ?? "--"} BPM</span></div>
                <div className="flex justify-between"><span className="text-muted-foreground">Respiration</span><span className="font-semibold">{edgeVitals.breathing_rate_bpm?.toFixed(1) ?? "--"} RPM</span></div>
                <div className="flex justify-between"><span className="text-muted-foreground">Presence</span><span className="font-semibold">{edgeVitals.presence ? "Yes" : "No"}</span></div>
                {edgeVitals.fall_detected && <p className="mt-1 text-xs font-bold uppercase text-destructive">Edge fall flag raised</p>}
                <p className="mt-1 text-[10px] font-mono text-muted-foreground opacity-70">{edgeVitals.received_at ? new Date(edgeVitals.received_at).toLocaleTimeString() : ""}</p>
              </div>
            ) : <p className="text-sm text-muted-foreground">No edge_vitals packets received yet.</p>}
          </CardContent>
        </Card>
      </div>

      <Tabs defaultValue="log">
        <TabsList>
          <TabsTrigger value="log">Event Log</TabsTrigger>
          <TabsTrigger value="insights">AI Insights</TabsTrigger>
          <TabsTrigger value="analytics">Analytics</TabsTrigger>
        </TabsList>

        <TabsContent value="log">
          <Card>
            <CardHeader>
              <CardTitle>Medical Event Log</CardTitle>
              <CardDescription>Recent physiological status changes and dispatched alerts.</CardDescription>
            </CardHeader>
            <CardContent>
              <div className="space-y-4 max-h-[400px] overflow-auto pr-2">
                {isFallDetected && (
                  <div className="p-3 bg-destructive/10 border border-destructive/20 rounded-lg animate-pulse">
                    <p className="text-sm font-bold text-destructive">EMERGENCY: Fall Detected</p>
                    <p className="text-xs text-muted-foreground">Sensing engine recognized high-velocity vertical displacement.</p>
                    <p className="text-[10px] font-mono text-muted-foreground mt-1 opacity-70">{new Date().toLocaleTimeString()}</p>
                  </div>
                )}
                <div className="p-3 bg-secondary/30 rounded-lg border border-border/40">
                  <p className="text-sm font-semibold">Continuous Vitals Monitor</p>
                  <p className="text-xs text-muted-foreground">WiFi CSI subcarrier analysis is extracting sub-millimeter chest wall movement.</p>
                  <p className="text-[10px] font-mono text-muted-foreground mt-1 opacity-70">{new Date().toLocaleTimeString()}</p>
                </div>
                <div className="p-3 bg-secondary/20 rounded-lg border border-border/40 opacity-70">
                  <p className="text-sm font-semibold">Baseline Established</p>
                  <p className="text-xs text-muted-foreground">Environmental noise floor calibrated. Sensitivity set to medical mode.</p>
                  <p className="text-[10px] font-mono text-muted-foreground mt-1 opacity-70">System Boot</p>
                </div>
              </div>
            </CardContent>
          </Card>
        </TabsContent>

        <TabsContent value="insights">
          <Card>
            <CardHeader>
              <CardTitle className="flex items-center justify-between flex-wrap gap-2">
                <span>LangGraph Multi-Agent Analysis</span>
                <Button onClick={runAnalysis} disabled={insightLoading} size="sm">
                  {insightLoading ? "Running..." : "Run Analysis"}
                </Button>
              </CardTitle>
              <CardDescription>Runs vitals, anomaly, clinical, risk, and trend agents.</CardDescription>
            </CardHeader>
            <CardContent>
              {insightError && <p className="text-sm text-destructive mb-3">Error: {insightError}</p>}
              {insightResult ? (
                <div className="space-y-4">
                  <div className="grid grid-cols-1 sm:grid-cols-2 gap-4 items-start">
                    <div>
                      <p className="text-xs text-muted-foreground uppercase tracking-wider mb-2">Composite Risk Score</p>
                      <RiskGauge score={composite} />
                    </div>
                    <div className="space-y-2 text-sm">
                      {insightResult.risk_assessment && (
                        <>
                          {[
                            { label: "Fall Risk", val: insightResult.risk_assessment.fall_score },
                            { label: "Cardiovascular", val: insightResult.risk_assessment.cardiovascular_score },
                            { label: "Respiratory", val: insightResult.risk_assessment.respiratory_score },
                          ].map(({ label, val }) => (
                            <div key={label}>
                              <div className="flex justify-between mb-1"><span className="text-muted-foreground">{label}</span><span>{val ?? 0}</span></div>
                              <Progress value={val ?? 0} />
                            </div>
                          ))}
                        </>
                      )}
                      {insightResult.trace_id && (
                        <a href={langfuseTraceUrl(insightResult.trace_id)} target="_blank" rel="noopener noreferrer"
                          className="inline-block text-xs text-muted-foreground underline mt-2">
                          Trace: {insightResult.trace_id.slice(0, 12)}...
                        </a>
                      )}
                    </div>
                  </div>
                  <AgentAccordion result={insightResult} />
                </div>
              ) : (
                <p className="text-sm text-muted-foreground">Click "Run Analysis" to run the LangGraph multi-agent pipeline.</p>
              )}
            </CardContent>
          </Card>
        </TabsContent>

        <TabsContent value="analytics">
          {isCloud ? (
            <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
              <Card>
                <CardHeader>
                  <CardTitle className="text-sm">HR / BR Trend</CardTitle>
                  <CardDescription>Moving average over last {trendPoints.length} readings</CardDescription>
                </CardHeader>
                <CardContent>
                  <TrendChart points={trendPoints} />
                  {insightResult?.trend_analysis?.direction && (
                    <p className="text-sm mt-2 font-medium">{directionIcon(insightResult.trend_analysis.direction)}</p>
                  )}
                </CardContent>
              </Card>
              <Card>
                <CardHeader>
                  <CardTitle className="text-sm">Risk Distribution</CardTitle>
                  <CardDescription>Sessions by risk level</CardDescription>
                </CardHeader>
                <CardContent><RiskDistribution buckets={riskDist} /></CardContent>
              </Card>
            </div>
          ) : (
            <UpgradePrompt feature="Cross-Session Analytics" requiredPlan="cloud" className="py-16" />
          )}
        </TabsContent>
      </Tabs>
    </div>
  );
}
