/**
 * Tour — lightweight anchored-tooltip walkthrough (W3, step E).
 *
 * Renders a non-modal tooltip layer OUTSIDE the wizard Dialog so it can point
 * at real page elements (dashboard, 3D view, medical page). Each stop may name
 * a `page` — the parent (App.tsx via the onboarding store) navigates there, and
 * an `anchor` CSS selector the tooltip positions next to. If the anchor is not
 * found (page not mounted, element renamed) the tooltip falls back to centered,
 * so a missing selector degrades gracefully instead of vanishing.
 */

import { useEffect, useLayoutEffect, useState } from "react";
import { Button } from "@/components/ui/button";
import type { TourPage } from "@/lib/onboarding-store";

export interface TourStop {
  title: string;
  body: string;
  /** Page the app should be showing for this stop. */
  page: Exclude<TourPage, null>;
  /** CSS selector to anchor the tooltip to (best-effort). */
  anchor?: string;
}

interface TourProps {
  stops: TourStop[];
  onFinish: () => void;
  onSkip: () => void;
  /** Called whenever the active stop changes so the app can navigate. */
  onNavigate: (page: Exclude<TourPage, null>) => void;
}

interface AnchorRect {
  top: number;
  left: number;
  width: number;
  height: number;
}

export function Tour({ stops, onFinish, onSkip, onNavigate }: TourProps) {
  const [index, setIndex] = useState(0);
  const stop = stops[Math.max(0, Math.min(stops.length - 1, index))];
  const [rect, setRect] = useState<AnchorRect | null>(null);

  // Navigate the app to this stop's page whenever the active stop changes.
  useEffect(() => {
    if (stop) onNavigate(stop.page);
  }, [stop, onNavigate]);

  // Locate the anchor element. Retry briefly because the target page mounts
  // async after navigation; give up (centered fallback) if it never appears.
  useLayoutEffect(() => {
    if (!stop?.anchor) {
      setRect(null);
      return;
    }
    let cancelled = false;
    let attempts = 0;
    const measure = () => {
      if (cancelled) return;
      const el = document.querySelector(stop.anchor!);
      if (el) {
        const r = el.getBoundingClientRect();
        setRect({ top: r.top, left: r.left, width: r.width, height: r.height });
        el.scrollIntoView({ behavior: "smooth", block: "center", inline: "nearest" });
      } else if (attempts < 20) {
        attempts += 1;
        window.setTimeout(measure, 150);
      } else {
        setRect(null); // fall back to centered
      }
    };
    measure();
    window.addEventListener("resize", measure);
    return () => {
      cancelled = true;
      window.removeEventListener("resize", measure);
    };
  }, [stop]);

  if (!stop) return null;

  const isLast = index >= stops.length - 1;

  // Position the card near the anchor (below it) or centered as a fallback.
  const cardStyle: React.CSSProperties = rect
    ? {
        position: "fixed",
        top: Math.min(rect.top + rect.height + 12, window.innerHeight - 220),
        left: Math.min(Math.max(rect.left, 16), window.innerWidth - 360),
        zIndex: 60,
      }
    : {
        position: "fixed",
        top: "50%",
        left: "50%",
        transform: "translate(-50%, -50%)",
        zIndex: 60,
      };

  return (
    <div className="pointer-events-none fixed inset-0 z-[55]">
      {/* Dim + highlight ring around the anchor, if located. */}
      {rect ? (
        <div
          className="pointer-events-none fixed rounded-lg ring-2 ring-primary ring-offset-2 ring-offset-background transition-all"
          style={{ top: rect.top - 4, left: rect.left - 4, width: rect.width + 8, height: rect.height + 8, zIndex: 56 }}
        />
      ) : (
        <div className="pointer-events-none fixed inset-0 bg-background/60" />
      )}

      <div
        style={cardStyle}
        className="pointer-events-auto w-[340px] max-w-[calc(100vw-2rem)] rounded-lg border border-border bg-background p-4 shadow-xl"
      >
        <div className="mb-2 flex items-center justify-between">
          <p className="text-sm font-semibold">{stop.title}</p>
          <span className="text-[10px] uppercase tracking-wider text-muted-foreground">
            {index + 1} / {stops.length}
          </span>
        </div>
        <p className="text-sm text-muted-foreground">{stop.body}</p>
        <div className="mt-4 flex items-center justify-between">
          <Button variant="ghost" size="sm" onClick={onSkip}>
            Skip tour
          </Button>
          <div className="flex gap-2">
            {index > 0 ? (
              <Button variant="outline" size="sm" onClick={() => setIndex((i) => Math.max(0, i - 1))}>
                Back
              </Button>
            ) : null}
            {isLast ? (
              <Button size="sm" onClick={onFinish}>
                Finish
              </Button>
            ) : (
              <Button size="sm" onClick={() => setIndex((i) => Math.min(stops.length - 1, i + 1))}>
                Next
              </Button>
            )}
          </div>
        </div>
      </div>
    </div>
  );
}
