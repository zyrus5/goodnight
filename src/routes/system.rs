use axum::{
    Json,
    extract::{Query, State},
    http::HeaderMap,
    response::IntoResponse,
};
use chrono::{DateTime, Utc};
use serde::Serialize;
use serde_json::{Value, json};
use sqlx::FromRow;
use subtle::ConstantTimeEq;
use uuid::Uuid;

use crate::{
    app::AppState,
    auth::CurrentUser,
    error::{ApiError, ApiResult},
    models::{Page, PageQuery},
};

pub async fn dashboard(State(s): State<AppState>, _: CurrentUser) -> ApiResult<Json<Value>> {
    let tasks: i64 = sqlx::query_scalar("SELECT count(*) FROM gd_tasks WHERE NOT is_deleted")
        .fetch_one(&s.db)
        .await?;
    let running: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM gd_executions WHERE status IN('RUNNING','CANCELING')",
    )
    .fetch_one(&s.db)
    .await?;
    let failed:i64=sqlx::query_scalar("SELECT count(*) FROM gd_executions WHERE status='FAILED' AND created_at>now()-interval '24 hours'").fetch_one(&s.db).await?;
    Ok(Json(
        json!({"tasks":tasks,"running":running,"failed_24h":failed}),
    ))
}

#[derive(Serialize, FromRow)]
pub struct AuditView {
    id: Uuid,
    actor_name: Option<String>,
    action: String,
    object_type: String,
    object_id: Option<Uuid>,
    summary: Value,
    created_at: DateTime<Utc>,
}
pub async fn audits(
    State(s): State<AppState>,
    u: CurrentUser,
    Query(p): Query<PageQuery>,
) -> ApiResult<Json<Page<AuditView>>> {
    u.require_admin()?;
    let pat = p.pattern();
    let total = sqlx::query_scalar(
        "SELECT count(*) FROM gd_audit_logs WHERE action ILIKE $1 OR object_type ILIKE $1",
    )
    .bind(&pat)
    .fetch_one(&s.db)
    .await?;
    let items=sqlx::query_as::<_,AuditView>("SELECT a.id,u.display_name actor_name,a.action,a.object_type,a.object_id,a.summary,a.created_at FROM gd_audit_logs a LEFT JOIN gd_users u ON u.id=a.actor_id WHERE a.action ILIKE $1 OR a.object_type ILIKE $1 ORDER BY a.created_at DESC LIMIT $2 OFFSET $3").bind(pat).bind(p.limit()).bind(p.offset()).fetch_all(&s.db).await?;
    Ok(Json(Page {
        items,
        page: p.page.max(1),
        page_size: p.limit(),
        total,
    }))
}

pub async fn metrics(
    State(s): State<AppState>,
    headers: HeaderMap,
) -> ApiResult<impl IntoResponse> {
    let expected = s
        .config
        .metrics_token
        .as_deref()
        .ok_or_else(ApiError::forbidden)?;
    let supplied = headers
        .get("Authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .unwrap_or("");
    if expected.as_bytes().ct_eq(supplied.as_bytes()).unwrap_u8() != 1 {
        return Err(ApiError::forbidden());
    }
    let running: i64 =
        sqlx::query_scalar("SELECT count(*) FROM gd_executions WHERE status='RUNNING'")
            .fetch_one(&s.db)
            .await?;
    let queued: i64 =
        sqlx::query_scalar("SELECT count(*) FROM gd_node_executions WHERE status='QUEUED'")
            .fetch_one(&s.db)
            .await?;
    let unknown: i64 =
        sqlx::query_scalar("SELECT count(*) FROM gd_node_executions WHERE status='UNKNOWN'")
            .fetch_one(&s.db)
            .await?;
    let failures: i64 =
        sqlx::query_scalar("SELECT count(*) FROM gd_executions WHERE status='FAILED'")
            .fetch_one(&s.db)
            .await?;
    Ok((
        [("content-type", "text/plain; version=0.0.4")],
        format!(
            "goodnight_executions_running {running}\ngoodnight_nodes_queued {queued}\ngoodnight_nodes_unknown {unknown}\ngoodnight_executions_failed_total {failures}\n"
        ),
    ))
}
