import { useEffect, useMemo, useRef, useState } from "react";
import { layout, prepare } from "@chenglou/pretext";

export function useElementWidth<T extends HTMLElement>() {
  const ref = useRef<T | null>(null);
  const [width, setWidth] = useState(0);

  useEffect(() => {
    const target = ref.current;
    if (!target) return;

    const observer = new ResizeObserver((entries) => {
      const entry = entries[0];
      if (!entry) return;
      setWidth(entry.contentRect.width);
    });

    observer.observe(target);
    return () => observer.disconnect();
  }, []);

  return { ref, width };
}

export function usePretextMetrics(options: {
  text: string;
  font?: string;
  width: number;
  lineHeight: number;
}) {
  const { text, width, lineHeight, font = "500 14px \"Space Grotesk\", \"Inter\", sans-serif" } = options;

  const prepared = useMemo(() => {
    if (!text) return null;
    try {
      return prepare(text, font, { whiteSpace: "normal" });
    } catch {
      return null;
    }
  }, [text, font]);

  return useMemo(() => {
    if (!prepared || width <= 0) {
      return { lineCount: 1, height: lineHeight };
    }

    try {
      const measured = layout(prepared, width, lineHeight);
      return {
        lineCount: measured.lineCount,
        height: measured.height,
      };
    } catch {
      return { lineCount: 1, height: lineHeight };
    }
  }, [prepared, width, lineHeight]);
}

