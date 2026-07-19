import React, { useState, useEffect } from "react";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { NetworkPage } from "./network-page";
import { FlashPage } from "./flash-page";
import { OtaPage } from "./ota-page";
import { PiNodesPage } from "./pi-nodes-page";
import { SensingPage } from "./sensing-page";
import { MeshPage } from "./mesh-page";
import { ProvisioningPage } from "./provisioning-page";
import { ModulesPage } from "./modules-page";
import { ConsentGate } from "@/components/consent-gate";
import { invoke } from "@tauri-apps/api/core";
import type { DiscoveredNode, ServerStatusResponse } from "@/types";

interface SysAdminPageProps {
  nodes: DiscoveredNode[];
  onNodesUpdate: (nodes: DiscoveredNode[]) => void;
  serverStatus: ServerStatusResponse | null;
  onRefreshServer: () => Promise<void>;
  onRefreshNodes: () => Promise<void>;
}

export function SysAdminPage({
  nodes, onNodesUpdate, serverStatus, onRefreshServer, onRefreshNodes
}: SysAdminPageProps) {
  const [cloudEnabled, setCloudEnabled] = useState(false);
  const [cloudEndpoint, setCloudEndpoint] = useState("");
  const [consentGranted, setConsentGranted] = useState(false);
  const [showConsentGate, setShowConsentGate] = useState(false);

  useEffect(() => {
    invoke<{ endpoint: string; consent_granted: boolean; enabled: boolean }>("get_cloud_config")
      .then((cfg) => {
        setCloudEndpoint(cfg.endpoint);
        setConsentGranted(cfg.consent_granted);
        setCloudEnabled(cfg.enabled);
      })
      .catch(() => {});
  }, []);

  const handleToggleCloud = (next: boolean) => {
    if (next && !consentGranted) {
      setShowConsentGate(true);
    } else {
      setCloudEnabled(next);
      if (!next) invoke("set_consent", { granted: false }).catch(() => {});
    }
  };

  return (
    <div className="flex flex-col h-full space-y-4">
      {showConsentGate && (
        <ConsentGate
          onConsent={() => { setConsentGranted(true); setCloudEnabled(true); setShowConsentGate(false); }}
          onDecline={() => { setCloudEnabled(false); setShowConsentGate(false); }}
        />
      )}

      <div className="px-6 pt-4">
        <h1 className="text-2xl font-bold tracking-tight">System Administration</h1>
        <p className="text-sm text-muted-foreground">Technical infrastructure and hardware management controls.</p>
      </div>

      <Tabs defaultValue="network" className="flex-1 flex flex-col">
        <div className="px-6 border-b">
          <TabsList className="bg-transparent h-12 gap-6">
            <TabsTrigger value="network" className="data-[state=active]:bg-transparent data-[state=active]:border-b-2 data-[state=active]:border-primary rounded-none px-2 shadow-none">Network</TabsTrigger>
            <TabsTrigger value="pi-nodes" className="data-[state=active]:bg-transparent data-[state=active]:border-b-2 data-[state=active]:border-primary rounded-none px-2 shadow-none">Pi Nodes</TabsTrigger>
            <TabsTrigger value="flash" className="data-[state=active]:bg-transparent data-[state=active]:border-b-2 data-[state=active]:border-primary rounded-none px-2 shadow-none">Flash</TabsTrigger>
            <TabsTrigger value="ota" className="data-[state=active]:bg-transparent data-[state=active]:border-b-2 data-[state=active]:border-primary rounded-none px-2 shadow-none">OTA</TabsTrigger>
            <TabsTrigger value="sensing" className="data-[state=active]:bg-transparent data-[state=active]:border-b-2 data-[state=active]:border-primary rounded-none px-2 shadow-none">Server</TabsTrigger>
            <TabsTrigger value="mesh" className="data-[state=active]:bg-transparent data-[state=active]:border-b-2 data-[state=active]:border-primary rounded-none px-2 shadow-none">Mesh</TabsTrigger>
            <TabsTrigger value="provisioning" className="data-[state=active]:bg-transparent data-[state=active]:border-b-2 data-[state=active]:border-primary rounded-none px-2 shadow-none">Provision</TabsTrigger>
            <TabsTrigger value="modules" className="data-[state=active]:bg-transparent data-[state=active]:border-b-2 data-[state=active]:border-primary rounded-none px-2 shadow-none">Edge Mods</TabsTrigger>
            <TabsTrigger value="cloud" className="data-[state=active]:bg-transparent data-[state=active]:border-b-2 data-[state=active]:border-primary rounded-none px-2 shadow-none">Cloud Sync</TabsTrigger>
          </TabsList>
        </div>

        <div className="flex-1 overflow-auto">
          <TabsContent value="network" className="m-0 p-6 h-full focus-visible:outline-none">
            <NetworkPage nodes={nodes} onNodesUpdate={onNodesUpdate} />
          </TabsContent>
          <TabsContent value="pi-nodes" className="m-0 p-6 h-full focus-visible:outline-none">
            <PiNodesPage />
          </TabsContent>
          <TabsContent value="flash" className="m-0 p-6 h-full focus-visible:outline-none">
            <FlashPage />
          </TabsContent>
          <TabsContent value="ota" className="m-0 p-6 h-full focus-visible:outline-none">
            <OtaPage />
          </TabsContent>
          <TabsContent value="sensing" className="m-0 p-6 h-full focus-visible:outline-none">
            <SensingPage status={serverStatus} onStatusRefresh={onRefreshServer} />
          </TabsContent>
          <TabsContent value="mesh" className="m-0 p-6 h-full focus-visible:outline-none">
            <MeshPage nodes={nodes} onRefreshNodes={onRefreshNodes} />
          </TabsContent>
          <TabsContent value="provisioning" className="m-0 p-6 h-full focus-visible:outline-none">
            <ProvisioningPage />
          </TabsContent>
          <TabsContent value="modules" className="m-0 p-6 h-full focus-visible:outline-none">
            <ModulesPage />
          </TabsContent>
          <TabsContent value="cloud" className="m-0 p-6 h-full focus-visible:outline-none">
            <div className="space-y-6 max-w-lg">
              <div>
                <h2 className="text-base font-semibold mb-1">Cloud Sync</h2>
                <p className="text-sm text-muted-foreground">
                  Upload encrypted sensing sessions to RuView cloud for AI-powered insights.
                </p>
              </div>

              <div className="flex items-center justify-between">
                <div>
                  <p className="text-sm font-medium">Enable Cloud Sync</p>
                  <p className="text-xs text-muted-foreground">Requires consent to transmit data</p>
                </div>
                <button
                  role="switch"
                  aria-checked={cloudEnabled}
                  onClick={() => handleToggleCloud(!cloudEnabled)}
                  className={`relative inline-flex h-6 w-11 items-center rounded-full transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-primary ${cloudEnabled ? "bg-blue-600" : "bg-gray-300 dark:bg-gray-600"}`}
                >
                  <span className={`inline-block h-4 w-4 transform rounded-full bg-white shadow transition-transform ${cloudEnabled ? "translate-x-6" : "translate-x-1"}`} />
                </button>
              </div>

              <div>
                <label className="block text-sm font-medium mb-1" htmlFor="cloud-endpoint">
                  Insight API Endpoint
                </label>
                <input
                  id="cloud-endpoint"
                  type="url"
                  value={cloudEndpoint}
                  onChange={(e) => setCloudEndpoint(e.target.value)}
                  placeholder="http://localhost:8001"
                  className="w-full rounded-md border border-input bg-background px-3 py-2 text-sm shadow-sm focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring"
                />
              </div>

              <div className="rounded-md border px-4 py-3 text-sm">
                <p className="font-medium mb-1">Consent Status</p>
                <p className={consentGranted ? "text-green-600 dark:text-green-400" : "text-muted-foreground"}>
                  {consentGranted ? "Consent granted — data may be uploaded." : "No consent recorded. Enable Cloud Sync to grant consent."}
                </p>
                {consentGranted && (
                  <button
                    onClick={() => { invoke("set_consent", { granted: false }).catch(() => {}); setConsentGranted(false); setCloudEnabled(false); }}
                    className="mt-2 text-xs text-red-600 hover:underline"
                  >
                    Withdraw consent
                  </button>
                )}
              </div>
            </div>
          </TabsContent>
        </div>
      </Tabs>
    </div>
  );
}
