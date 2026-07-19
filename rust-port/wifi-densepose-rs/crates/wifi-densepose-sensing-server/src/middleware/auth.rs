use axum::{
    extract::{Request, State},
    http::{header::AUTHORIZATION, StatusCode},
    middleware::Next,
    response::Response,
};
use jsonwebtoken::{decode, DecodingKey, Validation};
use serde::Deserialize;
use std::sync::Arc;

#[derive(Debug, Deserialize)]
struct Claims {
    sub: String,
    exp: usize,
}

#[derive(Clone)]
pub struct AuthState {
    pub jwt_secret: Arc<String>,
}

pub async fn require_bearer(
    State(auth): State<AuthState>,
    request: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    let token = request
        .headers()
        .get(AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .ok_or(StatusCode::UNAUTHORIZED)?;

    let key = DecodingKey::from_secret(auth.jwt_secret.as_bytes());
    decode::<Claims>(token, &key, &Validation::default())
        .map_err(|_| StatusCode::UNAUTHORIZED)?;
    Ok(next.run(request).await)
}

/// For WebSocket endpoints: validate ?token= query param
pub fn validate_ws_token(token: Option<String>, secret: &str) -> Result<(), StatusCode> {
    let t = token.ok_or(StatusCode::UNAUTHORIZED)?;
    let key = DecodingKey::from_secret(secret.as_bytes());
    decode::<Claims>(&t, &key, &Validation::default())
        .map_err(|_| StatusCode::UNAUTHORIZED)?;
    Ok(())
}
