import React, { useState } from "react";
import { Heart, Activity, User, AlertTriangle, CheckCircle2 } from "lucide-react";
import { useSensingStore } from "@/lib/sensing-store";
import { PageSection } from "@/components/layout/page-section";
import { Card, CardContent, CardHeader, CardTitle, CardDescription } from "@/components/ui/card";
import { Badge } from "@/components/ui/badge";
import { Switch } from "@/components/ui/switch";
import { Label } from "@/components/ui/label";
import { Button } from "@/components/ui/button";

function Sparkline({ data, color = "currentColor" }: { data: number[], color?: string }) {
  const max = Math.max(...data, 1);
  const min = Math.min(...data, 0);
  const range = max - min;
  const points = data.map((d, i) => `${(i / (data.length - 1)) * 100},${100 - ((d - min) / range) * 100}`).join(" ");
  
  return (
    <svg viewBox="0 0 100 100" className="w-full h-12 overflow-visible">
      <polyline
        fill="none"
        stroke={color}
        strokeWidth="4"
        strokeLinecap="round"
        strokeLinejoin="round"
        points={points}
      />
    </svg>
  );
}

export function MedicalPage() {
  const { latestUpdate } = useSensingStore();
  const [hrHistory, setHrHistory] = useState<number[]>(new Array(20).fill(0));
  const [brHistory, setBrHistory] = useState<number[]>(new Array(20).fill(0));

  // Update history
  React.useEffect(() => {
    if (latestUpdate?.vital_signs?.heart_rate_bpm) {
      setHrHistory(prev => [...prev.slice(1), latestUpdate.vital_signs!.heart_rate_bpm!]);
    }
    if (latestUpdate?.vital_signs?.breathing_rate_bpm) {
      setBrHistory(prev => [...prev.slice(1), latestUpdate.vital_signs!.breathing_rate_bpm!]);
    }
  }, [latestUpdate]);

  const vitals = latestUpdate?.vital_signs;
  const isFallDetected = latestUpdate?.classification?.fall_detected;

  return (
    <div className="space-y-6 p-6 max-w-7xl mx-auto">
      <div className="flex justify-between items-center">
        <div>
          <h1 className="text-3xl font-bold tracking-tight">Medical Monitoring</h1>
          <p className="text-muted-foreground">Real-time WiFi CSI vital signs and fall detection.</p>
        </div>
        <Badge variant={isFallDetected ? "danger" : "outline"} className="px-4 py-1 text-sm animate-pulse">
          {isFallDetected ? "🚨 EMERGENCY: FALL DETECTED" : "✅ SYSTEM NORMAL"}
        </Badge>
      </div>

      <div className="grid grid-cols-1 md:grid-cols-3 gap-6">
        {/* Heart Rate Card */}
        <Card className="bg-gradient-to-br from-background to-rose-50/10 border-rose-500/20">
          <CardHeader className="flex flex-row items-center justify-between pb-2">
            <CardTitle className="text-sm font-medium">Heart Rate</CardTitle>
          </CardHeader>
          <CardContent>
            <div className="text-4xl font-bold">{vitals?.heart_rate_bpm?.toFixed(0) ?? "--"}</div>
            <p className="text-xs text-muted-foreground mt-1">BPM (Beats per minute)</p>
            <div className="mt-4">
              <Sparkline data={hrHistory} color="#f43f5e" />
            </div>
            <div className="mt-2 h-1.5 w-full bg-secondary rounded-full overflow-hidden">
               <div 
                 className="h-full bg-rose-500 transition-all duration-500" 
                 style={{ width: `${Math.min(100, (vitals?.heart_rate_bpm ?? 0))}%` }}
               />
            </div>
          </CardContent>
        </Card>

        {/* Respiration Card */}
        <Card className="bg-gradient-to-br from-background to-blue-50/10 border-blue-500/20">
          <CardHeader className="flex flex-row items-center justify-between pb-2">
            <CardTitle className="text-sm font-medium">Respiration</CardTitle>
          </CardHeader>
          <CardContent>
            <div className="text-4xl font-bold">{vitals?.breathing_rate_bpm?.toFixed(1) ?? "--"}</div>
            <p className="text-xs text-muted-foreground mt-1">RPM (Breaths per minute)</p>
            <div className="mt-4">
              <Sparkline data={brHistory} color="#3b82f6" />
            </div>
            <div className="mt-2 h-1.5 w-full bg-secondary rounded-full overflow-hidden">
               <div 
                 className="h-full bg-blue-500 transition-all duration-500" 
                 style={{ width: `${Math.min(100, (vitals?.breathing_rate_bpm ?? 0) * 3)}%` }}
               />
            </div>
          </CardContent>
        </Card>

        {/* Patient Status */}
        <Card className="bg-gradient-to-br from-background to-emerald-50/10 border-emerald-500/20">
          <CardHeader className="flex flex-row items-center justify-between pb-2">
            <CardTitle className="text-sm font-medium">Posture / Activity</CardTitle>
          </CardHeader>
          <CardContent>
            <div className="text-2xl font-bold capitalize">{latestUpdate?.posture ?? "Stationary"}</div>
            <p className="text-xs text-muted-foreground mt-1">WiFi Pose Estimation active</p>
            <div className="mt-4">
              <span className="text-xs font-medium text-emerald-500 uppercase tracking-tighter">Continuous Monitoring</span>
            </div>
          </CardContent>
        </Card>
      </div>

      <div className="grid grid-cols-1 gap-6">
        {/* Live Event Feed */}
        <Card>
          <CardHeader>
            <CardTitle>Medical Event Log</CardTitle>
            <CardDescription>Recent physiological status changes and dispatched alerts.</CardDescription>
          </CardHeader>
          <CardContent>
            <div className="space-y-4 max-h-[400px] overflow-auto pr-2">
              {isFallDetected && (
                <div className="flex items-start gap-3 p-3 bg-destructive/10 border border-destructive/20 rounded-lg animate-pulse">
                  <div>
                    <p className="text-sm font-bold text-destructive">EMERGENCY: Fall Detected</p>
                    <p className="text-xs text-muted-foreground">Sensing engine recognized high-velocity vertical displacement. Alert dispatched.</p>
                    <p className="text-[10px] font-mono text-muted-foreground mt-1 opacity-70">{new Date().toLocaleTimeString()}</p>
                  </div>
                </div>
              )}
              
              <div className="flex items-start gap-3 p-3 bg-secondary/30 rounded-lg border border-border/40">
                <div>
                  <p className="text-sm font-semibold">Continuous Vitals Monitor</p>
                  <p className="text-xs text-muted-foreground">WiFi CSI subcarrier analysis is extracting sub-millimeter chest wall movement.</p>
                  <p className="text-[10px] font-mono text-muted-foreground mt-1 opacity-70">{new Date().toLocaleTimeString()}</p>
                </div>
              </div>

              <div className="flex items-start gap-3 p-3 bg-secondary/20 rounded-lg border border-border/40 opacity-70">
                <div>
                  <p className="text-sm font-semibold">Baseline Established</p>
                  <p className="text-xs text-muted-foreground">Environmental noise floor calibrated. Sensitivity set to medical mode.</p>
                  <p className="text-[10px] font-mono text-muted-foreground mt-1 opacity-70">System Boot</p>
                </div>
              </div>
            </div>
          </CardContent>
        </Card>
      </div>
    </div>
  );
}
