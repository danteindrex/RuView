//! Domain models for the Wave auth system.
//!
//! All 17 structs mirror Taha's TypeORM entities, adapted for SQLite/sqlx.

use serde::{Deserialize, Serialize};

// ─── License ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct License {
    pub id: String,
    pub license_key: String,
    pub tenant_id: String,
    pub tenant_name: String,
    pub tier: String,
    pub hardware_fingerprint: Option<String>,
    pub allowed_modules: String, // JSON array
    pub max_users: i32,
    pub issued_at: String,
    pub expires_at: String,
    pub last_validated_at: String,
    pub is_active: bool,
    pub cached_response: Option<String>,
}

// ─── Tenant ─────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Tenant {
    pub id: String,
    pub name: String,
    pub domain: Option<String>,
    pub contact: Option<String>,
    pub email: Option<String>,
    pub location: Option<String>,
    pub industry: Option<String>,
    pub currency_code: Option<String>,
    #[sqlx(rename = "type")]
    #[serde(rename = "type")]
    pub tenant_type: Option<String>,
    pub verification_status: Option<String>,
    pub logo: Option<String>,
    pub description: Option<String>,
    pub registered_by: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

// ─── User ───────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct User {
    pub id: String,
    pub first_name: String,
    pub last_name: String,
    pub email: String,
    pub phone: Option<String>,
    pub avatar_url: Option<String>,
    #[serde(skip_serializing)]
    pub password_hash: String,
    pub is_active: bool,
    pub must_change_password: bool,
    pub email_verified: bool,
    pub tenant_id: String,
    pub department_id: Option<String>,
    pub last_login: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

/// Lightweight user response sent to the frontend (no password hash).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserResponse {
    pub id: String,
    pub first_name: String,
    pub last_name: String,
    pub email: String,
    pub phone: Option<String>,
    pub avatar_url: Option<String>,
    pub is_active: bool,
    pub must_change_password: bool,
    pub tenant_id: String,
    pub department_id: Option<String>,
    pub last_login: Option<String>,
    pub roles: Vec<RoleWithModules>,
    pub scope: Option<String>,
    pub created_at: String,
}

impl From<User> for UserResponse {
    fn from(u: User) -> Self {
        Self {
            id: u.id,
            first_name: u.first_name,
            last_name: u.last_name,
            email: u.email,
            phone: u.phone,
            avatar_url: u.avatar_url,
            is_active: u.is_active,
            must_change_password: u.must_change_password,
            tenant_id: u.tenant_id,
            department_id: u.department_id,
            last_login: u.last_login,
            roles: Vec::new(),
            scope: None,
            created_at: u.created_at,
        }
    }
}

// ─── Role ───────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Role {
    pub id: String,
    pub name: String,
    pub description: String,
    pub tenant_id: String,
    pub created_at: String,
}

/// Role with its associated modules and permissions (for login response).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoleWithModules {
    pub id: String,
    pub name: String,
    pub description: String,
    pub modules: Vec<ModuleWithPermission>,
}

// ─── Department ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Department {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub tenant_id: String,
    pub created_at: String,
}

// ─── Access Module ──────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct AccessModule {
    pub id: String,
    pub code: String,
    pub name: String,
    pub dirs: Option<String>,
    pub created_at: String,
}

/// Module sent to frontend with its permission attached.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModuleWithPermission {
    pub id: String,
    pub name: String,
    pub code: String,
    pub permission: PermissionDetail,
}

// ─── Permission ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Permission {
    pub id: String,
    pub can_read: bool,
    pub can_add: bool,
    pub can_edit: bool,
    pub can_delete: bool,
    pub can_approve: bool,
    pub can_give_discount: bool,
    pub read_scope: i32,
    pub add_scope: i32,
    pub edit_scope: i32,
    pub delete_scope: i32,
    pub approve_scope: i32,
    pub created_at: String,
}

/// Permission detail sent in login responses (matches Taha's frontend contract).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PermissionDetail {
    pub edit: bool,
    pub view: bool,
    #[serde(rename = "deleteAccess")]
    pub delete_access: bool,
    pub add: bool,
    pub approve: bool,
    #[serde(rename = "giveDiscount")]
    pub give_discount: bool,
    #[serde(rename = "readScoop")]
    pub read_scope: i32,
    #[serde(rename = "editScoop")]
    pub edit_scope: i32,
    #[serde(rename = "addScoop")]
    pub add_scope: i32,
    #[serde(rename = "deleteScoop")]
    pub delete_scope: i32,
    #[serde(rename = "approveScoop")]
    pub approve_scope: i32,
}

