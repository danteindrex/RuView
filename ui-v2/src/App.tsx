import { useEffect, useMemo, useRef, useState } from "react";
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
import { MedicalPage } from "@/pages/medical-page";
import { SecurityPage } from "@/pages/security-page";
import { SysAdminPage } from "@/pages/sysadmin-page";
import CsiVisionPage from "@/pages/csi-vision-page";
import { Badge } from "@/components/ui/badge";
import { tauriApi } from "@/lib/tauri-api";
import { useAuthStore } from "@/lib/auth-store";
import { usePlanStore } from "@/lib/plan-store";
import { usePermissions } from "@/hooks/use-permissions";
import { SensingProvider } from "@/lib/SensingProvider";
import { OnboardingWizard } from "@/components/onboarding/OnboardingWizard";
import { isOnboardingComplete, ONBOARDING_STEPS, useOnboardingStore } from "@/lib/onboarding-store";
import type { DiscoveredNode, ServerStatusResponse } from "@/types";
import { Shield, ShieldAlert, ShieldCheck, Users, AlertCircle, Settings2, Lock, Unlock, Activity, LayoutDashboard, HeartPulse, Box, Settings, Building2, Terminal, Eye } from "lucide-react";

type PageId =
  | "dashboard" | "medical" | "security" | "pose3d"
  | "settings" | "users" | "roles" | "tenants" | "sysadmin"
  | "csi-vision";

/** All pages — filtered at runtime by permission hook */
const ALL_PAGES: ShellPage[] = [
  { id: "pose3d", label: "3D Pose", icon: Box },
  { id: "dashboard", label: "Overview", icon: LayoutDashboard },
  { id: "csi-vision", label: "CSI Vision", icon: Eye },
  { id: "medical", label: "Medical Hub", icon: HeartPulse },
  { id: "security", label: "Security Center", icon: ShieldAlert },
  { id: "settings", label: "Enterprise Settings", icon: Settings },
  { id: "users", label: "User Management", icon: Users },
  { id: "roles", label: "Access Matrix", icon: ShieldCheck },
  { id: "tenants", label: "Tenancy Oversight", icon: Building2 },
  { id: "sysadmin", label: "System Admin", icon: Terminal },
];

function loadTheme(): "light" | "dark" {
  const stored = localStorage.getItem("wave-v2-theme");
  return stored === "light" ? "light" : "dark";
}

