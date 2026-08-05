use axum::{Json, extract::State, http::StatusCode};
use serde::Serialize;

use crate::app::AppState;

#[derive(Serialize)]
pub struct HealthResponse {
    service: &'static str,
    database: &'static str,
}

pub async fn check(State(state): State<AppState>) -> (StatusCode, Json<HealthResponse>) {
    let database_is_ready = sqlx::query_scalar::<_, i32>("SELECT 1")
        .fetch_one(&state.db)
        .await
        .is_ok();

    if database_is_ready {
        (
            StatusCode::OK,
            Json(HealthResponse {
                service: "ok",
                database: "ok",
            }),
        )
    } else {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(HealthResponse {
                service: "ok",
                database: "unavailable",
            }),
        )
    }
}
