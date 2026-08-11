use axum::{
    Json,
    extract::State,
    http::{HeaderMap, HeaderValue, header},
};
use serde::Deserialize;
use serde_json::json;
use sqlx::FromRow;
use uuid::Uuid;

use crate::{
    app::AppState,
    audit,
    auth::{self, CurrentUser},
    error::{ApiError, ApiResult},
};

#[derive(Deserialize)]
pub struct LoginRequest {
    username: String,
    password: String,
}
#[derive(FromRow)]
struct LoginRow {
    id: Uuid,
    username: String,
    display_name: String,
    role: String,
    password_hash: String,
    is_admin: bool,
}

pub async fn login(
    State(state): State<AppState>,
    Json(input): Json<LoginRequest>,
) -> ApiResult<(HeaderMap, Json<serde_json::Value>)> {
    let row = sqlx::query_as::<_, LoginRow>("SELECT id,username,display_name,password_hash,role,is_admin FROM gd_users WHERE username=$1 AND is_active")
        .bind(input.username.trim()).fetch_optional(&state.db).await?;
    let Some(row) = row else {
        return Err(ApiError::Client(
            axum::http::StatusCode::UNAUTHORIZED,
            "用户名或密码错误".into(),
            "INVALID_CREDENTIALS",
            None,
        ));
    };
    if !auth::verify_password(&input.password, &row.password_hash) {
        return Err(ApiError::Client(
            axum::http::StatusCode::UNAUTHORIZED,
            "用户名或密码错误".into(),
            "INVALID_CREDENTIALS",
            None,
        ));
    }
    let (token, expires) =
        auth::create_session(&state.db, row.id, state.config.session_hours).await?;
    *state.worker_user_id.write().await = Some(row.id);
    let mut headers = HeaderMap::new();
    headers.insert(
        header::SET_COOKIE,
        HeaderValue::from_str(&auth::cookie_header(
            &token,
            expires,
            state.config.session_secure,
        ))
        .map_err(|e| anyhow::anyhow!(e))?,
    );
    audit::record(
        &state.db,
        Some(row.id),
        "LOGIN",
        "USER",
        Some(row.id),
        json!({}),
    )
    .await;
    Ok((
        headers,
        Json(
            json!({"id":row.id,"username":row.username,"display_name":row.display_name,"role":row.role,"is_admin":row.is_admin}),
        ),
    ))
}

pub async fn me(State(state): State<AppState>, user: CurrentUser) -> Json<CurrentUser> {
    *state.worker_user_id.write().await = Some(user.id);
    Json(user)
}

pub async fn logout(
    State(state): State<AppState>,
    user: CurrentUser,
    headers: HeaderMap,
) -> ApiResult<(HeaderMap, Json<serde_json::Value>)> {
    if let Some(token) = auth::cookie_value(headers.get(header::COOKIE), auth::SESSION_COOKIE) {
        sqlx::query("DELETE FROM gd_sessions WHERE token_hash=$1")
            .bind(auth::token_hash(token.as_bytes()))
            .execute(&state.db)
            .await?;
    }
    audit::record(
        &state.db,
        Some(user.id),
        "LOGOUT",
        "USER",
        Some(user.id),
        json!({}),
    )
    .await;
    let mut worker_user_id = state.worker_user_id.write().await;
    if *worker_user_id == Some(user.id) {
        *worker_user_id = None;
    }
    drop(worker_user_id);
    let mut response_headers = HeaderMap::new();
    response_headers.insert(
        header::SET_COOKIE,
        HeaderValue::from_str(&auth::clear_cookie_header(state.config.session_secure))
            .map_err(|e| anyhow::anyhow!(e))?,
    );
    Ok((response_headers, Json(json!({"ok":true}))))
}
