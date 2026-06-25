import { useCallback, useEffect, useState } from "react";
import { useAuthStore } from "@/lib/auth-store";
import { authApi, type Tenant } from "@/lib/auth-api";
import { PageSection } from "@/components/layout/page-section";
import { Badge } from "@/components/ui/badge";
import { CreateTenantDialog } from "@/components/tenants/create-tenant-dialog";
import { AssignModulesDialog } from "@/components/tenants/assign-modules-dialog";

interface TenantsPageProps {
  onNavigateToUsers: (tenantId: string) => void;
}

export function TenantsPage({ onNavigateToUsers }: TenantsPageProps) {
  const { accessToken, user } = useAuthStore();
  const [tenants, setTenants] = useState<Tenant[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState("");
  const [showCreate, setShowCreate] = useState(false);
  const [selectedTenant, setSelectedTenant] = useState<Tenant | null>(null);
  const [dialogMode, setDialogMode] = useState<"modules" | null>(null);

  const isSuperAdmin = user?.scope === "global";

  const fetchTenants = useCallback(async () => {
    if (!accessToken) return;
    setLoading(true);
    setError("");
    console.log("[TenantsPage] Fetching tenants...");
    try {
      const data = await authApi.listTenants(accessToken);
      console.log("[TenantsPage] Data received:", data);
      setTenants(data);
    } catch (err) {
      console.error("[TenantsPage] Fetch error:", err);
      setError(String(err));
    } finally {
      setLoading(false);
    }
  }, [accessToken]);

  useEffect(() => {
    fetchTenants();
  }, [fetchTenants]);

  async function handleDelete(id: string, name: string) {
    if (!accessToken || !window.confirm(`Are you sure you want to delete tenant "${name}"? All associated data will be purged.`)) return;
    try {
      await authApi.deleteTenant(accessToken, id);
      fetchTenants();
    } catch (err) {
      setError(String(err));
    }
  }

  if (loading) {
    return (
      <div className="flex h-full items-center justify-center py-40">
        <div className="flex flex-col items-center gap-3">
          <div className="h-5 w-5 border-2 border-primary border-t-transparent rounded-full animate-spin"></div>
          <span className="text-sm text-muted-foreground font-medium">Scanning clusters...</span>
        </div>
      </div>
    );
  }

  return (
    <div className="space-y-6">
      <div className="flex items-center justify-between">
        <div>
          <h2 className="text-lg font-semibold tracking-tight">Tenant Oversight</h2>
          <p className="text-sm text-muted-foreground">Manage organizational boundaries and installation tenancy</p>
        </div>
        {isSuperAdmin && (
          <button 
            onClick={() => setShowCreate(true)}
            className="h-9 rounded-lg bg-primary px-6 text-xs font-semibold text-primary-foreground transition-all hover:bg-primary/90 hover:scale-[1.02] active:scale-[0.98]">
            New Tenant
          </button>
        )}
      </div>

      {error && (
        <div className="rounded-lg bg-destructive/10 p-4 text-sm text-destructive border border-destructive/20">
          <p className="font-semibold mb-1">Error Loading Tenants</p>
          <p>{error}</p>
        </div>
      )}

      {tenants.length === 0 && !error && (
        <div className="rounded-xl border border-dashed border-border/60 p-12 text-center">
          <p className="text-sm text-muted-foreground">No tenants registered in this installation.</p>
        </div>
      )}

      <div className="grid gap-4 md:grid-cols-2 lg:grid-cols-3">
        {tenants.map((tenant) => (
          <div key={tenant.id} className="group relative overflow-hidden rounded-xl border border-border/60 bg-card/60 p-5 backdrop-blur transition-all hover:border-primary/40 hover:bg-card/80">
            <div className="flex items-start justify-between">
              <div className="space-y-1">
                <h3 className="font-semibold text-foreground">{tenant.name}</h3>
                <p className="text-[10px] text-muted-foreground font-mono uppercase tracking-wider">{tenant.id}</p>
              </div>
              <Badge variant={tenant.verification_status === "verified" ? "success" : "warning"}>
                {tenant.verification_status?.toUpperCase() ?? "ACTIVE"}
              </Badge>
            </div>

            <div className="mt-4 space-y-2 border-t border-border/40 pt-4">
              <div className="flex items-center justify-between text-[11px]">
                <span className="text-muted-foreground">Industry</span>
                <span className="font-medium">{tenant.industry ?? "General Tech"}</span>
              </div>
              <div className="flex items-center justify-between text-[11px]">
                <span className="text-muted-foreground">Type</span>
                <span className="font-medium text-primary">{tenant.type ?? "COMPANY"}</span>
              </div>
            </div>

            <div className="mt-6 flex gap-2 opacity-0 transition-all translate-y-1 group-hover:opacity-100 group-hover:translate-y-0">
               <button 
                 onClick={() => { setSelectedTenant(tenant); setDialogMode("modules"); }}
                 className="flex-1 rounded-md bg-primary/10 py-2 text-xs font-semibold text-primary hover:bg-primary/20 transition-colors">
                 Modules
               </button>
               <button 
                 onClick={() => onNavigateToUsers(tenant.id)}
                 className="flex-1 rounded-md bg-secondary/50 py-2 text-xs font-semibold text-foreground hover:bg-secondary/80 transition-colors">
                 Users
               </button>
               {isSuperAdmin && (
                 <button 
                   onClick={() => handleDelete(tenant.id, tenant.name)}
                   className="rounded-md bg-destructive/5 px-3 py-2 text-xs font-semibold text-destructive hover:bg-destructive/15 transition-colors">
                   Del
                 </button>
               )}
            </div>
          </div>
        ))}
      </div>

      {showCreate && <CreateTenantDialog onClose={() => setShowCreate(false)} onSuccess={fetchTenants} />}
      {dialogMode === "modules" && selectedTenant && (
        <AssignModulesDialog tenant={selectedTenant} onClose={() => { setSelectedTenant(null); setDialogMode(null); }} onSuccess={fetchTenants} />
      )}

      <PageSection title="Local Tenancy Architecture" description="Isolation boundaries in Wave Desktop.">
        <div className="rounded-lg bg-background/40 p-4 text-xs text-muted-foreground leading-relaxed border border-border/30">
          In this desktop deployment, each installation represents a single root tenant. Super Admins can manage 
          the local tenancy, define roles with granular Access Matrix permissions, and setup cross-departmental 
          users.
        </div>
      </PageSection>
    </div>
  );
}
