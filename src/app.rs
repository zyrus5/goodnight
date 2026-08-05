use axum::{
    Router,
    http::{HeaderValue, Method},
    routing::get,
};
use sqlx::PgPool;
use tower_http::{
    cors::CorsLayer,
    trace::{DefaultMakeSpan, DefaultOnResponse, TraceLayer},
};
use tracing::Level;

use crate::{frontend, routes};

#[derive(Clone)]
pub struct AppState {
    pub db: PgPool,
}

pub fn router(db: PgPool, frontend_origin: &str) -> Router {
    let cors = CorsLayer::new()
        .allow_origin(
            frontend_origin
                .parse::<HeaderValue>()
                .expect("FRONTEND_ORIGIN must be a valid HTTP header value"),
        )
        .allow_methods([
            Method::GET,
            Method::POST,
            Method::PUT,
            Method::PATCH,
            Method::DELETE,
        ])
        .allow_headers(tower_http::cors::Any);

    let api = Router::new()
        .route("/health", get(routes::health::check))
        .fallback(api_not_found);

    Router::new()
        .nest("/api", api)
        .fallback(frontend::serve)
        .with_state(AppState { db })
        .layer(cors)
        .layer(
            TraceLayer::new_for_http()
                .make_span_with(DefaultMakeSpan::new().level(Level::INFO))
                .on_response(DefaultOnResponse::new().level(Level::INFO)),
        )
}

async fn api_not_found() -> (axum::http::StatusCode, &'static str) {
    (axum::http::StatusCode::NOT_FOUND, "API route not found")
}
