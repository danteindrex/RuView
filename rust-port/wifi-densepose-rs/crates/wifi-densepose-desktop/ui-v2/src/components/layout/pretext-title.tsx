import type React from "react";
import { useElementWidth, usePretextMetrics } from "@/lib/pretext";
import { cn } from "@/lib/utils";

interface PretextTitleProps {
  text: string;
  className?: string;
  lineHeight?: number;
  maxLines?: number;
  as?: "h1" | "h2" | "h3" | "p" | "span";
}

export function PretextTitle({
  text,
  className,
  lineHeight = 22,
  maxLines = 2,
  as = "h3",
}: PretextTitleProps) {
  const { ref, width } = useElementWidth<HTMLDivElement>();
  const metrics = usePretextMetrics({
    text,
    width,
    lineHeight,
    font: '600 16px "Space Grotesk", "Inter", sans-serif',
  });

  const Element = as as React.ElementType;
  const clampedHeight = Math.min(metrics.height, maxLines * lineHeight);

  return (
    <div ref={ref} className="w-full">
      <Element
        className={cn("overflow-hidden text-balance", className)}
        style={{ minHeight: clampedHeight, maxHeight: clampedHeight, lineHeight: `${lineHeight}px` }}
        title={metrics.lineCount > maxLines ? text : undefined}
      >
        {text}
      </Element>
    </div>
  );
}

