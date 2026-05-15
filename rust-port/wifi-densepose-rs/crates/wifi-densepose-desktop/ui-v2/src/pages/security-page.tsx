import React, { useState } from "react";
import { Shield, ShieldAlert, ShieldCheck, Users, AlertCircle, Lock, Unlock, Activity } from "lucide-react";
import { useSensingStore } from "@/lib/sensing-store";
import { useEnterpriseSettings } from "@/lib/enterprise-store";
import { Card, CardContent, CardHeader, CardTitle, CardDescription } from "@/components/ui/card";
import { Badge } from "@/components/ui/badge";
import { Switch } from "@/components/ui/switch";
import { Label } from "@/components/ui/label";
import { Button } from "@/components/ui/button";

export function SecurityPage() {
  const { latestUpdate } = useSensingStore();
  const { settings, saveSettings } = useEnterpriseSettings();

  const handleToggleArmed = async () => {
    if (settings) {
      const newSettings = { ...settings, security_armed: !settings.security_armed };
      await saveSettings(newSettings);
    }
  };

  const presence = latestUpdate?.classification?.presence;
  const persons = latestUpdate?.estimated_persons ?? 0;
  const isBreach = settings?.security_armed && presence;
  const isOverCapacity = persons > (settings?.crowd_threshold ?? 0);

  return (
    <div className="space-y-6 p-6 max-w-7xl mx-auto">
      <div className="flex justify-between items-center">
        <div>
          <h1 className="text-3xl font-bold tracking-tight">Security Center</h1>
          <p className="text-muted-foreground">Privacy-first RF intrusion and occupancy monitoring.</p>
        </div>
        <div className="flex items-center gap-4">
          <Badge variant={settings?.security_armed ? "default" : "outline"} className="px-4 py-1">
            {settings?.security_armed ? "SYSTEM ARMED" : "SYSTEM DISARMED"}
          </Badge>
          <Button 
            variant={settings?.security_armed ? "destructive" : "default"}
            onClick={handleToggleArmed}
          >
            {settings?.security_armed ? "Disarm System" : "Arm System"}
          </Button>
        </div>
      </div>

      {isBreach && (
        <div className="bg-destructive text-destructive-foreground p-4 rounded-xl flex items-center gap-4 animate-bounce shadow-2xl shadow-destructive/50">
          <div className="flex-1">
            <h2 className="text-xl font-bold uppercase tracking-widest">Intrusion Detected</h2>
            <p className="text-sm opacity-90">Unauthorized presence recognized in the secured zone. Alert dispatched.</p>
          </div>
          <Badge variant="outline" className="bg-white/20 border-white/40 text-white">IMMEDIATE</Badge>
        </div>
      )}

      <div className="grid grid-cols-1 md:grid-cols-2 gap-6">
        {/* Occupancy Card */}
        <Card className={isOverCapacity ? "border-amber-500 bg-amber-500/5" : ""}>
          <CardHeader className="flex flex-row items-center justify-between pb-2">
            <CardTitle className="text-sm font-medium uppercase tracking-tight">Occupancy</CardTitle>
          </CardHeader>
          <CardContent>
            <div className="flex items-baseline gap-2">
              <span className="text-4xl font-bold">{persons}</span>
              <span className="text-muted-foreground text-xs">/ {settings?.crowd_threshold} MAX</span>
            </div>
            <p className="text-[10px] text-muted-foreground mt-1 uppercase">Estimated persons in range</p>
            <div className="mt-4 space-y-2">
              <div className="h-2 w-full bg-secondary rounded-full overflow-hidden">
                <div 
                  className={`h-full transition-all duration-1000 ${isOverCapacity ? "bg-amber-500" : "bg-primary"}`}
                  style={{ width: `${Math.min(100, (persons / (settings?.crowd_threshold || 1)) * 100)}%` }}
                />
              </div>
              {isOverCapacity && (
                <div className="text-amber-600 dark:text-amber-400 text-[10px] font-bold uppercase tracking-tighter">
                  Capacity limit exceeded
                </div>
              )}
            </div>
          </CardContent>
        </Card>

        {/* Status Card */}
        <Card>
          <CardHeader className="flex flex-row items-center justify-between pb-2">
            <CardTitle className="text-sm font-medium uppercase tracking-tight">Zone Integrity</CardTitle>
          </CardHeader>
          <CardContent>
            <div className="text-2xl font-bold uppercase">{presence ? "Active Motion" : "Zone Secure"}</div>
            <p className="text-[10px] text-muted-foreground mt-1 uppercase">
              {presence ? "RF disturbance detected" : "No unauthorized movement"}
            </p>
            <div className="mt-4 flex flex-wrap gap-2">
              <Badge variant="secondary" className="text-[9px] uppercase">RF Sensing</Badge>
              <Badge variant="secondary" className="text-[9px] uppercase">No Cameras</Badge>
              <Badge variant="secondary" className="text-[9px] uppercase">Privacy First</Badge>
            </div>
          </CardContent>
        </Card>
      </div>
    </div>
  );
}
