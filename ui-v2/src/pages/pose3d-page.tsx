import React, { useMemo } from "react";
import { ObservatoryHost } from "@/components/observatory/observatory-host";
import type { ServerStatusResponse } from "@/types";

interface Pose3DPageProps {
  status: ServerStatusResponse | null;
  onStatusRefresh: () => Promise<void>;
  theme: "light" | "dark";
}

function buildWsUrl(status: ServerStatusResponse | null): string | null {
  if (!status?.ws_port) return "ws://127.0.0.1:3001/ws/sensing";
  const host = status.bind_address && status.bind_address !== "0.0.0.0" ? status.bind_address : "127.0.0.1";
  return `ws://${host}:${status.ws_port}/ws/sensing`;
}

export const Pose3DPage: React.FC<Pose3DPageProps> = ({ status, theme }) => {
  const wsUrl = useMemo(() => buildWsUrl(status), [status]);

  return (
    <div className="h-full min-h-[720px] overflow-hidden rounded-none">
      <ObservatoryHost mode="live" theme={theme} wsUrl={wsUrl} />
    </div>
  );
};