export default function App() {
  const { stage, initialize, user, isSuperAdmin, logout, license, accessToken } = useAuthStore();
  const { isSectionVisible } = usePermissions();
  const { open: onboardingOpen, openAt: openOnboarding, tourPage, step: onboardingStep } = useOnboardingStore();

  const [activePage, setActivePage] = useState<PageId>("pose3d");
  const [navigationTenantId, setNavigationTenantId] = useState<string | undefined>(undefined);
  const [isLicenseModalOpen, setIsLicenseModalOpen] = useState(false);
  const [theme, setTheme] = useState<"light" | "dark">(loadTheme);
  const [nodes, setNodes] = useState<DiscoveredNode[]>([]);
  const [serverStatus, setServerStatus] = useState<ServerStatusResponse | null>(null);

  // Initialize auth system — but only once the Tauri IPC bridge is ready.
  // The bridge (window.__TAURI_INTERNALS__) is injected asynchronously by the
  // Rust webview; calling invoke() before it exists throws the "Cannot read
  // properties of undefined" error we are guarding against here.
  useEffect(() => {
    let cancelled = false;
    
    async function safeInit() {
      // Small delay to ensure bridge injection, but not blocking
      await new Promise(r => setTimeout(r, 200));
      if (!cancelled) {
        try {
          await initialize();
        } catch (e) {
          console.error("Auth initialization failed, retrying...", e);
          setTimeout(() => { if (!cancelled) initialize(); }, 1000);
        }
      }
    }

    void safeInit();
    return () => { cancelled = true; };
  }, [initialize]);

  // Filter pages based on permissions
  const visiblePages = useMemo(() => {
    return ALL_PAGES.filter((page) => isSectionVisible(page.id));
  }, [isSectionVisible]);

  async function refreshNodes() {
    if (!accessToken) return;
    try {
      const discovered = await tauriApi.discoverNodes(accessToken, 2500);
      setNodes(discovered);
    } catch (err) {
      console.error("Failed to refresh nodes:", err);
    }
  }

  async function refreshServer() {
    if (!accessToken) return;
    try {
      const status = await tauriApi.serverStatus(accessToken);
      setServerStatus(status);
    } catch (err) {
      console.error("Failed to refresh server:", err);
    }
  }

  useEffect(() => {
    document.documentElement.classList.toggle("dark", theme === "dark");
    localStorage.setItem("wave-v2-theme", theme);
  }, [theme]);

  // Load plan tier once auth reaches dashboard stage
  useEffect(() => {
    if (stage !== "dashboard") return;
    void usePlanStore.getState().load();
  }, [stage]);

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

  // First-run onboarding: open the wizard the first time the operator reaches
  // the dashboard, unless it was already completed (settings flag or the
  // localStorage mirror). Runs once per dashboard entry.
  const onboardingCheckedRef = useRef(false);
  useEffect(() => {
    if (stage !== "dashboard" || !accessToken || onboardingCheckedRef.current) return;
    onboardingCheckedRef.current = true;
    void (async () => {
      try {
        const settings = await tauriApi.getSettings(accessToken);
        if (!isOnboardingComplete(settings)) {
          openOnboarding(0);
        }
      } catch {
        // Settings unavailable — fall back to the localStorage mirror only.
        if (!isOnboardingComplete(null)) openOnboarding(0);
      }
    })();
  }, [stage, accessToken, openOnboarding]);

  // The tour step navigates the app to specific pages via the onboarding store.
  useEffect(() => {
    if (!tourPage) return;
    const allowed: PageId[] = ["dashboard", "medical", "pose3d"];
    if (allowed.includes(tourPage as PageId)) {
      setActivePage(tourPage as PageId);
    }
  }, [tourPage]);

  // Snapshot the page the operator was on when the wizard opens, and restore it
  // when it closes (so launching setup from any page returns you there).
  const preOnboardingPageRef = useRef<PageId | null>(null);
  useEffect(() => {
    if (onboardingOpen && preOnboardingPageRef.current === null) {
      preOnboardingPageRef.current = activePage;
    } else if (!onboardingOpen && preOnboardingPageRef.current !== null) {
      setActivePage(preOnboardingPageRef.current);
      preOnboardingPageRef.current = null;
    }
  }, [onboardingOpen, activePage]);

  // The observatory HUD binds its toolbar handlers via document-global
  // getElementById, so two live observatories cross-wire (the wizard's Place
  // step embeds its own). While that step is active, keep the background page
  // off pose3d so exactly one observatory exists and placement arms correctly.
  useEffect(() => {
    if (onboardingOpen && ONBOARDING_STEPS[onboardingStep] === "place" && activePage === "pose3d") {
      setActivePage("dashboard");
    }
  }, [onboardingOpen, onboardingStep, activePage]);

  const title = useMemo(() => {
    const page = ALL_PAGES.find((entry) => entry.id === activePage);
    return page?.label ?? "Overview";
  }, [activePage]);

  const subtitle = activePage === "pose3d"
    ? "Primary live observatory for WiFi sensing, pose estimation, and real-time room intelligence."
    : activePage === "csi-vision"
    ? "Generate high-resolution environment images from WiFi CSI using LatentCSI (arXiv:2506.10605)."
    : "Production command center for Wave sensing, firmware, mesh, and observability controls.";

  // ─── 3-Stage Rendering ──────────────────────────────────────────────────

  // Stage 0: Loading
  if (stage === "loading") {
    return (
      <div className="flex h-full w-full items-center justify-center bg-background">
        <div className="flex flex-col items-center gap-4">
          <div className="h-1 w-48 overflow-hidden rounded-full bg-secondary border border-border/20">
            <div className="h-full w-1/3 animate-[loading_2s_infinite] bg-primary" />
          </div>
          <p className="text-[10px] font-bold uppercase tracking-[0.2em] text-muted-foreground animate-pulse">Initializing Wave Desktop...</p>
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
    <SensingProvider>
      {onboardingOpen ? (
        <OnboardingWizard accessToken={accessToken} serverStatus={serverStatus} onRefreshServer={refreshServer} />
      ) : null}
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
      onActivateLicense={() => setIsLicenseModalOpen(true)}
    >
      <LicenseActivationDialog open={isLicenseModalOpen} onOpenChange={setIsLicenseModalOpen} />
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
      {activePage === "csi-vision" ? <CsiVisionPage /> : null}
      {activePage === "medical" ? <MedicalPage /> : null}
      {activePage === "security" ? <SecurityPage /> : null}
      {activePage === "pose3d" ? <Pose3DPage status={serverStatus} onStatusRefresh={refreshServer} theme={theme} /> : null}
      {activePage === "settings" ? <SettingsPage theme={theme} onThemeChange={setTheme} /> : null}
      {activePage === "sysadmin" ? (
        <SysAdminPage 
          nodes={nodes}
          onNodesUpdate={setNodes}
          serverStatus={serverStatus}
          onRefreshServer={refreshServer}
          onRefreshNodes={refreshNodes}
        />
      ) : null}
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
    </SensingProvider>
  );
}
