use std::sync::Arc;

use argon2::{
    Argon2, PasswordHash, PasswordHasher, PasswordVerifier,
    password_hash::{SaltString, rand_core::OsRng},
};
use axum::{
    extract::FromRequestParts,
    http::{header, request::Parts},
};
use chrono::{Duration, Utc};
use serde::Serialize;
use sha2::{Digest, Sha256};
use sqlx::{FromRow, PgPool};
use uuid::Uuid;

use crate::{
    app::AppState,
    config::Config,
    error::{ApiError, ApiResult},
};

pub const SESSION_COOKIE: &str = "goodnight_session";

#[derive(Debug, Clone, Serialize, FromRow)]
pub struct CurrentUser {
    pub id: Uuid,
    pub username: String,
    pub display_name: String,
    pub is_admin: bool,
}

impl CurrentUser {
    pub fn require_admin(&self) -> ApiResult<()> {
        if self.is_admin {
            Ok(())
        } else {
            Err(ApiError::forbidden())
        }
    }
}

impl FromRequestParts<AppState> for CurrentUser {
    type Rejection = ApiError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let token = cookie_value(parts.headers.get(header::COOKIE), SESSION_COOKIE)
            .ok_or_else(ApiError::unauthorized)?;
        let hash = token_hash(token.as_bytes());
        sqlx::query_as::<_, CurrentUser>(
            "SELECT u.id,u.username,u.display_name,u.is_admin FROM gd_sessions s JOIN gd_users u ON u.id=s.user_id WHERE s.token_hash=$1 AND s.expires_at>now() AND u.is_active",
        )
        .bind(hash)
        .fetch_optional(&state.db)
        .await?
        .ok_or_else(ApiError::unauthorized)
    }
}

pub fn hash_password(password: &str) -> ApiResult<String> {
    if password.len() < 10 || password.len() > 512 {
        return Err(ApiError::bad_request(
            "INVALID_PASSWORD",
            "密码长度必须为 10 到 512 个字符",
        ));
    }
    let salt = SaltString::generate(&mut OsRng);
    Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .map(|hash| hash.to_string())
        .map_err(|error| anyhow::anyhow!(error).into())
}

pub fn verify_password(password: &str, encoded: &str) -> bool {
    PasswordHash::new(encoded).ok().is_some_and(|hash| {
        Argon2::default()
            .verify_password(password.as_bytes(), &hash)
            .is_ok()
    })
}

pub async fn create_session(
    db: &PgPool,
    user_id: Uuid,
    hours: i64,
) -> ApiResult<(String, chrono::DateTime<Utc>)> {
    let token = format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple());
    let expires_at = Utc::now() + Duration::hours(hours);
    sqlx::query("INSERT INTO gd_sessions(user_id,token_hash,expires_at) VALUES($1,$2,$3)")
        .bind(user_id)
        .bind(token_hash(token.as_bytes()))
        .bind(expires_at)
        .execute(db)
        .await?;
    Ok((token, expires_at))
}

pub fn cookie_header(token: &str, expires: chrono::DateTime<Utc>, secure: bool) -> String {
    format!(
        "{SESSION_COOKIE}={token}; Path=/; HttpOnly; SameSite=Lax; Expires={}{}",
        expires.format("%a, %d %b %Y %H:%M:%S GMT"),
        if secure { "; Secure" } else { "" }
    )
}

pub fn clear_cookie_header(secure: bool) -> String {
    format!(
        "{SESSION_COOKIE}=; Path=/; HttpOnly; SameSite=Lax; Max-Age=0{}",
        if secure { "; Secure" } else { "" }
    )
}

pub fn token_hash(value: &[u8]) -> Vec<u8> {
    Sha256::digest(value).to_vec()
}

pub fn cookie_value<'a>(
    header: Option<&'a axum::http::HeaderValue>,
    name: &str,
) -> Option<&'a str> {
    header?.to_str().ok()?.split(';').find_map(|part| {
        let (key, value) = part.trim().split_once('=')?;
        (key == name).then_some(value)
    })
}

pub async fn bootstrap_admin(db: &PgPool, config: Arc<Config>) -> anyhow::Result<()> {
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM gd_users")
        .fetch_one(db)
        .await?;
    if count != 0 {
        return Ok(());
    }
    let username = config.bootstrap_admin_username.as_deref().ok_or_else(|| {
        anyhow::anyhow!("BOOTSTRAP_ADMIN_USERNAME is required for an empty database")
    })?;
    let password = config.bootstrap_admin_password.as_deref().ok_or_else(|| {
        anyhow::anyhow!("BOOTSTRAP_ADMIN_PASSWORD is required for an empty database")
    })?;
    let hash = hash_password(password).map_err(|e| anyhow::anyhow!(e.to_string()))?;
    sqlx::query("INSERT INTO gd_users(username,display_name,password_hash,is_admin) VALUES($1,$2,$3,true) ON CONFLICT DO NOTHING")
        .bind(username).bind(&config.bootstrap_admin_display_name).bind(hash).execute(db).await?;
    tracing::info!(username, "bootstrap administrator created");
    Ok(())
}

pub async fn component_permission(
    db: &PgPool,
    user: &CurrentUser,
    component_id: Uuid,
    maintain: bool,
) -> ApiResult<()> {
    if user.is_admin {
        return Ok(());
    }
    let allowed: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM gd_component_members WHERE component_id=$1 AND user_id=$2 AND ($3=false OR role='MAINTAINER'))")
        .bind(component_id).bind(user.id).bind(maintain).fetch_one(db).await?;
    if allowed {
        Ok(())
    } else {
        Err(ApiError::forbidden())
    }
}
