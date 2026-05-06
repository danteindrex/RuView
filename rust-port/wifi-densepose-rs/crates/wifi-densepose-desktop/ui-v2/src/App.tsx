import { useEffect, useMemo, useState } from "react";
import { LicenseActivationDialog } from "@/components/auth/license-dialog";
import { AppShell, type ShellPage } from "@/components/layout/app-shell";
import { DashboardPage } from "@/pages/dashboard-page";
import { NetworkPage } from "@/pages/network-page";
import { FlashPage } from "@/pages/flash-page";
import { OtaPage } from "@/pages/ota-page";
import { ModulesPage } from "@/pages/modules-page";
import { PiNodesPage } from "@/pages/pi-nodes-page";
import { SensingPage } from "@/pages/sensing-page";
import { ProvisioningPage } from "@/pages/provisioning-page";
import { Pose3DPage } from "@/pages/pose3d-page";
import { MeshPage } from "@/pages/mesh-page";
import { SettingsPage } from "@/pages/settings-page";
import { LicenseActivationPage } from "@/pages/license-page";
import { LoginPage } from "@/pages/login-page";
import { UsersPage } from "@/pages/users-page";
import { RolesPage } from "@/pages/roles-page";
import { TenantsPage } from "@/pages/tenants-page";
import { tauriApi } from "@/lib/tauri-api";
import { useAuthStore } from "@/lib/auth-store";
import { usePermissions } from "@/hooks/use-permissions";
import type { DiscoveredNode, ServerStatusResponse } from "@/types";
import { 
  LayoutDashboard, Network, Zap, CloudUpload, Puzzle, 
  Cpu, Activity, Share2, Settings2, Box, Settings, 
  Users, ShieldCheck, Building2 
} from "lucide-react";

type PageId =
  | "dashboard" | "network" | "flash" | "ota" | "modules"
  | "pi-nodes" | "sensing" | "mesh" | "provisioning" | "pose3d"
  | "settings" | "users" | "roles" | "tenants";

/** All pages — filtered at runtime by permission hook */
const ALL_PAGES: ShellPage[] = [
  { id: "dashboard", label: "Overview", icon: LayoutDashboard },
  { id: "network", label: "Network", icon: Network },
  { id: "flash", label: "Firmware Flash", icon: Zap },
  { id: "ota", label: "OTA Rollout", icon: CloudUpload },
  { id: "modules", label: "Edge Modules", icon: Puzzle },
  { id: "pi-nodes", label: "Pi Nodes", icon: Cpu },
  { id: "sensing", label: "Sensing Server", icon: Activity },
  { id: "mesh", label: "Mesh View", icon: Share2 },
  { id: "provisioning", label: "Provisioning", icon: Settings2 },
  { id: "pose3d", label: "3D Pose", icon: Box },
  { id: "settings", label: "Settings", icon: Settings },
  { id: "users", label: "Users", icon: Users },
  { id: "roles", label: "Roles", icon: ShieldCheck },
  { id: "tenants", label: "Tenants", icon: Building2 },
];

function loadTheme(): "light" | "dark" {
  const stored = localStorage.getItem("wave-v2-theme");
  return stored === "light" ? "light" : "dark";
}

