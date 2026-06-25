import React, { useState } from "react";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { NetworkPage } from "./network-page";
import { FlashPage } from "./flash-page";
import { OtaPage } from "./ota-page";
import { PiNodesPage } from "./pi-nodes-page";
import { SensingPage } from "./sensing-page";
import { MeshPage } from "./mesh-page";
import { ProvisioningPage } from "./provisioning-page";
import { ModulesPage } from "./modules-page";
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
  return (
    <div className="flex flex-col h-full space-y-4">
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
        </div>
      </Tabs>
    </div>
  );
}
