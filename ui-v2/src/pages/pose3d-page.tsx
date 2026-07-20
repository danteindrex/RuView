import React, { useEffect, useMemo, useRef, useState } from "react";
import { AlertTriangle, WifiOff, X } from "lucide-react";
import { ObservatoryHost } from "@/components/observatory/observatory-host";
import {
  fetchServerNodes,
  isNodeOnline,
  nodeIdNumber,
  resolveServerBase,
} from "@/components/onboarding/server-api";
import type { ServerStatusResponse } from "@/types";

interface Pose3DPageProps {
  status: ServerStatusResponse | null;
  onStatusRefresh: () => Promise<void>;
  theme: "light" | "dark";
}

/** A transient disconnect notification. */
interface NodeNotice {
  key: number;
  message: string;
}

/** Poll interval for the node roster (ms). */
const ROSTER_POLL_MS = 3000;
/** How long a disconnect notice stays on screen (ms). */
const NOTICE_TTL_MS = 8000;

function buildWsUrl(status: ServerStatusResponse | null): string | null {
  // Unknown WS port: return null so the observatory's own auto-detect probes
  // (:8765 WS default, :8080 HTTP default) run instead of pinning a dead port.
  if (!status?.ws_port) return null;
  const host = status.bind_address && status.bind_address !== "0.0.0.0" ? status.bind_address : "127.0.0.1";
  return `ws://${host}:${status.ws_port}/ws/sensing`;
}

export const Pose3DPage: React.FC<Pose3DPageProps> = ({ status, theme }) => {
  const wsUrl = useMemo(() => buildWsUrl(status), [status]);
  const base = useMemo(() => resolveServerBase(status), [status]);

  // Whether we have a confirmed roster reading yet (distinguishes "no server /
  // unknown" from "server says zero nodes online" — we only hide the pose when
  // we positively know all nodes are down).
  const [rosterKnown, setRosterKnown] = useState(false);
  const [onlineCount, setOnlineCount] = useState<number | null>(null);
  const [notices, setNotices] = useState<NodeNotice[]>([]);
  // Ids of nodes that were online on the previous successful poll.
  const prevOnlineRef = useRef<Set<number>>(new Set());
  const noticeSeqRef = useRef(0);

  // Poll the sensing-server roster to detect node disconnections. The roster is
  // the source of truth for which nodes are actively streaming; a node that
  // stops streaming flips to "stale" (isNodeOnline == false) and eventually
  // drops out entirely.
  useEffect(() => {
    if (!base) {
      // No reachable server — don't force-hide; let the observatory's own WS
      // auto-detect run. Reset detection state.
      setRosterKnown(false);
      setOnlineCount(null);
      prevOnlineRef.current = new Set();
      return;
    }

    let cancelled = false;

    const pushNotice = (message: string) => {
      const key = ++noticeSeqRef.current;
      setNotices((cur) => [...cur, { key, message }]);
      window.setTimeout(() => {
        if (!cancelled) setNotices((cur) => cur.filter((n) => n.key !== key));
      }, NOTICE_TTL_MS);
    };

    const poll = async () => {
      let nodes;
      try {
        nodes = await fetchServerNodes(base);
      } catch {
        // Transient fetch failure — keep the last known state, do NOT flip to
        // "all disconnected" on a single network blip.
        return;
      }
      if (cancelled) return;

      const online = new Set<number>();
      for (const n of nodes) {
        const id = nodeIdNumber(n);
        if (id !== null && isNodeOnline(n)) online.add(id);
      }

      // Detect nodes that were online last poll and are no longer online.
      const prev = prevOnlineRef.current;
      for (const id of prev) {
        if (!online.has(id)) {
          pushNotice(`Node ${id} disconnected — live pose accuracy is reduced.`);
        }
      }

      prevOnlineRef.current = online;
      setOnlineCount(online.size);
      setRosterKnown(true);
    };

    void poll();
    const interval = window.setInterval(() => void poll(), ROSTER_POLL_MS);
    return () => {
      cancelled = true;
      window.clearInterval(interval);
    };
  }, [base]);

  const dismiss = (key: number) => setNotices((cur) => cur.filter((n) => n.key !== key));

  // Only hide the pose once we positively know every node is down. Without a
  // node streaming, the pose is derived from stale/no data and is not accurate.
  const allDown = rosterKnown && onlineCount === 0;

  return (
    <div className="relative h-full min-h-[720px] overflow-hidden rounded-none">
      <ObservatoryHost mode="live" theme={theme} wsUrl={wsUrl} />

      {/* Disconnect notifications (top-right stack). */}
      <div className="pointer-events-none absolute right-4 top-4 z-20 flex w-80 max-w-[calc(100%-2rem)] flex-col gap-2">
        {notices.map((n) => (
          <div
            key={n.key}
            className="pointer-events-auto flex items-start gap-2 rounded-lg border border-amber-500/40 bg-amber-950/90 px-3 py-2 text-sm text-amber-100 shadow-lg backdrop-blur"
          >
            <AlertTriangle className="mt-0.5 h-4 w-4 shrink-0 text-amber-400" />
            <span className="flex-1">{n.message}</span>
            <button
              onClick={() => dismiss(n.key)}
              className="shrink-0 rounded p-0.5 text-amber-300/70 hover:text-amber-100"
              aria-label="Dismiss"
            >
              <X className="h-4 w-4" />
            </button>
          </div>
        ))}
      </div>

      {/* All nodes down: cover the (inaccurate) pose rather than unmount the
          three.js scene, so it resumes instantly when a node reconnects. */}
      {allDown ? (
        <div className="absolute inset-0 z-10 flex flex-col items-center justify-center gap-4 bg-background/95 text-center backdrop-blur-sm">
          <WifiOff className="h-14 w-14 text-muted-foreground" />
          <div className="space-y-1">
            <h2 className="text-xl font-semibold">No active sensing nodes</h2>
            <p className="max-w-md text-sm text-muted-foreground">
              Live pose is hidden because no node is currently streaming — without a connected node the pose data isn&apos;t
              accurate. It will reappear automatically as soon as a node comes back online.
            </p>
          </div>
        </div>
      ) : null}
    </div>
  );
};
