use serde_json::Value;
use sqlx::PgPool;
use uuid::Uuid;

pub async fn record(
    db: &PgPool,
    actor: Option<Uuid>,
    action: &str,
    object_type: &str,
    object_id: Option<Uuid>,
    summary: Value,
) {
    if let Err(error) = sqlx::query("INSERT INTO gd_audit_logs(actor_id,action,object_type,object_id,summary) VALUES($1,$2,$3,$4,$5)")
        .bind(actor).bind(action).bind(object_type).bind(object_id).bind(summary).execute(db).await {
        tracing::error!(%error, action, object_type, "failed to record audit event");
    }
}
