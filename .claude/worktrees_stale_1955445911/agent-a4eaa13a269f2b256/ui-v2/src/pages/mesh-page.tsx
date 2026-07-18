import { useEffect, useMemo, useRef, useState } from "react";
import { Inspect, RefreshCw } from "lucide-react";
import { JsonViewer } from "@/components/layout/json-viewer";
import { PageSection } from "@/components/layout/page-section";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import type { DiscoveredNode } from "@/types";

interface MeshPageProps {
  nodes: DiscoveredNode[];
  onRefreshNodes: () => Promise<void>;
}

interface GraphNode {
  node: DiscoveredNode;
  x: number;
  y: number;
  radius: number;
}

interface GraphEdge {
  from: string;
  to: string;
}

const CANVAS_HEIGHT = 520;

function nodeKey(node: DiscoveredNode) {
  return node.mac ?? node.ip;
}

function nodeColor(node: DiscoveredNode) {
  if (node.health === "online") return "hsl(166 81% 45%)";
  if (node.health === "degraded") return "hsl(42 92% 56%)";
  return "hsl(0 73% 51%)";
}

function buildGraph(nodes: DiscoveredNode[], width: number): { graphNodes: GraphNode[]; edges: GraphEdge[] } {
  const centerX = width / 2;
  const centerY = CANVAS_HEIGHT / 2;
  const radius = Math.min(width, CANVAS_HEIGHT) * 0.34;
  const sorted = [...nodes].sort((a, b) => nodeKey(a).localeCompare(nodeKey(b)));

  const graphNodes = sorted.map((node, index) => {
    const angle = sorted.length <= 1 ? -Math.PI / 2 : (index / sorted.length) * Math.PI * 2 - Math.PI / 2;
    const roleScale = node.mesh_role === "coordinator" ? 0 : node.mesh_role === "aggregator" ? 0.58 : 1;
    return {
      node,
      x: centerX + Math.cos(angle) * radius * roleScale,
      y: centerY + Math.sin(angle) * radius * roleScale,
      radius: node.mesh_role === "coordinator" ? 18 : node.mesh_role === "aggregator" ? 15 : 12,
    };
  });

  const coordinators = sorted.filter((node) => node.mesh_role === "coordinator");
  const aggregators = sorted.filter((node) => node.mesh_role === "aggregator");
  const fallbackRoot = coordinators[0] ?? aggregators[0] ?? sorted[0];
  const edges: GraphEdge[] = [];

  for (const node of sorted) {
    if (!fallbackRoot || nodeKey(node) === nodeKey(fallbackRoot)) continue;
    const parent = node.mesh_role === "node" ? aggregators[0] ?? fallbackRoot : fallbackRoot;
    if (nodeKey(parent) !== nodeKey(node)) {
      edges.push({ from: nodeKey(parent), to: nodeKey(node) });
    }
  }

  return { graphNodes, edges };
}

