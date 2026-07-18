import { prettyJson } from "@/lib/utils";

interface JsonViewerProps {
  value: unknown;
  maxHeight?: number;
}

export function JsonViewer({ value, maxHeight = 320 }: JsonViewerProps) {
  return (
    <pre
      className="overflow-auto rounded-md border border-border/60 bg-background/70 p-3 text-xs leading-relaxed text-muted-foreground"
      style={{ maxHeight }}
    >
      {prettyJson(value)}
    </pre>
  );
}

