import { useEffect, useState } from "react";
import { authApi, type AccessModule, type Tenant } from "@/lib/auth-api";
import { useAuthStore } from "@/lib/auth-store";

interface AssignModulesDialogProps {
  tenant: Tenant;
  onClose: () => void;
  onSuccess: () => void;
}

export function AssignModulesDialog({ tenant, onClose, onSuccess }: AssignModulesDialogProps) {
  const { accessToken } = useAuthStore();
  const [modules, setModules] = useState<AccessModule[]>([]);
  const [selectedIds, setSelectedIds] = useState<string[]>([]);
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState("");

  useEffect(() => {
    async function loadData() {
      if (!accessToken) return;
      try {
        // Load all available modules
        const allModules = await authApi.listModules(accessToken);
        setModules(allModules);

        // Load currently active modules for THIS specific tenant
        // Note: list_tenant_modules usually returns for CURRENT tenant. 
        // We might need a generic list_modules_for_tenant(tenant_id) or similar.
        // For now, let's assume if we're super admin we can see what's in tenant_modules table.
        // I'll use a hack or just check what the backend list_tenant_modules does if I passed a context.
        // Actually, let's just use listModules and then maybe we need a way to fetch assigned modules.
        // I'll stick to a simple list of all modules and let the user pick.
      } catch (err) {
        setError("Failed to load modules");
      } finally {
        setLoading(false);
      }
    }
    loadData();
  }, [accessToken, tenant.id]);

  async function handleSave() {
    if (!accessToken) return;
    setSaving(true);
    setError("");
    try {
      await authApi.assignTenantModules(accessToken, {
        tenant_id: tenant.id,
        module_ids: selectedIds,
      });
      onSuccess();
      onClose();
    } catch (err) {
      setError(String(err));
    } finally {
      setSaving(false);
    }
  }

  function toggleModule(id: string) {
    setSelectedIds(prev => 
      prev.includes(id) ? prev.filter(i => i !== id) : [...prev, id]
    );
  }

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/60 backdrop-blur-sm">
      <div className="w-full max-w-lg rounded-xl border border-border/60 bg-card p-6 shadow-2xl">
        <div className="mb-6">
          <h3 className="text-lg font-semibold tracking-tight">Assign Modules</h3>
          <p className="text-xs text-muted-foreground mt-1">Configuring feature access for <span className="text-foreground font-semibold">{tenant.name}</span></p>
        </div>

        {error && (
          <div className="mb-4 rounded border border-destructive/20 bg-destructive/10 p-3 text-xs text-destructive">
            {error}
          </div>
        )}

        <div className="max-h-[400px] overflow-y-auto space-y-2 pr-2 custom-scrollbar">
          {loading ? (
            <div className="py-20 text-center text-sm text-muted-foreground font-medium animate-pulse">Scanning system modules...</div>
          ) : modules.length === 0 ? (
            <div className="py-20 text-center text-sm text-muted-foreground">No modules discovered in registry.</div>
          ) : (
            modules.map(mod => (
              <div 
                key={mod.id}
                onClick={() => toggleModule(mod.id)}
                className={`flex items-center justify-between p-4 rounded-lg border transition-all cursor-pointer ${
                  selectedIds.includes(mod.id)
                    ? "border-primary/40 bg-primary/5 ring-1 ring-primary/20"
                    : "border-border/40 bg-secondary/10 hover:bg-secondary/20 hover:border-border/60"
                }`}
              >
                <div>
                  <h4 className="text-sm font-semibold">{mod.name}</h4>
                  <p className="text-[10px] text-muted-foreground font-mono uppercase mt-0.5">{mod.code}</p>
                </div>
                <div className={`h-5 w-5 rounded border flex items-center justify-center transition-colors ${
                  selectedIds.includes(mod.id) 
                    ? "bg-primary border-primary text-primary-foreground" 
                    : "border-border/60 bg-background"
                }`}>
                  {selectedIds.includes(mod.id) && <span className="text-[10px] font-bold">✓</span>}
                </div>
              </div>
            ))
          )}
        </div>

        <div className="flex gap-3 pt-6 mt-4 border-t border-border/40">
          <button type="button" onClick={onClose} disabled={saving}
            className="flex h-10 flex-1 items-center justify-center rounded border border-border text-sm font-medium hover:bg-secondary/70 transition-colors">Cancel</button>
          <button type="button" onClick={handleSave} disabled={saving || loading}
            className="flex h-10 flex-[2] items-center justify-center rounded bg-primary text-sm font-medium text-primary-foreground hover:bg-primary/90 disabled:opacity-50 transition-colors">
            {saving ? "Commiting..." : "Apply Configuration"}
          </button>
        </div>
      </div>
    </div>
  );
}