export function MeshPage({ nodes, onRefreshNodes }: MeshPageProps) {
  const canvasRef = useRef<HTMLCanvasElement | null>(null);
  const wrapperRef = useRef<HTMLDivElement | null>(null);
  const [width, setWidth] = useState(900);
  const [selectedKey, setSelectedKey] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);

  const graph = useMemo(() => buildGraph(nodes, width), [nodes, width]);
  const selected = graph.graphNodes.find((entry) => nodeKey(entry.node) === selectedKey)?.node ?? null;
  const online = nodes.filter((node) => node.health === "online").length;
  const coordinators = nodes.filter((node) => node.mesh_role === "coordinator").length;

  useEffect(() => {
    const el = wrapperRef.current;
    if (!el) return;
    const observer = new ResizeObserver(([entry]) => {
      setWidth(Math.max(320, Math.floor(entry.contentRect.width)));
    });
    observer.observe(el);
    return () => observer.disconnect();
  }, []);

  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    const ctx = canvas.getContext("2d");
    if (!ctx) return;

    const dpr = window.devicePixelRatio || 1;
    canvas.width = Math.floor(width * dpr);
    canvas.height = Math.floor(CANVAS_HEIGHT * dpr);
    canvas.style.width = `${width}px`;
    canvas.style.height = `${CANVAS_HEIGHT}px`;
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    ctx.clearRect(0, 0, width, CANVAS_HEIGHT);

    const gridColor = getComputedStyle(document.documentElement).getPropertyValue("--border").trim();
    ctx.strokeStyle = `hsl(${gridColor} / 0.22)`;
    ctx.lineWidth = 1;
    for (let x = 0; x <= width; x += 48) {
      ctx.beginPath();
      ctx.moveTo(x, 0);
      ctx.lineTo(x, CANVAS_HEIGHT);
      ctx.stroke();
    }
    for (let y = 0; y <= CANVAS_HEIGHT; y += 48) {
      ctx.beginPath();
      ctx.moveTo(0, y);
      ctx.lineTo(width, y);
      ctx.stroke();
    }

    const lookup = new Map(graph.graphNodes.map((entry) => [nodeKey(entry.node), entry]));
    ctx.lineWidth = 2;
    for (const edge of graph.edges) {
      const from = lookup.get(edge.from);
      const to = lookup.get(edge.to);
      if (!from || !to) continue;
      const gradient = ctx.createLinearGradient(from.x, from.y, to.x, to.y);
      gradient.addColorStop(0, "hsl(186 100% 57% / 0.8)");
      gradient.addColorStop(1, "hsl(166 81% 45% / 0.45)");
      ctx.strokeStyle = gradient;
      ctx.beginPath();
      ctx.moveTo(from.x, from.y);
      ctx.lineTo(to.x, to.y);
      ctx.stroke();
    }

    for (const entry of graph.graphNodes) {
      const isSelected = nodeKey(entry.node) === selectedKey;
      ctx.beginPath();
      ctx.arc(entry.x, entry.y, entry.radius + (isSelected ? 7 : 4), 0, Math.PI * 2);
      ctx.fillStyle = `${nodeColor(entry.node)}22`;
      ctx.fill();
      ctx.beginPath();
      ctx.arc(entry.x, entry.y, entry.radius, 0, Math.PI * 2);
      ctx.fillStyle = nodeColor(entry.node);
      ctx.fill();
      ctx.lineWidth = isSelected ? 3 : 2;
      ctx.strokeStyle = isSelected ? "hsl(186 100% 57%)" : "hsl(196 28% 92% / 0.75)";
      ctx.stroke();

      ctx.font = "12px ui-monospace, SFMono-Regular, Consolas, monospace";
      ctx.fillStyle = "hsl(196 28% 92%)";
      ctx.textAlign = "center";
      ctx.fillText(String(entry.node.node_id), entry.x, entry.y + 4);

      ctx.font = "12px ui-sans-serif, system-ui";
      ctx.fillStyle = "hsl(205 15% 68%)";
      ctx.fillText(entry.node.hostname ?? entry.node.ip, entry.x, entry.y + entry.radius + 20);
    }
  }, [graph, selectedKey, width]);

  function handleCanvasClick(event: React.MouseEvent<HTMLCanvasElement>) {
    const rect = event.currentTarget.getBoundingClientRect();
    const x = event.clientX - rect.left;
    const y = event.clientY - rect.top;
    const hit = graph.graphNodes.find((entry) => {
      const dx = x - entry.x;
      const dy = y - entry.y;
      return Math.sqrt(dx * dx + dy * dy) <= entry.radius + 10;
    });
    setSelectedKey(hit ? nodeKey(hit.node) : null);
  }

  async function refresh() {
    setLoading(true);
    try {
      await onRefreshNodes();
    } finally {
      setLoading(false);
    }
  }

  return (
    <div className="space-y-6">
      <PageSection title="Mesh Topology" description="Live graph generated from discovered node roles, health, and mesh metadata.">
        <div className="mb-4 flex flex-wrap gap-2">
          <Badge variant={online > 0 ? "success" : "warning"}>{online}/{nodes.length} online</Badge>
          <Badge variant="outline">{coordinators} coordinators</Badge>
          <Badge variant="outline">{graph.edges.length} links</Badge>
          <Button disabled={loading} variant="secondary" size="sm" onClick={refresh}>
            <RefreshCw className="mr-2 h-4 w-4" />
            Refresh
          </Button>
        </div>
        <div ref={wrapperRef} className="overflow-hidden rounded-md border border-border/60 bg-background/70">
          <canvas
            ref={canvasRef}
            className="block cursor-crosshair"
            role="img"
            aria-label="Mesh topology graph"
            onClick={handleCanvasClick}
          />
        </div>
      </PageSection>

      <PageSection title="Selected Node" description="Inspect the selected graph node and its advertised mesh metadata.">
        {selected ? (
          <div className="space-y-3">
            <div className="flex flex-wrap gap-2">
              <Badge variant={selected.health === "online" ? "success" : selected.health === "degraded" ? "warning" : "danger"}>{selected.health}</Badge>
              <Badge variant="outline">{selected.mesh_role}</Badge>
              <Badge variant="outline">{selected.chip}</Badge>
            </div>
            <JsonViewer value={selected} />
          </div>
        ) : (
          <div className="flex items-center gap-2 text-sm text-muted-foreground">
            <Inspect className="h-4 w-4" />
            Select a topology node to inspect its details.
          </div>
        )}
      </PageSection>
    </div>
  );
}
