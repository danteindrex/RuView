/**
 * Step C — Place nodes in 3D (W3).
 *
 * Embeds the observatory the way pose3d-page does, arms placement mode by
 * clicking the observatory's own PLACE NODES button (retrying until it mounts),
 * and overlays a checklist of nodes still missing a position. RANGE NODES (WiFi
 * FTM auto-placement) is surfaced when ≥3 non-Pi nodes are online. The step is
 * "done" once every online node has a position.
 */

import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { CheckCircle2, Circle, Move3d, Radar } from "lucide-react";
import { ObservatoryHost } from "@/components/observatory/observatory-host";
import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";
import { StatusLine, StepShell } from "../step-shell";
import type { StepProps } from "../step-shell";
import {
  fetchNodePositions,
  fetchServerNodes,
  isNodeOnline,
  isPosition3,
  nodeIdNumber,
  resolveSensingWsUrl,
  resolveServerBase,
} from "../server-api";
import type { ServerNode } from "@/types";

function isPiNode(node: ServerNode): boolean {
  return (node.origin ?? "").toLowerCase().includes("nexmon");
}

/**
 * Click an observatory toolbar button by id, scoped to THIS step's own
 * observatory instance. The pose3d page mounts a second observatory behind the
 * modal, so a document-wide getElementById would grab the wrong (background)
 * instance — we query inside the step's container ref instead. Retries until
 * the async-mounted button exists.
 */
function clickScopedButton(container: HTMLElement | null, id: string) {
  let attempts = 0;
  const tryClick = () => {
    const root = container ?? document.body;
    const btn = root.querySelector<HTMLButtonElement>(`#${id}`);
    if (btn && !btn.disabled) {
      btn.click();
      return;
    }
    if (attempts < 20) {
      attempts += 1;
      window.setTimeout(tryClick, 200);
    }
  };
  tryClick();
}

export function PlaceNodesStep({ serverStatus }: StepProps) {
  const [nodes, setNodes] = useState<ServerNode[]>([]);
  // Persisted placement map (id → [x,y,z]) from /api/v1/config/node-positions.
  // Manual drags PUT here rather than into /api/v1/nodes, so a node counts as
  // placed if it has a position field OR an entry in this map.
  const [positions, setPositions] = useState<Record<string, unknown>>({});
  const base = useMemo(() => resolveServerBase(serverStatus), [serverStatus]);
  const wsUrl = useMemo(() => resolveSensingWsUrl(serverStatus), [serverStatus]);
  const armedRef = useRef(false);
  const hostRef = useRef<HTMLDivElement>(null);

  const isPlaced = useCallback(
    (node: ServerNode): boolean => {
      if (isPosition3(node.position)) return true;
      const key = String(nodeIdNumber(node) ?? node.node_id);
      return isPosition3(positions[key]);
    },
    [positions],
  );

  // Poll the node roster + persisted positions so the checklist reflects
  // placements (including manual drags) live.
  useEffect(() => {
    if (!base) return;
    let cancelled = false;
    const controller = new AbortController();
    const poll = async () => {
      const [found, pos] = await Promise.all([
        fetchServerNodes(base, controller.signal).catch(() => null),
        fetchNodePositions(base, controller.signal).catch(() => null),
      ]);
      if (cancelled) return;
      if (found) setNodes(found);
      if (pos) setPositions(pos);
    };
    void poll();
    const interval = window.setInterval(() => void poll(), 3000);
    return () => {
      cancelled = true;
      controller.abort();
      window.clearInterval(interval);
    };
  }, [base]);

  // Arm placement mode once after the observatory mounts.
  const armPlacement = useCallback(() => {
    if (armedRef.current) return;
    armedRef.current = true;
    clickScopedButton(hostRef.current, "place-nodes-btn");
  }, []);

  useEffect(() => {
    const t = window.setTimeout(armPlacement, 800);
    return () => window.clearTimeout(t);
  }, [armPlacement]);

  const onlineNodes = nodes.filter(isNodeOnline);
  const unplaced = onlineNodes.filter((n) => !isPlaced(n));
  const nonPiOnline = onlineNodes.filter((n) => !isPiNode(n));
  const rangeReady = nonPiOnline.length >= 3;
  const allPlaced = onlineNodes.length > 0 && unplaced.length === 0;

  return (
    <StepShell
      title="Place your nodes in 3D"
      description="Drag each node's marker to where the device physically sits in the room. Accurate positions dramatically improve person-location tracking."
    >
      <div className="space-y-3">
        <StatusLine tone="muted">
          <Move3d className="mr-2 inline h-4 w-4" /> Placement mode is armed. Drag markers in the view below, then walk near each
          node and confirm its marker glows to verify it's live.
        </StatusLine>

        <div className="flex flex-wrap items-center gap-2">
          <Button variant="outline" size="sm" onClick={() => clickScopedButton(hostRef.current, "place-nodes-btn")}>
            <Move3d className="mr-2 h-4 w-4" /> Re-arm placement
          </Button>
          {rangeReady ? (
            <Button variant="secondary" size="sm" onClick={() => clickScopedButton(hostRef.current, "range-nodes-btn")}>
              <Radar className="mr-2 h-4 w-4" /> Auto-place with FTM (Range Nodes)
            </Button>
          ) : (
            <Badge variant="outline">Range Nodes needs 3+ online ESP32 nodes ({nonPiOnline.length} now)</Badge>
          )}
        </div>

        {onlineNodes.length === 0 ? (
          <StatusLine tone="warn">No online nodes yet. Once a node streams it appears here for placement.</StatusLine>
        ) : (
          <div className="rounded-lg border border-border/60 bg-secondary/20 p-3">
            <p className="mb-2 text-xs font-semibold uppercase tracking-wider text-muted-foreground">Placement checklist</p>
            <ul className="space-y-1.5">
              {onlineNodes.map((n) => {
                const placed = isPlaced(n);
                return (
                  <li key={String(n.node_id)} className="flex items-center gap-2 text-sm">
                    {placed ? <CheckCircle2 className="h-4 w-4 text-emerald-400" /> : <Circle className="h-4 w-4 text-muted-foreground" />}
                    <span className={placed ? "" : "text-muted-foreground"}>
                      Node {nodeIdNumber(n) ?? n.node_id}
                      {isPiNode(n) ? " (Pi)" : ""}
                    </span>
                    <span className="ml-auto text-xs text-muted-foreground">{placed ? "placed" : "drag me"}</span>
                  </li>
                );
              })}
            </ul>
          </div>
        )}

        {allPlaced ? (
          <StatusLine tone="ok">
            <CheckCircle2 className="mr-2 inline h-4 w-4" /> Every online node has a position. Continue to calibration.
          </StatusLine>
        ) : null}

        <div ref={hostRef} className="h-[420px] overflow-hidden rounded-lg border border-border/60">
          <ObservatoryHost mode="live" theme="dark" wsUrl={wsUrl} />
        </div>
      </div>
    </StepShell>
  );
}