impl From<Permission> for PermissionDetail {
    fn from(p: Permission) -> Self {
        Self {
            edit: p.can_edit,
            view: p.can_read,
            delete_access: p.can_delete,
            add: p.can_add,
            approve: p.can_approve,
            give_discount: p.can_give_discount,
            read_scope: p.read_scope,
            edit_scope: p.edit_scope,
            add_scope: p.add_scope,
            delete_scope: p.delete_scope,
            approve_scope: p.approve_scope,
        }
    }
}

// ─── Junction tables ────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct RoleModule {
    pub id: String,
    pub role_id: String,
    pub module_id: String,
    pub permission_id: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct UserRole {
    pub id: String,
    pub user_id: String,
    pub role_id: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct TenantModule {
    pub id: String,
    pub tenant_id: String,
    pub module_id: String,
    pub is_active: bool,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct DepartmentRole {
    pub id: String,
    pub department_id: String,
    pub role_id: String,
}

// ─── Security & session ────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct RefreshToken {
    pub id: String,
    pub token: String,
    pub user_id: String,
    pub user_agent: Option<String>,
    pub ip_address: Option<String>,
    pub revoked: bool,
    pub revoked_at: Option<String>,
    pub replaced_by_token: Option<String>,
    pub expires_at: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct AuditLog {
    pub id: String,
    pub user_id: Option<String>,
    pub action: String,
    pub ip_address: Option<String>,
    pub user_agent: Option<String>,
    pub details: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct ApiKey {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub key_hash: String,
    pub key_prefix: String,
    pub tenant_id: String,
    pub generated_by: String,
    pub expires_at: Option<String>,
    pub last_used_at: Option<String>,
    pub status: String,
    pub revoked_by: Option<String>,
    pub revoked_at: Option<String>,
    pub revoke_reason: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct ServiceToken {
    pub id: String,
    pub token: String,
    pub service_name: String,
    pub description: Option<String>,
    pub is_active: bool,
    pub permissions: Option<String>,
    pub expires_at: Option<String>,
    pub last_used_at: Option<String>,
    pub created_at: String,
}

// ─── Auth DTOs (request/response) ──────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct LoginRequest {
    pub email: String,
    pub password: String,
}

#[derive(Debug, Serialize)]
pub struct LoginResponse {
    pub access_token: String,
    pub refresh_token: String,
    pub user: UserResponse,
}

#[derive(Debug, Deserialize)]
pub struct CreateUserRequest {
    pub first_name: String,
    pub last_name: String,
    pub email: String,
    pub password: String,
    pub phone: Option<String>,
    pub role_id: Option<String>,
    pub tenant_id: Option<String>,
    pub department_id: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct CreateTenantRequest {
    pub name: String,
    pub industry: Option<String>,
    pub domain: Option<String>,
    pub contact: Option<String>,
    pub email: Option<String>,
    pub location: Option<String>,
    pub currency_code: Option<String>,
    pub description: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct AssignTenantModulesRequest {
    pub tenant_id: String,
    pub module_ids: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateUserRequest {
    pub first_name: Option<String>,
    pub last_name: Option<String>,
    pub email: Option<String>,
    pub phone: Option<String>,
    pub is_active: Option<bool>,
    pub department_id: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct ChangePasswordRequest {
    pub old_password: String,
    pub new_password: String,
}

#[derive(Debug, Deserialize)]
pub struct CreateRoleRequest {
    pub name: String,
    pub description: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct SetRolePermissionsRequest {
    pub role_id: String,
    pub module_permissions: Vec<ModulePermissionInput>,
}

#[derive(Debug, Deserialize)]
pub struct ModulePermissionInput {
    pub module_id: String,
    pub can_read: bool,
    pub can_add: bool,
    pub can_edit: bool,
    pub can_delete: bool,
    pub can_approve: bool,
    pub read_scope: i32,
    pub add_scope: i32,
    pub edit_scope: i32,
    pub delete_scope: i32,
    pub approve_scope: i32,
}

#[derive(Debug, Deserialize)]
pub struct ActivateLicenseRequest {
    pub license_key: String,
    pub admin_details: Option<AdminDetails>,
}

#[derive(Debug, Deserialize)]
pub struct AdminDetails {
    pub first_name: String,
    pub last_name: String,
    pub email: String,
    pub password: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct LicenseStatus {
    pub is_licensed: bool,
    pub tenant_name: Option<String>,
    pub tier: Option<String>,
    pub expires_at: Option<String>,
    pub allowed_modules: Vec<String>,
    pub max_users: i32,
}

/// Super Admin JWT claims (from cloud license server).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SuperAdminClaims {
    pub sub: String, // super admin user id
    pub email: String,
    pub first_name: String,
    pub last_name: String,
    pub role: String,  // "super_admin"
    pub scope: String, // "global"
    pub exp: i64,
    pub iat: i64,
}
