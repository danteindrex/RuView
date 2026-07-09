/**
 * Step A — Hub check (W3).
 *
 * Confirms the local sensing server is reachable, shows the resolved ports, and
 * detects the classic :8080 conflict: if something answers on the HTTP port but
 * `GET /api/v1/nodes` 404s, another app (e.g. taha-backend) likely owns it, so
 * we warn and point the operator at Settings to change the sensing HTTP port.
 */

import { useCallback, useEffect, useState } from "react";
import { CheckCircle2, Loader2, RefreshCw, ServerCog } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";
import { StatusLine, StepShell } from "../step-shell";
import type { StepProps } from "../step-shell";
import { fetchServerNodes, HttpError, probeHealth, resolveServerBase } from "../server-api";

type Verdict = "checking" | "ok" | "port-conflict" | "unreachable" | "not-running";

export function HubCheckStep({ serverStatus, onRefreshServer, onNext: _onNext }: StepProps) {
  const [verdict, setVerdict] = useState<Verdict>("checking");
  const [nodeCount, setNodeCount] = useState<number | null>(null);
  const [refreshing, setRefreshing] = useState(false);
  // Bumped by "Re-check" so the probe re-runs even when server_status is
  // unchanged (e.g. a persistent port conflict the user hasn't fixed yet).
  const [nonce, setNonce] = useState(0);

  const base = resolveServerBase(serverStatus);

  const runCheck = useCallback(
    async (signal: AbortSignal) => {
      setVerdict("checking");
      setNodeCount(null);
      if (!serverStatus?.running) {
        if (!signal.aborted) setVerdict("not-running");
        return;
      }
      if (!base) {
        if (!signal.aborted) setVerdict("unreachable");
        return;
      }
      const healthy = await probeHealth(base, signal);
      if (signal.aborted) return;
      try {
        const nodes = await fetchServerNodes(base, signal);
        if (signal.aborted) return;
        setNodeCount(nodes.length);
        setVerdict("ok");
      } catch (err) {
        if (signal.aborted) return;
        // Server answers but the sensing routes 404 → something else owns the port.
        if (err instanceof HttpError && err.status === 404) {
          setVerdict("port-conflict");
        } else {
          setVerdict(healthy ? "port-conflict" : "unreachable");
        }
      }
    },
    [base, serverStatus?.running],
  );

  useEffect(() => {
    const controller = new AbortController();
    void runCheck(controller.signal);
    return () => controller.abort();
  }, [runCheck, nonce]);

  async function handleRefresh() {
    setRefreshing(true);
    try {
      await onRefreshServer?.();
    } finally {
      setRefreshing(false);
      setNonce((n) => n + 1);
    }
  }

  return (
    <StepShell
      title="Check your hub"
      description="Wave runs a local sensing server that your nodes stream to. Let's confirm it's up and reachable."
    >
      <div className="space-y-4">
        <div className="rounded-lg border border-border/60 bg-secondary/20 p-3">
          <div className="mb-2 flex items-center gap-2 text-sm font-medium">
            <ServerCog className="h-4 w-4 text-primary" /> Sensing server
          </div>
          <div className="grid grid-cols-2 gap-2 text-xs sm:grid-cols-4">
            <PortStat label="HTTP" value={serverStatus?.http_port} />
            <PortStat label="WebSocket" value={serverStatus?.ws_port} />
            <PortStat label="UDP (ESP32)" value={serverStatus?.udp_port} />
            <PortStat label="Nexmon (Pi)" value={serverStatus?.nexmon_port} />
          </div>
          <div className="mt-2 flex flex-wrap items-center gap-2">
            <Badge variant={serverStatus?.running ? "success" : "danger"}>
              {serverStatus?.running ? "running" : "stopped"}
            </Badge>
            {base ? <Badge variant="outline">{base}</Badge> : null}
            {nodeCount !== null ? <Badge variant="secondary">{nodeCount} node(s) known</Badge> : null}
          </div>
        </div>

        {verdict === "checking" ? (
          <StatusLine tone="muted">
            <Loader2 className="mr-2 inline h-4 w-4 animate-spin" /> Probing /health and /api/v1/nodes…
          </StatusLine>
        ) : null}

        {verdict === "ok" ? (
          <StatusLine tone="ok">
            <CheckCircle2 className="mr-2 inline h-4 w-4" /> Hub is reachable and answering sensing routes. You're ready to add nodes.
          </StatusLine>
        ) : null}

        {verdict === "not-running" ? (
          <StatusLine tone="warn">
            The sensing server isn't running yet. Start it from the 3D Pose or System Admin page, then re-check.
          </StatusLine>
        ) : null}

        {verdict === "unreachable" ? (
          <StatusLine tone="error">
            Couldn't reach the server at its bind address. Confirm the ports in Settings and that the server process is alive.
          </StatusLine>
        ) : null}

        {verdict === "port-conflict" ? (
          <StatusLine tone="warn">
            Something is answering on this port but it isn't the Wave sensing API. Another app likely owns port{" "}
            {serverStatus?.http_port ?? "?"} — change the sensing HTTP port in Settings → Runtime and Transport, restart the
            server, then re-check.
          </StatusLine>
        ) : null}

        <div className="flex flex-wrap gap-2">
          <Button variant="outline" size="sm" disabled={refreshing} onClick={handleRefresh}>
            {refreshing ? <Loader2 className="mr-2 h-4 w-4 animate-spin" /> : <RefreshCw className="mr-2 h-4 w-4" />}
            Re-check
          </Button>
        </div>
      </div>
    </StepShell>
  );
}

function PortStat({ label, value }: { label: string; value: number | null | undefined }) {
  return (
    <div className="rounded-md border border-border/50 bg-background/40 px-2 py-1.5">
      <p className="text-[10px] uppercase tracking-wider text-muted-foreground">{label}</p>
      <p className="font-mono text-sm">{value ?? "—"}</p>
    </div>
  );
}
