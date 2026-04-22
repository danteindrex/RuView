import { useEffect, useMemo, useState } from "react";
import {
  Blocks,
  Gauge,
  Network,
  Radar,
  RadioTower,
  Settings2,
  Sparkles,
  Upload,
  Wrench,
} from "lucide-react";
import { AppShell, type ShellPage } from "@/components/layout/app-shell";
import { DashboardPage } from "@/pages/dashboard-page";
import { NetworkPage } from "@/pages/network-page";
import { FlashPage } from "@/pages/flash-page";
import { OtaPage } from "@/pages/ota-page";
import { ModulesPage } from "@/pages/modules-page";
import { SensingPage } from "@/pages/sensing-page";
import { ProvisioningPage } from "@/pages/provisioning-page";
import { Pose3DPage } from "@/pages/pose3d-page";
import { SettingsPage } from "@/pages/settings-page";
import { tauriApi } from "@/lib/tauri-api";
import type { DiscoveredNode, ServerStatusResponse } from "@/types";

type PageId = "dashboard" | "network" | "flash" | "ota" | "modules" | "sensing" | "provisioning" | "pose3d" | "settings";

const PAGES: ShellPage[] = [
  { id: "dashboard", label: "Overview", icon: Gauge },
  { id: "network", label: "Network", icon: Network },
  { id: "flash", label: "Firmware Flash", icon: Upload },
  { id: "ota", label: "OTA Rollout", icon: RadioTower },
  { id: "modules", label: "Edge Modules", icon: Blocks },
  { id: "sensing", label: "Sensing Server", icon: Radar },
  { id: "provisioning", label: "Provisioning", icon: Wrench },
  { id: "pose3d", label: "3D Pose", icon: Sparkles },
  { id: "settings", label: "Settings", icon: Settings2 },
];

function loadTheme(): "light" | "dark" {
  const stored = localStorage.getItem("ruview-v2-theme");
  return stored === "light" ? "light" : "dark";
}

export default function App() {
  const [activePage, setActivePage] = useState<PageId>("dashboard");
  const [theme, setTheme] = useState<"light" | "dark">(loadTheme);
  const [nodes, setNodes] = useState<DiscoveredNode[]>([]);
  const [serverStatus, setServerStatus] = useState<ServerStatusResponse | null>(null);

  async function refreshNodes() {
    const discovered = await tauriApi.discoverNodes(2500);
    setNodes(discovered);
  }

  async function refreshServer() {
    const status = await tauriApi.serverStatus();
    setServerStatus(status);
  }

  useEffect(() => {
    document.documentElement.classList.toggle("dark", theme === "dark");
    localStorage.setItem("ruview-v2-theme", theme);
  }, [theme]);

  useEffect(() => {
    void refreshNodes().catch(() => undefined);
    void refreshServer().catch(() => undefined);
    const interval = setInterval(() => {
      void refreshNodes().catch(() => undefined);
      void refreshServer().catch(() => undefined);
    }, 8000);
    return () => clearInterval(interval);
  }, []);

  const title = useMemo(() => {
    const page = PAGES.find((entry) => entry.id === activePage);
    return page?.label ?? "Overview";
  }, [activePage]);

  const subtitle = "Production command center with deterministic state, grouped advanced controls, and no simulated production paths.";

  return (
    <AppShell
      pages={PAGES}
      activePage={activePage}
      onPageChange={(id) => setActivePage(id as PageId)}
      title={title}
      subtitle={subtitle}
      serverRunning={Boolean(serverStatus?.running)}
      onlineNodes={nodes.filter((node) => node.health === "online").length}
      totalNodes={nodes.length}
      theme={theme}
      onThemeToggle={() => setTheme((current) => (current === "dark" ? "light" : "dark"))}
    >
      {activePage === "dashboard" ? <DashboardPage nodes={nodes} serverStatus={serverStatus} onRefreshNodes={refreshNodes} onRefreshServer={refreshServer} /> : null}
      {activePage === "network" ? <NetworkPage nodes={nodes} onNodesUpdate={setNodes} /> : null}
      {activePage === "flash" ? <FlashPage /> : null}
      {activePage === "ota" ? <OtaPage /> : null}
      {activePage === "modules" ? <ModulesPage /> : null}
      {activePage === "sensing" ? <SensingPage status={serverStatus} onStatusRefresh={refreshServer} /> : null}
      {activePage === "provisioning" ? <ProvisioningPage /> : null}
      {activePage === "pose3d" ? <Pose3DPage /> : null}
      {activePage === "settings" ? <SettingsPage theme={theme} onThemeChange={setTheme} /> : null}
    </AppShell>
  );
}