export default function App() {
  const { stage, initialize, user, isSuperAdmin, logout, license } = useAuthStore();
  const { isSectionVisible } = usePermissions();

  const [activePage, setActivePage] = useState<PageId>("dashboard");
  const [navigationTenantId, setNavigationTenantId] = useState<string | undefined>(undefined);
  const [isLicenseModalOpen, setIsLicenseModalOpen] = useState(false);
  const [theme, setTheme] = useState<"light" | "dark">(loadTheme);
  const [nodes, setNodes] = useState<DiscoveredNode[]>([]);
  const [serverStatus, setServerStatus] = useState<ServerStatusResponse | null>(null);

  // Initialize auth system on mount
  useEffect(() => {
    initialize();
  }, [initialize]);

  // Filter pages based on permissions
  const visiblePages = useMemo(() => {
    return ALL_PAGES.filter((page) => isSectionVisible(page.id));
  }, [isSectionVisible]);

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
    localStorage.setItem("wave-v2-theme", theme);
  }, [theme]);

  // Only poll when in dashboard stage
  useEffect(() => {
    if (stage !== "dashboard") return;
    void refreshNodes().catch(() => undefined);
    void refreshServer().catch(() => undefined);
    const interval = setInterval(() => {
      void refreshNodes().catch(() => undefined);
      void refreshServer().catch(() => undefined);
    }, 8000);
    return () => clearInterval(interval);
  }, [stage]);

  const title = useMemo(() => {
    const page = ALL_PAGES.find((entry) => entry.id === activePage);
    return page?.label ?? "Overview";
  }, [activePage]);

  const subtitle = "Production command center for Wave sensing, firmware, mesh, and observability controls.";

  // ─── 3-Stage Rendering ──────────────────────────────────────────────────

  // Stage 0: Loading
  if (stage === "loading") {
    return (
      <div className="flex h-full w-full items-center justify-center bg-background">
        <div className="flex flex-col items-center gap-4">
          <div className="h-1 w-48 overflow-hidden rounded-full bg-secondary">
            <div className="h-full w-1/3 animate-[loading_2s_infinite] bg-primary" />
          </div>
          <p className="text-[10px] font-bold uppercase tracking-[0.2em] text-muted-foreground">Initializing Wave Desktop...</p>
        </div>
      </div>
    );
  }

  // Stage 1: License activation
  if (stage === "license") {
    return <LicenseActivationPage />;
  }

  // Stage 2: Login
  if (stage === "login") {
    return <LoginPage />;
  }

  // Stage 3: Dashboard (with permission-filtered sidebar)
  return (
    <AppShell
      pages={visiblePages}
      activePage={activePage}
      onPageChange={(id) => {
        setActivePage(id as PageId);
        if (id !== "users") setNavigationTenantId(undefined); // Reset filter when leaving users page
      }}
      title={title}
      subtitle={subtitle}
      serverRunning={Boolean(serverStatus?.running)}
      onlineNodes={nodes.filter((node) => node.health === "online").length}
      totalNodes={nodes.length}
      theme={theme}
      onThemeToggle={() => setTheme((current) => (current === "dark" ? "light" : "dark"))}
      immersive={activePage === "pose3d"}
      user={user}
      isSuperAdmin={isSuperAdmin}
      onLogout={logout}
      isLicensed={license?.is_licensed ?? false}
      onActivateLicense={() => {
        setActivePage("dashboard");
        setIsLicenseModalOpen(true);
      }}
    >
      <LicenseActivationDialog 
        open={isLicenseModalOpen} 
        onOpenChange={setIsLicenseModalOpen} 
      />
      {activePage === "dashboard" ? (
        <DashboardPage 
          nodes={nodes} 
          serverStatus={serverStatus} 
          onRefreshNodes={refreshNodes} 
          onRefreshServer={refreshServer} 
          license={license}
          isLicenseModalOpen={isLicenseModalOpen}
          onLicenseModalChange={setIsLicenseModalOpen}
        />
      ) : null}
      {activePage === "network" ? <NetworkPage nodes={nodes} onNodesUpdate={setNodes} /> : null}
      {activePage === "flash" ? <FlashPage /> : null}
      {activePage === "ota" ? <OtaPage /> : null}
      {activePage === "modules" ? <ModulesPage /> : null}
      {activePage === "pi-nodes" ? <PiNodesPage /> : null}
      {activePage === "sensing" ? <SensingPage status={serverStatus} onStatusRefresh={refreshServer} /> : null}
      {activePage === "mesh" ? <MeshPage nodes={nodes} onRefreshNodes={refreshNodes} /> : null}
      {activePage === "provisioning" ? <ProvisioningPage /> : null}
      {activePage === "pose3d" ? <Pose3DPage status={serverStatus} onStatusRefresh={refreshServer} /> : null}
      {activePage === "settings" ? <SettingsPage theme={theme} onThemeChange={setTheme} /> : null}
      {activePage === "users" ? <UsersPage tenantId={navigationTenantId} /> : null}
      {activePage === "roles" ? <RolesPage /> : null}
      {activePage === "tenants" ? (
        <TenantsPage 
          onNavigateToUsers={(tid) => {
            setNavigationTenantId(tid);
            setActivePage("users");
          }} 
        />
      ) : null}
    </AppShell>
  );
}
