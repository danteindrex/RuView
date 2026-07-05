/**
 * Auth API — Tauri invoke wrappers for authentication and user management.
 *
 * All commands map 1:1 to the Rust Tauri commands in src/commands/*.
 */

import { invoke } from "@tauri-apps/api/core";

// ─── Auth Types ─────────────────────────────────────────────────────────────

export interface LoginResponse {
  access_token: string;
  refresh_token?: string;
  user: AuthUser;
}

export interface AuthUser {
  id: string;
  first_name: string;
  last_name: string;
  email: string;
  phone?: string;
  avatar_url?: string;
  is_active: boolean;
  must_change_password: boolean;
  tenant_id: string;
  department_id?: string;
  last_login?: string;
  roles: RoleWithModules[];
  scope?: string; // "global" for super admin
  created_at?: string;
}

export interface RoleWithModules {
  id: string;
  name: string;
  description: string;
  modules: ModuleWithPermission[];
}

export interface ModuleWithPermission {
  id: string;
  name: string;
  code: string;
  permission: PermissionDetail;
}

export interface PermissionDetail {
  edit: boolean;
  view: boolean;
  deleteAccess: boolean;
  add: boolean;
  approve: boolean;
  giveDiscount: boolean;
  readScoop: number;
  editScoop: number;
  addScoop: number;
  deleteScoop: number;
  approveScoop: number;
}

export interface LicenseStatus {
  is_licensed: boolean;
  tenant_name?: string;
  tier?: string;
  expires_at?: string;
  allowed_modules: string[];
  max_users: number;
}

export interface AccessModule {
  id: string;
  code: string;
  name: string;
  dirs?: string;
  created_at: string;
}

export interface Role {
  id: string;
  name: string;
  description: string;
  tenant_id: string;
  created_at: string;
}

export interface UserListResponse {
  users: AuthUser[];
  total: number;
  page: number;
  limit: number;
}

// ─── API Functions ──────────────────────────────────────────────────────────

export const authApi = {
  // Auth
  login(email: string, password: string) {
    return invoke<LoginResponse>("login", {
      email,
      passwordInput: password,
    });
  },

  logout(accessToken: string) {
    return invoke<void>("logout", { accessToken });
  },

  refreshToken(refreshToken: string) {
    return invoke<{ access_token: string; refresh_token: string }>(
      "refresh_token",
      { refreshTokenStr: refreshToken }
    );
  },

  getCurrentUser(accessToken: string) {
    return invoke<AuthUser>("get_current_user", { accessToken });
  },

  changePassword(accessToken: string, oldPassword: string, newPassword: string) {
    return invoke<void>("change_password", {
      accessToken,
      oldPassword,
      newPassword,
    });
  },

  // Users
  listUsers(accessToken: string, page = 0, limit = 25, search?: string, tenant_id?: string) {
    return invoke<UserListResponse>("list_users", {
      accessToken,
      page,
      limit,
      search,
      tenantId: tenant_id,
    });
  },

  getUser(accessToken: string, userId: string) {
    return invoke<AuthUser>("get_user", { accessToken, userId });
  },

  createUser(
    accessToken: string,
    request: {
      first_name: string;
      last_name: string;
      email: string;
      password: string;
      phone?: string;
      role_id?: string;
      tenant_id?: string;
      department_id?: string;
    }
  ) {
    return invoke<AuthUser>("create_user", { accessToken, request });
  },

  updateUser(
    accessToken: string,
    userId: string,
    request: {
      first_name?: string;
      last_name?: string;
      email?: string;
      phone?: string;
      is_active?: boolean;
      department_id?: string;
    }
  ) {
    return invoke<AuthUser>("update_user", { accessToken, userId, request });
  },

  deleteUser(accessToken: string, userId: string) {
    return invoke<void>("delete_user", { accessToken, userId });
  },

  assignUserRole(accessToken: string, userId: string, roleId: string) {
    return invoke<void>("assign_user_role", { accessToken, userId, roleId });
  },

  // Roles
  listRoles(accessToken: string) {
    return invoke<Role[]>("list_roles", { accessToken });
  },

  getRole(accessToken: string, roleId: string) {
    return invoke<RoleWithModules>("get_role", { accessToken, roleId });
  },

  createRole(accessToken: string, request: { name: string; description?: string }) {
    return invoke<Role>("create_role", { accessToken, request });
  },

  deleteRole(accessToken: string, roleId: string) {
    return invoke<void>("delete_role", { accessToken, roleId });
  },

  setRolePermissions(
    accessToken: string,
    request: {
      role_id: string;
      module_permissions: Array<{
        module_id: string;
        can_read: boolean;
        can_add: boolean;
        can_edit: boolean;
        can_delete: boolean;
        can_approve: boolean;
        read_scope: number;
        add_scope: number;
        edit_scope: number;
        delete_scope: number;
        approve_scope: number;
      }>;
    }
  ) {
    return invoke<void>("set_role_permissions", { accessToken, request });
  },

  listModules(accessToken: string) {
    return invoke<AccessModule[]>("list_modules", { accessToken });
  },

  listTenantModules(accessToken: string, tenantId?: string) {
    return invoke<AccessModule[]>("list_tenant_modules", { accessToken, tenantId });
  },

  // License
  licenseStatus() {
    return invoke<LicenseStatus>("license_status");
  },

  activateLicense: async (req: { 
    license_key: string;
    admin_details?: {
      first_name: string;
      last_name: string;
      email: string;
      password: string;
    };
  }) => {
    return await invoke<LicenseStatus>("activate_license", { request: req });
  },
  listTenants(accessToken: string) {
    return invoke<Tenant[]>("list_tenants", { accessToken });
  },

  createTenant(accessToken: string, request: CreateTenantRequest) {
    return invoke<Tenant>("create_tenant", { accessToken, request });
  },

  assignTenantModules(accessToken: string, request: AssignTenantModulesRequest) {
    return invoke<void>("assign_tenant_modules", { accessToken, request });
  },

  deleteTenant(accessToken: string, tenantId: string) {
    return invoke<void>("delete_tenant", { accessToken, tenantId });
  },
};

export interface Tenant {
  id: string;
  name: string;
  domain?: string;
  industry?: string;
  type?: string;
  verification_status?: string;
  created_at: string;
}

export interface CreateTenantRequest {
  name: string;
  industry?: string;
  domain?: string;
  contact?: string;
  email?: string;
  location?: string;
  currency_code?: string;
  description?: string;
}

export interface AssignTenantModulesRequest {
  tenant_id: string;
  module_ids: string[];
}
