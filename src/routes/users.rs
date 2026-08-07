use axum::{
    Json,
    extract::{Path, Query, State},
};
use serde::Deserialize;
use serde_json::json;
use uuid::Uuid;

use crate::{
    app::AppState,
    audit,
    auth::{self, CurrentUser},
    error::{ApiError, ApiResult},
    models::{Page, PageQuery, UserView},
};

#[derive(Deserialize)]
pub struct UserInput {
    username: String,
    display_name: String,
    role: String,
    password: Option<String>,
    #[serde(default = "yes")]
    is_active: bool,
    version: Option<i32>,
}
fn yes() -> bool {
    true
}

pub async fn list(
    State(state): State<AppState>,
    _user: CurrentUser,
    Query(page): Query<PageQuery>,
) -> ApiResult<Json<Page<UserView>>> {
    let pattern = page.pattern();
    let total = sqlx::query_scalar(
        "SELECT count(*) FROM gd_users WHERE username ILIKE $1 OR display_name ILIKE $1",
    )
    .bind(&pattern)
    .fetch_one(&state.db)
    .await?;
    let items=sqlx::query_as::<_,UserView>("SELECT id,username,display_name,role,is_admin,is_active,version,created_at,updated_at FROM gd_users WHERE username ILIKE $1 OR display_name ILIKE $1 ORDER BY updated_at DESC,id LIMIT $2 OFFSET $3")
        .bind(pattern).bind(page.limit()).bind(page.offset()).fetch_all(&state.db).await?;
    Ok(Json(Page {
        items,
        page: page.page.max(1),
        page_size: page.limit(),
        total,
    }))
}

pub async fn create(
    State(state): State<AppState>,
    user: CurrentUser,
    Json(input): Json<UserInput>,
) -> ApiResult<Json<UserView>> {
    user.require_admin()?;
    let password = input
        .password
        .as_deref()
        .ok_or_else(|| ApiError::bad_request("PASSWORD_REQUIRED", "必须设置初始密码"))?;
    let hash = auth::hash_password(password)?;
    validate_role(&input.role)?;
    let is_admin = input.role == "ADMIN";
    let row=sqlx::query_as::<_,UserView>("INSERT INTO gd_users(username,display_name,password_hash,role,is_admin,is_active) VALUES($1,$2,$3,$4,$5,$6) RETURNING id,username,display_name,role,is_admin,is_active,version,created_at,updated_at")
        .bind(input.username.trim()).bind(input.display_name.trim()).bind(hash).bind(&input.role).bind(is_admin).bind(input.is_active).fetch_one(&state.db).await?;
    audit::record(
        &state.db,
        Some(user.id),
        "CREATE",
        "USER",
        Some(row.id),
        json!({"username":row.username}),
    )
    .await;
    Ok(Json(row))
}

pub async fn update(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
    Json(input): Json<UserInput>,
) -> ApiResult<Json<UserView>> {
    user.require_admin()?;
    let version = input
        .version
        .ok_or_else(|| ApiError::bad_request("VERSION_REQUIRED", "缺少版本号"))?;
    validate_role(&input.role)?;
    let is_admin = input.role == "ADMIN";
    let row = if let Some(password) = input.password.filter(|v| !v.is_empty()) {
        let hash = auth::hash_password(&password)?;
        sqlx::query_as::<_,UserView>("UPDATE gd_users SET display_name=$2,password_hash=$3,role=$4,is_admin=$5,is_active=$6,version=version+1,updated_at=now() WHERE id=$1 AND version=$7 RETURNING id,username,display_name,role,is_admin,is_active,version,created_at,updated_at").bind(id).bind(input.display_name.trim()).bind(hash).bind(&input.role).bind(is_admin).bind(input.is_active).bind(version).fetch_optional(&state.db).await?
    } else {
        sqlx::query_as::<_,UserView>("UPDATE gd_users SET display_name=$2,role=$3,is_admin=$4,is_active=$5,version=version+1,updated_at=now() WHERE id=$1 AND version=$6 RETURNING id,username,display_name,role,is_admin,is_active,version,created_at,updated_at").bind(id).bind(input.display_name.trim()).bind(&input.role).bind(is_admin).bind(input.is_active).bind(version).fetch_optional(&state.db).await?
    };
    let row = row.ok_or_else(|| ApiError::conflict("用户已被修改，请刷新后重试"))?;
    audit::record(
        &state.db,
        Some(user.id),
        "UPDATE",
        "USER",
        Some(id),
        json!({"is_active":row.is_active,"is_admin":row.is_admin}),
    )
    .await;
    Ok(Json(row))
}

fn validate_role(role: &str) -> ApiResult<()> {
    if matches!(role, "ADMIN" | "OPS" | "DEVELOPER" | "TESTER") {
        Ok(())
    } else {
        Err(ApiError::bad_request("INVALID_ROLE", "无效的系统角色"))
    }
}
