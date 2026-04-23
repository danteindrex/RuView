import React, { useMemo } from "react";
import type { ServerStatusResponse } from "@/types";

interface Pose3DPageProps {
  status: ServerStatusResponse | null;
  onStatusRefresh: () => Promise<void>;
}

function buildWsUrl(status: ServerStatusResponse | null): string | null {
  if (!status?.ws_port) return "ws://127.0.0.1:3001/ws/sensing";
  const host = status.bind_address && status.bind_address !== "0.0.0.0" ? status.bind_address : "127.0.0.1";
  return `ws://${host}:${status.ws_port}/ws/sensing`;
}

export const Pose3DPage: React.FC<Pose3DPageProps> = ({ status }) => {
  const observatorySrc = useMemo(() => {
    const wsUrl = buildWsUrl(status);
    return wsUrl ? `/observatory.html?mode=live&wsUrl=${encodeURIComponent(wsUrl)}` : "/observatory.html?mode=live";
  }, [status]);

  return (
    <div className="overflow-hidden bg-black" style={{ height: "100%", minHeight: 720 }}>
      <iframe
        key={observatorySrc}
        src={observatorySrc}
        title="Wave Observatory"
        className="border-0 bg-black"
        style={{ width: "100%", height: "100%", display: "block" }}
      />
    </div>
  );
};
