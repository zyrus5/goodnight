use std::sync::Arc;

use axum::{
    Router,
    http::{HeaderValue, Method, header},
    routing::{get, post, put},
};
use sqlx::PgPool;
use tokio::sync::RwLock;
use tower_http::{
    cors::CorsLayer,
    trace::{DefaultMakeSpan, DefaultOnResponse, TraceLayer},
};
use tracing::Level;
use uuid::Uuid;

use crate::{config::Config, crypto::Keyring, frontend, jenkins::JenkinsClient, routes};

#[derive(Clone)]
pub struct AppState {
    pub db: PgPool,
    pub config: Arc<Config>,
    pub crypto: Arc<Keyring>,
    pub jenkins: JenkinsClient,
    pub instance_id: Uuid,
    pub worker_user_id: Arc<RwLock<Option<Uuid>>>,
}

pub fn router(state: AppState) -> Router {
    let cors = CorsLayer::new()
        .allow_origin(
            state
                .config
                .frontend_origin
                .parse::<HeaderValue>()
                .expect("FRONTEND_ORIGIN must be valid"),
        )
        .allow_credentials(true)
        .allow_methods([
            Method::GET,
            Method::POST,
            Method::PUT,
            Method::PATCH,
            Method::DELETE,
        ])
        .allow_headers([
            header::CONTENT_TYPE,
            header::ACCEPT,
            header::AUTHORIZATION,
            header::HeaderName::from_static("idempotency-key"),
        ]);
    let api = Router::new()
        .route("/health", get(routes::health::check))
        .route("/metrics", get(routes::system::metrics))
        .route("/auth/login", post(routes::auth::login))
        .route("/auth/logout", post(routes::auth::logout))
        .route("/auth/me", get(routes::auth::me))
        .route("/dashboard", get(routes::system::dashboard))
        .route(
            "/users",
            get(routes::users::list).post(routes::users::create),
        )
        .route("/users/{id}", put(routes::users::update))
        .route(
            "/customers",
            get(routes::catalog::customers).post(routes::catalog::create_customer),
        )
        .route("/customers/{id}", put(routes::catalog::update_customer))
        .route(
            "/components",
            get(routes::catalog::components).post(routes::catalog::create_component),
        )
        .route("/components/{id}", put(routes::catalog::update_component))
        .route(
            "/components/{id}/members/{role}",
            put(routes::catalog::update_component_members),
        )
        .route(
            "/environments",
            get(routes::catalog::environments).post(routes::catalog::create_environment),
        )
        .route(
            "/environments/{id}",
            put(routes::catalog::update_environment),
        )
        .route(
            "/environments/{id}/test",
            post(routes::catalog::test_environment),
        )
        .route(
            "/environments/{id}/folders",
            get(routes::catalog::discover_folders),
        )
        .route(
            "/component-instances",
            get(routes::catalog::instances).post(routes::catalog::create_instance),
        )
        .route(
            "/component-instances/{id}",
            put(routes::catalog::update_instance).delete(routes::catalog::delete_instance),
        )
        .route(
            "/component-instances/{id}/jobs/discover",
            get(routes::catalog::discover_jobs),
        )
        .route(
            "/component-instances/{id}/jobs/preview",
            post(routes::catalog::preview_instance_job),
        )
        .route(
            "/component-instances/{id}/jobs/test",
            post(routes::catalog::test_instance_job),
        )
        .route(
            "/component-instances/{id}/jobs/test/queue",
            post(routes::catalog::test_instance_job_queue),
        )
        .route(
            "/job-configs",
            get(routes::catalog::jobs).post(routes::catalog::create_job),
        )
        .route("/job-configs/{id}", put(routes::catalog::update_job))
        .route("/job-configs/{id}/sync", post(routes::catalog::sync_job))
        .route(
            "/tasks",
            get(routes::tasks::list).post(routes::tasks::create),
        )
        .route("/tasks/cron-preview", post(routes::tasks::preview))
        .route(
            "/tasks/{id}",
            get(routes::tasks::get)
                .put(routes::tasks::update)
                .delete(routes::tasks::delete_task),
        )
        .route("/tasks/{id}/toggle", post(routes::tasks::toggle))
        .route("/tasks/{id}/run", post(routes::tasks::run))
        .route("/executions", get(routes::tasks::executions))
        .route(
            "/executions/{id}",
            get(routes::tasks::execution_detail).delete(routes::tasks::delete_execution),
        )
        .route("/executions/{id}/stop", post(routes::tasks::stop_execution))
        .route("/executions/{id}/copy", post(routes::tasks::copy_execution))
        .route(
            "/executions/{execution_id}/nodes/{node_id}/log",
            get(routes::tasks::node_log),
        )
        .route("/audit-logs", get(routes::system::audits))
        .fallback(api_not_found);
    Router::new()
        .nest("/api", api)
        .fallback(frontend::serve)
        .with_state(state)
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
