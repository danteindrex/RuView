/**
 * usePermissions — Permission-based visibility hook.
 *
 * Mirrors Taha's `usePermissions` / `checkUserAccess` logic:
 * 1. Check if the module is in the tenant's allowed modules (license gate)
 * 2. Check if the user's role has the required permission on that module
 * 3. Super admins bypass all checks
 */

import { useMemo } from "react";
import { useAuthStore } from "@/lib/auth-store";
import type { PermissionDetail } from "@/lib/auth-api";

// ─── Module Code → Page ID mapping ─────────────────────────────────────────

/**
 * Maps our page IDs to the module codes defined in the Rust seed.
 * This links the sidebar navigation to the RBAC permission system.
 */
const PAGE_TO_MODULE: Record<string, string> = {
  dashboard: "dashboard",
  network: "network",
  flash: "firmware",
  ota: "firmware",
  modules: "edge-modules",
  "pi-nodes": "pi-nodes",
  sensing: "sensing",
  mesh: "mesh",
  provisioning: "provisioning",
  pose3d: "pose-3d",
  settings: "settings",
  users: "user-management",
  roles: "user-management",
  tenants: "user-management",
};

// ─── Hook ───────────────────────────────────────────────────────────────────

export function usePermissions() {
  const { user, isSuperAdmin, license } = useAuthStore();

  return useMemo(() => {
    /**
     * Check if a page/section should be visible in the sidebar.
     * This is the primary function used by the AppShell to filter tabs.
     */
    function isSectionVisible(pageId: string): boolean {
      // Super admin sees everything
      if (isSuperAdmin) return true;

      const moduleCode = PAGE_TO_MODULE[pageId];
      if (!moduleCode) return true; // Unknown pages are visible by default

      // 1. License gate: Is this module included in the tenant's license?
      if (license && license.allowed_modules.length > 0) {
        if (!license.allowed_modules.includes(moduleCode)) {
          return false;
        }
      }

      // 2. Role gate: Does the user have view permission on this module?
      if (!user) return false;

      return hasModulePermission(moduleCode, "view");
    }

    /**
     * Check if the current user has a specific permission on a module.
     *
     * @param moduleCode — The module code (e.g., "user-management")
     * @param action — The permission type: "view", "add", "edit", "delete", "approve"
     */
    function hasModulePermission(
      moduleCode: string,
      action: "view" | "add" | "edit" | "delete" | "approve"
    ): boolean {
      if (isSuperAdmin) return true;
      if (!user?.roles) return false;

      for (const role of user.roles) {
        for (const mod of role.modules) {
          if (mod.code === moduleCode) {
            return getPermissionValue(mod.permission, action);
          }
        }
      }
      return false;
    }

    /**
     * Get all permissions for a specific module (for the access matrix UI).
     */
    function getModulePermissions(moduleCode: string): PermissionDetail | null {
      if (!user?.roles) return null;

      for (const role of user.roles) {
        for (const mod of role.modules) {
          if (mod.code === moduleCode) {
            return mod.permission;
          }
        }
      }
      return null;
    }

    /**
     * Get the scope level for a given action on a module.
     *
     * Scope levels (matching Taha):
     * 0 = Own records only
     * 1 = Tenant-wide (all records in tenant)
     */
    function getActionScope(moduleCode: string, action: string): number {
      if (isSuperAdmin) return 1; // Global scope

      if (!user?.roles) return 0;

      for (const role of user.roles) {
        for (const mod of role.modules) {
          if (mod.code === moduleCode) {
            switch (action) {
              case "view": case "read": return mod.permission.readScoop;
              case "add": return mod.permission.addScoop;
              case "edit": return mod.permission.editScoop;
              case "delete": return mod.permission.deleteScoop;
              case "approve": return mod.permission.approveScoop;
              default: return 0;
            }
          }
        }
      }
      return 0;
    }

    return {
      isSectionVisible,
      hasModulePermission,
      getModulePermissions,
      getActionScope,
      isSuperAdmin,
      tenantModules: license?.allowed_modules ?? [],
    };
  }, [user, isSuperAdmin, license]);
}

// ─── Helpers ────────────────────────────────────────────────────────────────

function getPermissionValue(
  perm: PermissionDetail,
  action: "view" | "add" | "edit" | "delete" | "approve"
): boolean {
  switch (action) {
    case "view": return perm.view;
    case "add": return perm.add;
    case "edit": return perm.edit;
    case "delete": return perm.deleteAccess;
    case "approve": return perm.approve;
    default: return false;
  }
}
