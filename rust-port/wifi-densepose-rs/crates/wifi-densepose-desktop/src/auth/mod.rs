//! Wave Desktop authentication, authorization, and licensing subsystem.
//!
//! Mirrors the Taha user-management service architecture:
//! - JWT-based authentication with refresh token rotation
//! - Role → Module → Permission RBAC matrix
//! - Multi-tenancy with tenant-scoped queries
//! - Cloud-authenticated Super Admin (vendor access)
//! - License validation against cloud server

pub mod audit;
pub mod db;
pub mod jwt;
pub mod license;
pub mod models;
pub mod password;
pub mod rate_limiter;
pub mod rls;
pub mod seed;
pub mod super_admin;
pub mod vault;
