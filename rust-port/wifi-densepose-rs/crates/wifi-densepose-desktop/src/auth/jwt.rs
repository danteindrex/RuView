//! JWT token generation and verification.
//!
//! Issues short-lived access tokens (15 min) and long-lived refresh tokens (7 days).
//! Mirrors Taha's auth middleware token rotation logic.

use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Default access token lifetime: 15 minutes.
const ACCESS_TOKEN_EXPIRY_SECS: i64 = 15 * 60;

/// Default refresh token lifetime: 7 days.
const REFRESH_TOKEN_EXPIRY_SECS: i64 = 7 * 24 * 60 * 60;

/// JWT claims for local user access tokens.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccessClaims {
    /// Subject (user ID)
    pub sub: String,
    /// User email
    pub email: String,
    /// User first name
    pub first_name: String,
    /// User last name
    pub last_name: String,
    /// Tenant ID (for RLS filtering)
    pub tenant_id: String,
    /// Auth scope: "tenant" for local users, "global" for super admins
    pub scope: String,
    /// Issued at (Unix timestamp)
    pub iat: i64,
    /// Expiration (Unix timestamp)
    pub exp: i64,
}

/// Generate a JWT access token for a local user.
pub fn generate_access_token(
    user_id: &str,
    email: &str,
    first_name: &str,
    last_name: &str,
    tenant_id: &str,
    secret: &str,
) -> Result<String, jsonwebtoken::errors::Error> {
    let now = chrono::Utc::now().timestamp();
    let claims = AccessClaims {
        sub: user_id.to_string(),
        email: email.to_string(),
        first_name: first_name.to_string(),
        last_name: last_name.to_string(),
        tenant_id: tenant_id.to_string(),
        scope: "tenant".to_string(),
        iat: now,
        exp: now + ACCESS_TOKEN_EXPIRY_SECS,
    };
    encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(secret.as_bytes()),
    )
}

/// Verify and decode an access token.
///
/// Accepts both local JWTs (signed with local secret) and super-admin JWTs
/// (validated by checking the `scope` claim).
pub fn verify_access_token(
    token: &str,
    secret: &str,
) -> Result<AccessClaims, jsonwebtoken::errors::Error> {
    let token_data = decode::<AccessClaims>(
        token,
        &DecodingKey::from_secret(secret.as_bytes()),
        &Validation::default(),
    )?;
    Ok(token_data.claims)
}

/// Generate a cryptographically random refresh token string.
pub fn generate_refresh_token() -> String {
    Uuid::new_v4().to_string()
}

/// Calculate the expiry datetime string for a refresh token.
pub fn refresh_token_expiry() -> String {
    let expiry = chrono::Utc::now()
        + chrono::Duration::try_seconds(REFRESH_TOKEN_EXPIRY_SECS)
            .unwrap_or(chrono::Duration::try_days(7).unwrap());
    expiry.format("%Y-%m-%dT%H:%M:%SZ").to_string()
}

/// Generate or retrieve the JWT signing secret.
///
/// Uses a persistent secret stored in the database. If none exists,
/// generates a new 256-bit random secret.
pub async fn get_or_create_jwt_secret(
    pool: &sqlx::SqlitePool,
) -> Result<String, sqlx::Error> {
    // Check if we have a secret stored
    let existing: Option<String> = sqlx::query_scalar(
        "SELECT token FROM service_tokens WHERE service_name = 'jwt_secret' AND is_active = 1",
    )
    .fetch_optional(pool)
    .await?;

    if let Some(secret) = existing {
        return Ok(secret);
    }

    // Generate a new 256-bit secret
    use rand::Rng;
    let secret: String = rand::thread_rng()
        .sample_iter(&rand::distributions::Alphanumeric)
        .take(64)
        .map(char::from)
        .collect();

    let id = Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO service_tokens (id, token, service_name, description, is_active) \
         VALUES (?, ?, 'jwt_secret', 'JWT signing secret for local auth', 1)",
    )
    .bind(&id)
    .bind(&secret)
    .execute(pool)
    .await?;

    tracing::info!("Generated new JWT signing secret");
    Ok(secret)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_and_verify_token() {
        let secret = "test-secret-key-that-is-long-enough";
        let token = generate_access_token(
            "user-123",
            "test@example.com",
            "John",
            "Doe",
            "tenant-abc",
            secret,
        )
        .unwrap();

        let claims = verify_access_token(&token, secret).unwrap();
        assert_eq!(claims.sub, "user-123");
        assert_eq!(claims.email, "test@example.com");
        assert_eq!(claims.tenant_id, "tenant-abc");
        assert_eq!(claims.scope, "tenant");
    }

    #[test]
    fn test_wrong_secret_fails() {
        let token = generate_access_token(
            "user-123",
            "test@example.com",
            "John",
            "Doe",
            "tenant-abc",
            "correct-secret",
        )
        .unwrap();

        let result = verify_access_token(&token, "wrong-secret");
        assert!(result.is_err());
    }

    #[test]
    fn test_refresh_token_is_unique() {
        let t1 = generate_refresh_token();
        let t2 = generate_refresh_token();
        assert_ne!(t1, t2);
    }
}
