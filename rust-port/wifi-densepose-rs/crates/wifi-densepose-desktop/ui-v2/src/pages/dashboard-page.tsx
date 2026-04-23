import { Activity, Network, Server, ShieldCheck } from "lucide-react";
import { MetricCard } from "@/components/layout/metric-card";
import { PageSection } from "@/components/layout/page-section";
import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";
import { Separator } from "@/components/ui/separator";
import type { DiscoveredNode, ServerStatusResponse } from "@/types";

interface DashboardPageProps {
  nodes: DiscoveredNode[];
  serverStatus: ServerStatusResponse | null;
  onRefreshNodes: () => Promise<void>;
  onRefreshServer: () => Promise<void>;
}

export function DashboardPage({ nodes, serverStatus, onRefreshNodes, onRefreshServer }: DashboardPageProps) {
  const online = nodes.filter((node) => node.health === "online").length;
  const degraded = nodes.filter((node) => node.health === "degraded").length;

  return (
    <div className="space-y-6">
      <div className="panel-grid">
        <MetricCard title="Registered Nodes" value={String(nodes.length)} subtitle="Discovery scope" />
        <MetricCard title="Online Nodes" value={String(online)} subtitle={online > 0 ? "Active telemetry" : "No telemetry"} tone={online > 0 ? "success" : "warning"} />
        <MetricCard title="Degraded Nodes" value={String(degraded)} subtitle={degraded > 0 ? "Requires attention" : "No degraded nodes"} tone={degraded > 0 ? "danger" : "success"} />
      </div>

      <PageSection title="Control Plane Status" description="Instant operational state across server runtime and network discovery.">
        <div className="space-y-4">
          <div className="flex flex-wrap gap-2">
            <Badge variant={serverStatus?.running ? "success" : "danger"}>
              <Server className="mr-1 h-3 w-3" />
              {serverStatus?.running ? "Server Running" : "Server Stopped"}
            </Badge>
            <Badge variant={online > 0 ? "success" : "warning"}>
              <Network className="mr-1 h-3 w-3" />
              {online} Online
            </Badge>
            <Badge variant={degraded > 0 ? "danger" : "success"}>
              <Activity className="mr-1 h-3 w-3" />
              {degraded} Degraded
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
            <Button variant="secondary" onClick={onRefreshNodes}>
              Refresh Nodes
            </Button>
            <Button variant="outline" onClick={onRefreshServer}>
              Refresh Server Status
            </Button>
          </div>
        </div>
      </PageSection>

      <PageSection title="Release Controls" description="Live sensing is the default path; demo scenarios are available only as an explicit visual validation mode.">
        <ul className="space-y-2 text-sm text-muted-foreground">
          <li className="flex items-center gap-2">
            <ShieldCheck className="h-4 w-4 text-primary" />
            No emoji/icon glyph shortcuts in operational controls.
          </li>
          <li className="flex items-center gap-2">
            <ShieldCheck className="h-4 w-4 text-primary" />
            Advanced settings grouped under one dedicated settings surface.
          </li>
          <li className="flex items-center gap-2">
            <ShieldCheck className="h-4 w-4 text-primary" />
            3D observability supports live WebSocket data and explicit demo scenarios.
          </li>
        </ul>
      </PageSection>
    </div>
  );
}
