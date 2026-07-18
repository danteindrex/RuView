import { useCallback, useEffect, useState } from "react";
import { useAuthStore } from "@/lib/auth-store";
import { authApi, type AccessModule, type Role, type RoleWithModules } from "@/lib/auth-api";
import { usePermissions } from "@/hooks/use-permissions";

export function RolesPage() {
  const { accessToken } = useAuthStore();
  const { hasModulePermission } = usePermissions();

  const [roles, setRoles] = useState<Role[]>([]);
  const [modules, setModules] = useState<AccessModule[]>([]);
  const [selectedRole, setSelectedRole] = useState<RoleWithModules | null>(null);
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState("");
  const [success, setSuccess] = useState("");

  // Create role
  const [showCreate, setShowCreate] = useState(false);
  const [newRoleName, setNewRoleName] = useState("");
  const [newRoleDesc, setNewRoleDesc] = useState("");

  // Permission matrix state
  const [matrix, setMatrix] = useState<Record<string, {
    can_read: boolean; can_add: boolean; can_edit: boolean;
    can_delete: boolean; can_approve: boolean;
    read_scope: number; add_scope: number; edit_scope: number;
    delete_scope: number; approve_scope: number;
  }>>({});

  const canAdd = hasModulePermission("user-management", "add");
  const canEdit = hasModulePermission("user-management", "edit");
  const canDelete = hasModulePermission("user-management", "delete");

  const fetchRoles = useCallback(async () => {
    if (!accessToken) return;
    setLoading(true);
    try {
      const [r, m] = await Promise.all([
        authApi.listRoles(accessToken),
        authApi.listTenantModules(accessToken),
      ]);
      setRoles(r);
      setModules(m);
    } catch (err) {
      setError(String(err));
    } finally {
      setLoading(false);
    }
  }, [accessToken]);

  useEffect(() => { fetchRoles(); }, [fetchRoles]);

  async function selectRole(roleId: string) {
    if (!accessToken) return;
    setError("");
    setSuccess("");
    try {
      const role = await authApi.getRole(accessToken, roleId);
      setSelectedRole(role);

      // Build matrix from role's current permissions
      const m: typeof matrix = {};
      for (const mod of modules) {
        const existing = role.modules.find(rm => rm.id === mod.id);
        m[mod.id] = {
          can_read: existing?.permission.view ?? false,
          can_add: existing?.permission.add ?? false,
          can_edit: existing?.permission.edit ?? false,
          can_delete: existing?.permission.deleteAccess ?? false,
          can_approve: existing?.permission.approve ?? false,
          read_scope: existing?.permission.readScoop ?? 0,
          add_scope: existing?.permission.addScoop ?? 0,
          edit_scope: existing?.permission.editScoop ?? 0,
          delete_scope: existing?.permission.deleteScoop ?? 0,
          approve_scope: existing?.permission.approveScoop ?? 0,
        };
      }
      setMatrix(m);
    } catch (err) {
      setError(String(err));
    }
  }

  function togglePermission(moduleId: string, field: string) {
    setMatrix(prev => ({
      ...prev,
      [moduleId]: {
        ...prev[moduleId],
        [field]: !prev[moduleId]?.[field as keyof typeof prev[typeof moduleId]],
      },
    }));
  }

  async function savePermissions() {
    if (!accessToken || !selectedRole) return;
    setSaving(true);
    setError("");
    try {
      const modulePermissions = Object.entries(matrix).map(([moduleId, perms]) => ({
        module_id: moduleId,
        ...perms,
      }));
      await authApi.setRolePermissions(accessToken, {
        role_id: selectedRole.id,
        module_permissions: modulePermissions,
      });
      setSuccess("Permissions saved successfully");
      setTimeout(() => setSuccess(""), 3000);
    } catch (err) {
      setError(String(err));
    } finally {
      setSaving(false);
    }
  }

  async function handleCreateRole(e: React.FormEvent) {
    e.preventDefault();
    if (!accessToken || !newRoleName.trim()) return;
    try {
      await authApi.createRole(accessToken, { name: newRoleName.trim(), description: newRoleDesc });
      setShowCreate(false);
      setNewRoleName("");
      setNewRoleDesc("");
      await fetchRoles();
    } catch (err) {
      setError(String(err));
    }
  }

  async function handleDeleteRole(roleId: string, roleName: string) {
    if (!accessToken) return;
    if (!confirm(`Delete role "${roleName}"? Users with this role will lose their permissions.`)) return;
    try {
      await authApi.deleteRole(accessToken, roleId);
      if (selectedRole?.id === roleId) setSelectedRole(null);
      await fetchRoles();
    } catch (err) {
      setError(String(err));
    }
  }

  const permColumns = [
    { key: "can_read", label: "View" },
    { key: "can_add", label: "Add" },
    { key: "can_edit", label: "Edit" },
    { key: "can_delete", label: "Delete" },
    { key: "can_approve", label: "Approve" },
  ];

  return (
    <div className="space-y-6">
      {/* Header */}
      <div className="flex items-center justify-between">
        <div>
          <h2 className="text-lg font-semibold tracking-tight">Roles & Permissions</h2>
          <p className="text-sm text-muted-foreground">{roles.length} definitions found</p>
        </div>
        {canAdd && (
          <button onClick={() => setShowCreate(true)}
            className="h-9 rounded-lg bg-primary px-6 text-xs font-medium text-primary-foreground transition-colors hover:bg-primary/90">
            New Role
          </button>
        )}
      </div>

      {error && (
        <div className="rounded-lg bg-destructive/10 p-3 text-sm text-destructive border border-destructive/20">
          {error}
        </div>
      )}
      {success && (
        <div className="rounded-lg bg-accent/10 p-3 text-sm text-accent border border-accent/20">
          {success}
        </div>
      )}

      <div className="grid gap-6 lg:grid-cols-[280px_1fr]">
        {/* Role list */}
        <div className="rounded-xl border border-border/60 bg-card/60 p-4 backdrop-blur overflow-hidden">
          <h3 className="mb-4 text-xs font-semibold text-muted-foreground uppercase tracking-wider">Definitions</h3>
          {loading ? (
            <div className="flex justify-center py-20 text-sm text-muted-foreground">
              Polling roles...
            </div>
          ) : (
            <div className="space-y-1">
              {roles.map((role) => (
                <div key={role.id}
                  className={`flex items-center justify-between rounded px-3 py-3 transition-colors cursor-pointer ${
                    selectedRole?.id === role.id
                      ? "bg-primary/15 text-primary ring-1 ring-primary/40"
                      : "hover:bg-secondary/70"
                  }`}>
                  <button onClick={() => selectRole(role.id)} className="flex-1 text-left">
                    <span className="font-semibold text-sm">{role.name}</span>
                    {role.description && <p className="text-xs text-muted-foreground mt-0.5">{role.description}</p>}
                  </button>
                  {canDelete && (
                    <button onClick={() => handleDeleteRole(role.id, role.name)}
                      className="ml-2 rounded px-2 py-1 text-muted-foreground hover:bg-destructive/10 hover:text-destructive text-xs font-medium">
                      Del
                    </button>
                  )}
                </div>
              ))}
            </div>
          )}
        </div>

        {/* Permission matrix */}
        <div className="rounded-xl border border-border/60 bg-card/60 backdrop-blur overflow-hidden">
          {!selectedRole ? (
            <div className="flex items-center justify-center py-40 text-sm text-muted-foreground">
              Select definition to modify access matrix
            </div>
          ) : (
            <>
              <div className="flex items-center justify-between border-b border-border/60 p-4 bg-secondary/10">
                <div>
                  <h3 className="font-semibold text-base">{selectedRole.name}</h3>
                  <p className="text-xs text-muted-foreground font-medium uppercase tracking-wider">Access Matrix — Configuring Local Scope</p>
                </div>
                {canEdit && (
                  <button onClick={savePermissions} disabled={saving}
                    className="h-8 rounded bg-primary px-6 text-xs font-medium text-primary-foreground transition-colors hover:bg-primary/90 disabled:opacity-50">
                    {saving ? "Saving..." : "Commit Changes"}
                  </button>
                )}
              </div>
              <div className="overflow-x-auto">
                <table className="w-full text-sm">
                  <thead>
                    <tr className="border-b border-border/60 text-xs font-semibold text-muted-foreground bg-secondary/20">
                      <th className="px-4 py-3 text-left">Module</th>
                      {permColumns.map((col) => (
                        <th key={col.key} className="px-3 py-3 text-center">{col.label}</th>
                      ))}
                    </tr>
                  </thead>
                  <tbody>
                    {modules.map((mod) => (
                      <tr key={mod.id} className="border-b border-border/30 transition-colors hover:bg-secondary/20">
                        <td className="px-4 py-3 font-medium">{mod.name}</td>
                        {permColumns.map((col) => {
                          const hasPerm = matrix[mod.id]?.[col.key as keyof typeof matrix[string]];
                          const scopeKey = `${col.key.split('_')[1]}_scope` as keyof typeof matrix[string];
                          const scope = matrix[mod.id]?.[scopeKey] as number;

                          return (
                            <td key={col.key} className="px-3 py-3 text-center">
                              <div className="flex flex-col items-center gap-1.5">
                                <button
                                  onClick={() => canEdit && togglePermission(mod.id, col.key)}
                                  disabled={!canEdit}
                                  className={`inline-flex h-7 w-7 items-center justify-center rounded transition-colors text-[10px] font-bold ${
                                    hasPerm
                                      ? "bg-primary/20 text-primary ring-1 ring-primary/40"
                                      : "bg-secondary/40 text-muted-foreground/30"
                                  }`}
                                >
                                  {hasPerm ? "YES" : "NO"}
                                </button>
                                
                                {hasPerm && (
                                  <select
                                    value={scope}
                                    onChange={(e) => {
                                      const newScope = parseInt(e.target.value);
                                      setMatrix(prev => ({
                                        ...prev,
                                        [mod.id]: { ...prev[mod.id], [scopeKey]: newScope }
                                      }));
                                    }}
                                    disabled={!canEdit}
                                    className="h-5 rounded border border-border bg-background/50 px-1 text-[9px] font-bold"
                                  >
                                    <option value={0}>OWN</option>
                                    <option value={1}>TNT</option>
                                  </select>
                                )}
                              </div>
                            </td>
                          );
                        })}
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>
            </>
          )}
        </div>
      </div>

      {/* Create Role Dialog */}
      {showCreate && (
        <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/60 backdrop-blur-sm">
          <div className="w-full max-w-sm rounded-xl border border-border/60 bg-card p-6 shadow-2xl">
            <h3 className="mb-6 font-semibold text-lg tracking-tight">Create Role</h3>
            <form onSubmit={handleCreateRole} className="space-y-4">
              <div className="space-y-1.5">
                <label className="text-xs font-medium text-muted-foreground" htmlFor="role-name">Name</label>
                <input id="role-name" type="text" required value={newRoleName}
                  onChange={(e) => setNewRoleName(e.target.value)}
                  placeholder="e.g., Operator, Viewer"
                  className="h-10 w-full rounded border border-input bg-background/70 px-3 text-sm focus:outline-none focus:ring-1 focus:ring-primary" />
              </div>
              <div className="space-y-1.5">
                <label className="text-xs font-medium text-muted-foreground" htmlFor="role-desc">Description</label>
                <input id="role-desc" type="text" value={newRoleDesc}
                  onChange={(e) => setNewRoleDesc(e.target.value)}
                  className="h-10 w-full rounded border border-input bg-background/70 px-3 text-sm focus:outline-none focus:ring-1 focus:ring-primary" />
              </div>
              <div className="flex gap-3 pt-4">
                <button type="button" onClick={() => setShowCreate(false)}
                  className="flex h-10 flex-1 items-center justify-center rounded border border-border text-sm font-medium hover:bg-secondary/70">Cancel</button>
                <button type="submit"
                  className="flex h-10 flex-[2] items-center justify-center rounded bg-primary text-sm font-medium text-primary-foreground hover:bg-primary/90">
                  Confirm Create
                </button>
              </div>
            </form>
          </div>
        </div>
      )}
    </div>
  );
}
