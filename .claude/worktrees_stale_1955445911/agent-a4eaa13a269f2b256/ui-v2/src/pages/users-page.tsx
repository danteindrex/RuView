import { useCallback, useEffect, useState } from "react";
import { useAuthStore } from "@/lib/auth-store";
import { authApi, type AuthUser, type Role } from "@/lib/auth-api";
import { usePermissions } from "@/hooks/use-permissions";

interface UsersPageProps {
  tenantId?: string;
}

export function UsersPage({ tenantId }: UsersPageProps) {
  const { accessToken } = useAuthStore();
  const { hasModulePermission } = usePermissions();

  const [users, setUsers] = useState<AuthUser[]>([]);
  const [roles, setRoles] = useState<Role[]>([]);
  const [total, setTotal] = useState(0);
  const [page, setPage] = useState(0);
  const [search, setSearch] = useState("");
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState("");

  // Create dialog
  const [showCreate, setShowCreate] = useState(false);
  const [createForm, setCreateForm] = useState({
    first_name: "", last_name: "", email: "", password: "", phone: "", role_id: "", tenant_id: tenantId,
  });
  const [creating, setCreating] = useState(false);

  const canAdd = hasModulePermission("user-management", "add");
  const canEdit = hasModulePermission("user-management", "edit");
  const canDelete = hasModulePermission("user-management", "delete");

  const fetchUsers = useCallback(async () => {
    if (!accessToken) return;
    setLoading(true);
    try {
      const resp = await authApi.listUsers(accessToken, page, 25, search || undefined, tenantId);
      setUsers(resp.users);
      setTotal(resp.total);
    } catch (err) {
      setError(String(err));
    } finally {
      setLoading(false);
    }
  }, [accessToken, page, search, tenantId]);

  const fetchRoles = useCallback(async () => {
    if (!accessToken) return;
    try {
      const r = await authApi.listRoles(accessToken);
      setRoles(r);
    } catch { /* ignore */ }
  }, [accessToken]);

  useEffect(() => { fetchUsers(); }, [fetchUsers]);
  useEffect(() => { fetchRoles(); }, [fetchRoles]);

  async function handleCreate(e: React.FormEvent) {
    e.preventDefault();
    if (!accessToken) return;
    setCreating(true);
    setError("");
    try {
      await authApi.createUser(accessToken, createForm);
      setShowCreate(false);
      setCreateForm({ first_name: "", last_name: "", email: "", password: "", phone: "", role_id: "", tenant_id: tenantId });
      await fetchUsers();
    } catch (err) {
      setError(String(err));
    } finally {
      setCreating(false);
    }
  }

  async function handleDelete(userId: string, userName: string) {
    if (!accessToken) return;
    if (!confirm(`Delete user "${userName}"? This cannot be undone.`)) return;
    try {
      await authApi.deleteUser(accessToken, userId);
      await fetchUsers();
    } catch (err) {
      setError(String(err));
    }
  }

  async function handleToggleActive(userId: string, currentActive: boolean) {
    if (!accessToken) return;
    try {
      await authApi.updateUser(accessToken, userId, { is_active: !currentActive });
      await fetchUsers();
    } catch (err) {
      setError(String(err));
    }
  }

  async function handleRoleAssign(userId: string, roleId: string) {
    if (!accessToken || !roleId) return;
    try {
      await authApi.assignUserRole(accessToken, userId, roleId);
      await fetchUsers();
    } catch (err) {
      setError(String(err));
    }
  }

  return (
    <div className="space-y-6">
      {/* Header */}
      <div className="flex items-center justify-between">
        <div>
          <div className="flex items-center gap-2">
            <h2 className="text-lg font-semibold tracking-tight">User Management</h2>
            {tenantId && (
              <span className="inline-flex items-center rounded-full bg-primary/10 px-2 py-0.5 text-[10px] font-bold text-primary ring-1 ring-primary/20">
                FILTERED BY TENANT
              </span>
            )}
          </div>
          <p className="text-sm text-muted-foreground">{total} total accounts</p>
        </div>
        {canAdd && (
          <button
            onClick={() => setShowCreate(true)}
            className="h-9 rounded-lg bg-primary px-4 text-xs font-medium text-primary-foreground transition-colors hover:bg-primary/90"
          >
            Add User
          </button>
        )}
      </div>

      {/* Search */}
      <div className="relative">
        <input
          type="text"
          value={search}
          onChange={(e) => { setSearch(e.target.value); setPage(0); }}
          placeholder="Search by name or email..."
          className="h-10 w-full rounded-lg border border-input bg-background/70 px-4 text-sm focus:outline-none focus:ring-2 focus:ring-primary/40"
        />
      </div>

      {error && (
        <div className="rounded-lg bg-destructive/10 p-3 text-sm text-destructive border border-destructive/20">
          {error}
        </div>
      )}

      {/* User list */}
      <div className="rounded-xl border border-border/60 bg-card/60 backdrop-blur overflow-hidden">
        {loading ? (
          <div className="flex items-center justify-center py-20 text-sm text-muted-foreground">
            Polling user data...
          </div>
        ) : users.length === 0 ? (
          <div className="py-20 text-center text-sm text-muted-foreground">
            No accounts found
          </div>
        ) : (
          <table className="w-full text-sm">
            <thead>
              <tr className="border-b border-border/60 text-left text-xs font-semibold text-muted-foreground bg-secondary/20">
                <th className="px-4 py-3">User</th>
                <th className="px-4 py-3">Email</th>
                <th className="px-4 py-3">Role</th>
                <th className="px-4 py-3 text-center">Status</th>
                <th className="px-4 py-3">Last Access</th>
                {(canEdit || canDelete) && <th className="px-4 py-3 text-right">Actions</th>}
              </tr>
            </thead>
            <tbody>
              {users.map((user) => (
                <tr key={user.id} className="border-b border-border/30 transition-colors hover:bg-secondary/30">
                  <td className="px-4 py-3">
                    <div className="flex items-center gap-3">
                      <div className="flex h-8 w-8 items-center justify-center rounded bg-primary/10 text-[10px] font-bold text-primary">
                        {user.first_name[0]}{user.last_name[0]}
                      </div>
                      <span className="font-medium">{user.first_name} {user.last_name}</span>
                    </div>
                  </td>
                  <td className="px-4 py-3 text-muted-foreground">{user.email}</td>
                  <td className="px-4 py-3">
                    {canEdit ? (
                      <select
                        className="h-8 rounded border border-input bg-background/70 px-2 text-xs"
                        value={user.roles?.[0]?.id ?? ""}
                        onChange={(e) => handleRoleAssign(user.id, e.target.value)}
                      >
                        <option value="">No Role</option>
                        {roles.map((r) => (
                          <option key={r.id} value={r.id}>{r.name}</option>
                        ))}
                      </select>
                    ) : (
                      <span className="text-xs">{user.roles?.[0]?.name ?? "—"}</span>
                    )}
                  </td>
                  <td className="px-4 py-3 text-center">
                    <button
                      onClick={() => canEdit && handleToggleActive(user.id, user.is_active)}
                      disabled={!canEdit}
                      className={`inline-flex items-center rounded px-2.5 py-0.5 text-[10px] font-bold tracking-wider ${
                        user.is_active 
                          ? "bg-accent/10 text-accent ring-1 ring-accent/30" 
                          : "bg-destructive/10 text-destructive ring-1 ring-destructive/30"
                      }`}
                    >
                      {user.is_active ? "ACTIVE" : "DISABLED"}
                    </button>
                  </td>
                  <td className="px-4 py-3 text-muted-foreground">
                    {user.last_login ? new Date(user.last_login).toLocaleDateString() : "Never"}
                  </td>
                  {(canEdit || canDelete) && (
                    <td className="px-4 py-3 text-right">
                      {canDelete && (
                        <button
                          onClick={() => handleDelete(user.id, `${user.first_name} ${user.last_name}`)}
                          className="rounded px-3 py-1 text-xs font-medium text-destructive hover:bg-destructive/10"
                        >
                          Delete
                        </button>
                      )}
                    </td>
                  )}
                </tr>
              ))}
            </tbody>
          </table>
        )}

        {/* Pagination */}
        {total > 25 && (
          <div className="flex items-center justify-between border-t border-border/60 px-4 py-3 text-xs text-muted-foreground">
            <span>Showing {page * 25 + 1}–{Math.min((page + 1) * 25, total)} of {total}</span>
            <div className="flex gap-2">
              <button
                onClick={() => setPage(p => Math.max(0, p - 1))}
                disabled={page === 0}
                className="rounded border border-border px-3 py-1 hover:bg-secondary/70 disabled:opacity-40"
              >
                Previous
              </button>
              <button
                onClick={() => setPage(p => p + 1)}
                disabled={(page + 1) * 25 >= total}
                className="rounded border border-border px-3 py-1 hover:bg-secondary/70 disabled:opacity-40"
              >
                Next
              </button>
            </div>
          </div>
        )}
      </div>

      {/* Create User Dialog */}
      {showCreate && (
        <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/60 backdrop-blur-sm">
          <div className="w-full max-w-md rounded-xl border border-border/60 bg-card p-6 shadow-2xl">
            <div className="mb-6">
              <h3 className="font-semibold text-lg tracking-tight">Create User</h3>
              <p className="text-sm text-muted-foreground">Add a new account to this installation</p>
            </div>

            <form onSubmit={handleCreate} className="space-y-4">
              <div className="grid grid-cols-2 gap-3">
                <div className="space-y-1.5">
                  <label className="text-xs font-medium text-muted-foreground" htmlFor="new-first">First Name</label>
                  <input id="new-first" type="text" required value={createForm.first_name}
                    onChange={(e) => setCreateForm(f => ({ ...f, first_name: e.target.value }))}
                    className="h-10 w-full rounded border border-input bg-background/70 px-3 text-sm focus:outline-none focus:ring-1 focus:ring-primary" />
                </div>
                <div className="space-y-1.5">
                  <label className="text-xs font-medium text-muted-foreground" htmlFor="new-last">Last Name</label>
                  <input id="new-last" type="text" required value={createForm.last_name}
                    onChange={(e) => setCreateForm(f => ({ ...f, last_name: e.target.value }))}
                    className="h-10 w-full rounded border border-input bg-background/70 px-3 text-sm focus:outline-none focus:ring-1 focus:ring-primary" />
                </div>
              </div>
              <div className="space-y-1.5">
                <label className="text-xs font-medium text-muted-foreground" htmlFor="new-email">Email</label>
                <input id="new-email" type="email" required value={createForm.email}
                  onChange={(e) => setCreateForm(f => ({ ...f, email: e.target.value }))}
                  className="h-10 w-full rounded border border-input bg-background/70 px-3 text-sm focus:outline-none focus:ring-1 focus:ring-primary" />
              </div>
              <div className="space-y-1.5">
                <label className="text-xs font-medium text-muted-foreground" htmlFor="new-pass">Password</label>
                <input id="new-pass" type="password" required value={createForm.password}
                  onChange={(e) => setCreateForm(f => ({ ...f, password: e.target.value }))}
                  className="h-10 w-full rounded border border-input bg-background/70 px-3 text-sm focus:outline-none focus:ring-1 focus:ring-primary" />
              </div>
              <div className="space-y-1.5">
                <label className="text-xs font-medium text-muted-foreground" htmlFor="new-role">Role</label>
                <select id="new-role" value={createForm.role_id}
                  onChange={(e) => setCreateForm(f => ({ ...f, role_id: e.target.value }))}
                  className="h-10 w-full rounded border border-input bg-background/70 px-3 text-sm font-medium focus:outline-none focus:ring-1 focus:ring-primary">
                  <option value="">No Role</option>
                  {roles.map((r) => <option key={r.id} value={r.id}>{r.name}</option>)}
                </select>
              </div>

              <div className="flex gap-3 pt-4">
                <button type="button" onClick={() => setShowCreate(false)}
                  className="flex h-10 flex-1 items-center justify-center rounded border border-border text-sm font-medium transition-colors hover:bg-secondary/70">
                  Cancel
                </button>
                <button type="submit" disabled={creating}
                  className="flex h-10 flex-[2] items-center justify-center rounded bg-primary text-sm font-medium text-primary-foreground transition-colors hover:bg-primary/90 disabled:opacity-50">
                  {creating ? "Creating..." : "Confirm Create"}
                </button>
              </div>
            </form>
          </div>
        </div>
      )}
    </div>
  );
}
