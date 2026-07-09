/**
 * Step D — Empty-room calibration (W3).
 *
 * Drives the sensing-server calibration API: start with a 90s duration → live
 * countdown from GET status.seconds_remaining → show variance_explained on
 * finish. Degrades gracefully (explains + lets you skip) if the API is missing.
 */

import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { CheckCircle2, DoorOpen, Loader2, Play, Square } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Progress } from "@/components/ui/progress";
import { StatusLine, StepShell } from "../step-shell";
import type { StepProps } from "../step-shell";
import { fetchCalibrationStatus, resolveServerBase, startCalibration, stopCalibration } from "../server-api";
import type { CalibrationStatus } from "@/types";

const DURATION_SECS = 90;

type Phase = "idle" | "running" | "done" | "unavailable";

export function CalibrationStep({ serverStatus }: StepProps) {
  const base = useMemo(() => resolveServerBase(serverStatus), [serverStatus]);
  const [phase, setPhase] = useState<Phase>("idle");
  const [status, setStatus] = useState<CalibrationStatus | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [remaining, setRemaining] = useState(DURATION_SECS);
  const pollRef = useRef<number | null>(null);
  const controllerRef = useRef<AbortController | null>(null);
  const startedAtRef = useRef(0);
  // Guard against a "done" false-positive on the first poll before the server
  // has flipped to running: only finish once we've observed running === true.
  const sawRunningRef = useRef(false);

  // Hard cap so a stuck server (never flips running, or stops responding) can't
  // leave the countdown spinning forever.
  const MAX_WATCH_MS = (DURATION_SECS + 30) * 1000;

  const stopPoll = useCallback(() => {
    if (pollRef.current !== null) {
      window.clearInterval(pollRef.current);
      pollRef.current = null;
    }
    controllerRef.current?.abort();
    controllerRef.current = null;
  }, []);

  useEffect(() => () => stopPoll(), [stopPoll]);

  const poll = useCallback(
    async (controller: AbortController) => {
      if (!base) return;
      try {
        const s = await fetchCalibrationStatus(base, controller.signal);
        setStatus(s);
        if (typeof s.seconds_remaining === "number") setRemaining(Math.max(0, Math.round(s.seconds_remaining)));
        if (s.running === true) sawRunningRef.current = true;
        const finished =
          sawRunningRef.current && s.running === false && (s.seconds_remaining === 0 || s.seconds_remaining == null);
        if (finished) {
          stopPoll();
          setPhase("done");
          return;
        }
      } catch {
        // transient — keep polling; a persistent failure surfaces via the cap below
      }
      if (Date.now() - startedAtRef.current > MAX_WATCH_MS) {
        stopPoll();
        // If we at least saw it running, treat as done; otherwise surface unavailable.
        setPhase(sawRunningRef.current ? "done" : "unavailable");
      }
    },
    [base, stopPoll, MAX_WATCH_MS],
  );

  async function handleStart() {
    setError(null);
    if (!base) {
      setPhase("unavailable");
      return;
    }
    try {
      await startCalibration(base, DURATION_SECS);
      sawRunningRef.current = false;
      startedAtRef.current = Date.now();
      setPhase("running");
      setRemaining(DURATION_SECS);
      stopPoll();
      const controller = new AbortController();
      controllerRef.current = controller;
      pollRef.current = window.setInterval(() => void poll(controller), 1500);
      void poll(controller);
    } catch {
      setPhase("unavailable");
    }
  }

  async function handleStop() {
    if (!base) return;
    try {
      await stopCalibration(base);
    } catch {
      // ignore — best effort
    }
    stopPoll();
    setPhase("done");
  }

  const variance = typeof status?.variance_explained === "number" ? status.variance_explained : null;
  const progressPct = ((DURATION_SECS - remaining) / DURATION_SECS) * 100;

  return (
    <StepShell
      title="Calibrate the empty room"
      description="Wave learns what an empty room looks like so it can tell when someone enters. This takes about 90 seconds."
    >
      <div className="space-y-4">
        <div className="rounded-lg border border-primary/40 bg-primary/5 p-4">
          <div className="flex items-center gap-2 text-sm font-medium">
            <DoorOpen className="h-4 w-4 text-primary" /> Everyone leave the room (or stand at the very edge) before starting.
          </div>
        </div>

        {phase === "running" ? (
          <div className="space-y-2">
            <div className="flex items-center justify-between text-sm">
              <span className="flex items-center gap-2">
                <Loader2 className="h-4 w-4 animate-spin text-primary" /> Calibrating…
              </span>
              <span className="font-mono text-lg">{remaining}s</span>
            </div>
            <Progress value={progressPct} />
          </div>
        ) : null}

        {phase === "done" ? (
          <StatusLine tone="ok">
            <CheckCircle2 className="mr-2 inline h-4 w-4" /> Baseline captured.
            {variance !== null ? ` Variance explained: ${(variance * 100).toFixed(1)}%.` : ""} You're ready for the tour.
          </StatusLine>
        ) : null}

        {phase === "unavailable" ? (
          <StatusLine tone="warn">
            Calibration isn't available right now (the field model may be disabled or the server isn't reachable). You can skip
            this step — sensing still works, it just adapts more slowly.
          </StatusLine>
        ) : null}

        {error ? <StatusLine tone="error">{error}</StatusLine> : null}

        <StatusLine tone="muted">
          Note: your ESP32 nodes also self-calibrate presence for 60 seconds after power-on, so give freshly powered nodes a
          moment before relying on presence.
        </StatusLine>

        <div className="flex flex-wrap gap-2">
          {phase !== "running" ? (
            <Button onClick={handleStart}>
              <Play className="mr-2 h-4 w-4" /> {phase === "done" ? "Re-run calibration" : "Start 90s calibration"}
            </Button>
          ) : (
            <Button variant="outline" onClick={handleStop}>
              <Square className="mr-2 h-4 w-4" /> Stop early
            </Button>
          )}
        </div>
      </div>
    </StepShell>
  );
}
