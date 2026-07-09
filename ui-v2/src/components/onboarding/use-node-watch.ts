/**
 * useNodeWatch — poll `GET /api/v1/nodes` until a target node id appears (W3).
 *
 * Used by both the ESP32 and Pi setup flows after provisioning to confirm the
 * new node actually streams. Cleans up its interval on unmount / when disabled.
 */

import { useEffect, useRef, useState } from "react";
import { fetchServerNodes, nodeIdNumber } from "./server-api";
import type { ServerNode } from "@/types";

export type WatchState = "idle" | "watching" | "found" | "timeout";

interface UseNodeWatchArgs {
  base: string | null;
  /** Node id to wait for. When null, the hook stays idle. */
  targetId: number | null;
  /** Optional predicate to further qualify the appearing node (e.g. origin). */
  predicate?: (node: ServerNode) => boolean;
  /** Poll interval (ms). */
  intervalMs?: number;
  /** Give up after this many ms (0 = never). */
  timeoutMs?: number;
  enabled: boolean;
}

interface UseNodeWatchResult {
  state: WatchState;
  found: ServerNode | null;
  elapsedSecs: number;
}

export function useNodeWatch({
  base,
  targetId,
  predicate,
  intervalMs = 2500,
  timeoutMs = 120000,
  enabled,
}: UseNodeWatchArgs): UseNodeWatchResult {
  const [state, setState] = useState<WatchState>("idle");
  const [found, setFound] = useState<ServerNode | null>(null);
  const [elapsedSecs, setElapsedSecs] = useState(0);
  const predicateRef = useRef(predicate);
  predicateRef.current = predicate;

  useEffect(() => {
    if (!enabled || !base || targetId === null) {
      setState("idle");
      setFound(null);
      setElapsedSecs(0);
      return;
    }

    let cancelled = false;
    const controller = new AbortController();
    const start = Date.now();
    setState("watching");
    setFound(null);

    const tick = async () => {
      if (cancelled) return;
      setElapsedSecs(Math.floor((Date.now() - start) / 1000));
      try {
        const nodes = await fetchServerNodes(base, controller.signal);
        if (cancelled) return;
        const match = nodes.find((n) => {
          if (nodeIdNumber(n) !== targetId) return false;
          return predicateRef.current ? predicateRef.current(n) : true;
        });
        if (match) {
          setFound(match);
          setState("found");
          cancelled = true;
          controller.abort();
          window.clearInterval(interval);
          return;
        }
      } catch {
        // transient — keep polling
      }
      if (timeoutMs > 0 && Date.now() - start > timeoutMs) {
        setState("timeout");
        cancelled = true;
        controller.abort();
        window.clearInterval(interval);
      }
    };

    void tick();
    const interval = window.setInterval(() => void tick(), intervalMs);

    return () => {
      cancelled = true;
      controller.abort();
      window.clearInterval(interval);
    };
  }, [base, targetId, enabled, intervalMs, timeoutMs]);

  return { state, found, elapsedSecs };
}
