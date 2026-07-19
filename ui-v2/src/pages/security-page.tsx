import React, { useState, useEffect } from "react";
import { Shield, ShieldAlert, ShieldCheck, Users, AlertCircle, Lock, Unlock, Activity, Key } from "lucide-react";
import { tauriApi } from "@/lib/tauri-api";
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

  // SSH key management state
  const [sshKeyPem, setSshKeyPem] = useState("");
  const [sshPassphrase, setSshPassphrase] = useState("");
  const [sshKeyConfigured, setSshKeyConfigured] = useState<boolean | null>(null);
  const [sshSaveStatus, setSshSaveStatus] = useState<"idle" | "saving" | "ok" | "error">("idle");
  const [sshSaveError, setSshSaveError] = useState("");

  useEffect(() => {
    tauriApi.hasSshKey().then(setSshKeyConfigured).catch(() => setSshKeyConfigured(false));
  }, []);

  const handleSaveSshKey = async () => {
    setSshSaveStatus("saving");
    setSshSaveError("");
    try {
      await tauriApi.setSshKey(sshKeyPem, sshPassphrase);
      setSshSaveStatus("ok");
      setSshKeyConfigured(true);
      setSshKeyPem("");
      setSshPassphrase("");
    } catch (err: unknown) {
      setSshSaveStatus("error");
      setSshSaveError(err instanceof Error ? err.message : String(err));
    }
  };

  const handleToggleArmed = async () => {
    if (settings) {
      const newSettings = { ...settings, security_armed: !settings.security_armed };
      await saveSettings(newSettings);
    }
  };

  const presence = latestUpdate?.classification?.presence;
  const persons = latestUpdate?.persons;
  const estimatedCount = latestUpdate?.estimated_persons ?? 0;
  const personCount = persons?.length ?? estimatedCount;
  const isBreach = settings?.security_armed && presence;
  const isOverCapacity = personCount > (settings?.crowd_threshold ?? 0);

  // Intruder zone visualization - use first detected person's zone as primary
  const primaryZone = persons && persons.length > 0 ? persons[0].zone : null;
  const primaryPerson = persons && persons.length > 0 ? persons[0] : null;

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

      {/* Intruder Detail Panel - shows when breach detected */}
      {isBreach && primaryPerson && (
        <div className="bg-destructive/10 border border-destructive/30 p-4 rounded-xl">
          <div className="flex items-center gap-3 mb-3">
            <AlertCircle className="w-5 h-5 text-destructive" />
            <h3 className="font-semibold text-destructive">Intruder Details</h3>
          </div>
          <div className="grid grid-cols-2 md:grid-cols-4 gap-4">
            <div>
              <p className="text-[10px] uppercase text-muted-foreground mb-1">Zone</p>
              <p className="font-bold text-lg">{primaryZone ?? "Unknown"}</p>
            </div>
            <div>
              <p className="text-[10px] uppercase text-muted-foreground mb-1">Person ID</p>
              <p className="font-bold text-lg">#{primaryPerson.id}</p>
            </div>
            <div>
              <p className="text-[10px] uppercase text-muted-foreground mb-1">Confidence</p>
              <p className="font-bold text-lg">{(primaryPerson.confidence * 100).toFixed(0)}%</p>
            </div>
            <div>
              <p className="text-[10px] uppercase text-muted-foreground mb-1">Keypoints</p>
              <p className="font-bold text-lg">{primaryPerson.keypoints?.length ?? 0}</p>
            </div>
          </div>
          {primaryPerson.bbox && (
            <div className="mt-3 pt-3 border-t border-destructive/20">
              <p className="text-[10px] uppercase text-muted-foreground mb-2">Bounding Box</p>
              <div className="flex gap-4 text-sm font-mono">
                <span>X: {primaryPerson.bbox.x.toFixed(1)}</span>
                <span>Y: {primaryPerson.bbox.y.toFixed(1)}</span>
                <span>W: {primaryPerson.bbox.width.toFixed(1)}</span>
                <span>H: {primaryPerson.bbox.height.toFixed(1)}</span>
              </div>
            </div>
          )}
        </div>
      )}

      {/* SSH Key Management */}
      <Card>
        <CardHeader className="flex flex-row items-center gap-2 pb-2">
          <Key className="w-4 h-4 text-muted-foreground" />
          <CardTitle className="text-sm font-medium uppercase tracking-tight">SSH Key Management</CardTitle>
          <div className="ml-auto">
            {sshKeyConfigured === null ? null : sshKeyConfigured ? (
              <Badge variant="default" className="bg-green-600 text-white text-[10px]">Key Configured</Badge>
            ) : (
              <Badge variant="secondary" className="text-[10px]">No Key Set</Badge>
            )}
          </div>
        </CardHeader>
        <CardContent className="space-y-3">
          <div>
            <Label htmlFor="ssh-pem" className="text-[10px] uppercase text-muted-foreground mb-1 block">SSH Private Key (PEM)</Label>
            <textarea
              id="ssh-pem"
              rows={6}
              className="w-full rounded-md border border-input bg-background px-3 py-2 text-xs font-mono resize-none focus:outline-none focus:ring-2 focus:ring-ring"
              placeholder="Paste SSH private key PEM..."
              value={sshKeyPem}
              onChange={(e) => setSshKeyPem(e.target.value)}
            />
          </div>
          <div>
            <Label htmlFor="ssh-passphrase" className="text-[10px] uppercase text-muted-foreground mb-1 block">Passphrase (optional)</Label>
            <input
              id="ssh-passphrase"
              type="password"
              className="w-full rounded-md border border-input bg-background px-3 py-2 text-sm focus:outline-none focus:ring-2 focus:ring-ring"
              placeholder="Key passphrase..."
              value={sshPassphrase}
              onChange={(e) => setSshPassphrase(e.target.value)}
            />
          </div>
          <div className="flex items-center gap-3">
            <Button
              onClick={handleSaveSshKey}
              disabled={!sshKeyPem.trim() || sshSaveStatus === "saving"}
              size="sm"
            >
              {sshSaveStatus === "saving" ? "Saving..." : "Save Key"}
            </Button>
            {sshSaveStatus === "ok" && (
              <span className="text-green-600 text-xs font-medium">Key stored successfully.</span>
            )}
            {sshSaveStatus === "error" && (
              <span className="text-destructive text-xs font-medium">{sshSaveError}</span>
            )}
          </div>
        </CardContent>
      </Card>

      <div className="grid grid-cols-1 md:grid-cols-2 gap-6">
        {/* Occupancy Card */}
        <Card className={isOverCapacity ? "border-amber-500 bg-amber-500/5" : ""}>
          <CardHeader className="flex flex-row items-center justify-between pb-2">
            <CardTitle className="text-sm font-medium uppercase tracking-tight">Occupancy</CardTitle>
          </CardHeader>
          <CardContent>
            <div className="flex items-baseline gap-2">
              <span className="text-4xl font-bold">{personCount}</span>
              <span className="text-muted-foreground text-xs">/ {settings?.crowd_threshold} MAX</span>
            </div>
            <p className="text-[10px] text-muted-foreground mt-1 uppercase">Estimated persons in range</p>
            <div className="mt-4 space-y-2">
              <div className="h-2 w-full bg-secondary rounded-full overflow-hidden">
                <div 
                  className={`h-full transition-all duration-1000 ${isOverCapacity ? "bg-amber-500" : "bg-primary"}`}
                  style={{ width: `${Math.min(100, (personCount / (settings?.crowd_threshold || 1)) * 100)}%` }}
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
