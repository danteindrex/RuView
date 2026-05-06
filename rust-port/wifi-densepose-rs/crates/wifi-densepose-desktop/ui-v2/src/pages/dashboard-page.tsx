import { useState } from "react";
import { MetricCard } from "@/components/layout/metric-card";
import { PageSection } from "@/components/layout/page-section";
import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";
import { Separator } from "@/components/ui/separator";
import type { DiscoveredNode, ServerStatusResponse } from "@/types";
import type { LicenseStatus } from "@/lib/auth-api";

interface DashboardPageProps {
  nodes: DiscoveredNode[];
  serverStatus: ServerStatusResponse | null;
  onRefreshNodes: () => Promise<void>;
  onRefreshServer: () => Promise<void>;
  license: LicenseStatus | null;
  isLicenseModalOpen: boolean;
  onLicenseModalChange: (open: boolean) => void;
}

export function DashboardPage({ 
  nodes, 
  serverStatus, 
  onRefreshNodes, 
  onRefreshServer, 
  license,
  isLicenseModalOpen,
  onLicenseModalChange
}: DashboardPageProps) {
  const online = nodes.filter((node) => node.health === "online").length;
  const degraded = nodes.filter((node) => node.health === "degraded").length;
  const isLicensed = license?.is_licensed ?? false;

  return (
    <div className="space-y-6">
      <div className="panel-grid">
        <MetricCard title="Registered Nodes" value={String(nodes.length)} subtitle="Discovery scope" />
        <MetricCard title="Online Nodes" value={String(online)} subtitle={online > 0 ? "Active telemetry" : "No telemetry"} tone={online > 0 ? "success" : "warning"} />
      </div>

      <PageSection title="Control Plane Status" description="Instant operational state across server runtime and network discovery.">
        <div className="space-y-4">
          <div className="flex flex-wrap gap-2">
            <Badge variant={serverStatus?.running ? "success" : "danger"}>
              {serverStatus?.running ? "SERVER: RUNNING" : "SERVER: STOPPED"}
            </Badge>
            <Badge variant={online > 0 ? "success" : "warning"}>
              {online} NODES ONLINE
            </Badge>
            <Badge variant={degraded > 0 ? "danger" : "success"}>
              {degraded} DEGRADED
            </Badge>
          </div>

          <Separator />

          <div className="grid gap-3 md:grid-cols-3">
            <div className="rounded-md border border-border/60 bg-background/60 p-3 text-sm">
              <p className="text-muted-foreground">Server PID</p>
              <p className="font-medium">{serverStatus?.pid ?? "N/A"}</p>
            </div>
            <div className="rounded-md border border-border/60 bg-background/60 p-3 text-sm">
              <p className="text-muted-foreground">HTTP Port</p>
              <p className="font-medium">{serverStatus?.http_port ?? "N/A"}</p>
            </div>
            <div className="rounded-md border border-border/60 bg-background/60 p-3 text-sm">
              <p className="text-muted-foreground">Uptime</p>
              <p className="font-medium">{serverStatus?.uptime_secs ? `${serverStatus.uptime_secs}s` : "N/A"}</p>
            </div>
          </div>

          <div className="flex flex-wrap gap-2">
            <Button variant="secondary" onClick={onRefreshNodes} className="text-xs font-medium px-4">
              Refresh Nodes
            </Button>
            <Button variant="outline" onClick={onRefreshServer} className="text-xs font-medium px-4">
              Refresh Server
            </Button>
          </div>
        </div>
      </PageSection>

      <PageSection title="Operational Guidelines" description="Sensing telemetry is the primary data path. All interface controls are optimized for high-density administrative oversight.">
        <ul className="space-y-2 text-xs text-muted-foreground list-disc pl-4">
          <li>System utilizes a text-only interface to maximize information density.</li>
          <li>Management surfaces (Users, Roles, Tenants) are gated by role-based access.</li>
          <li>Performance metrics are polled at 8-second intervals across the local node grid.</li>
        </ul>
      </PageSection>
    </div>
  );
}
