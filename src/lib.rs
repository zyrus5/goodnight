pub mod app;
pub mod audit;
pub mod auth;
pub mod config;
pub mod crypto;
pub mod db;
pub mod error;
pub mod execution;
pub mod frontend;
pub mod jenkins;
pub mod models;
pub mod routes;

use anyhow::Context;
use std::{process::Command, sync::Arc};
use tokio::net::TcpListener;
use tracing::info;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};
use uuid::Uuid;

pub async fn run() -> anyhow::Result<()> {
    load_dotenv();
    init_tracing();
    let config = Arc::new(config::Config::from_env()?);
    let crypto = Arc::new(crypto::Keyring::from_env()?);
    let pool = db::connect(&config.database_url)?;
    sqlx::migrate!()
        .run(&pool)
        .await
        .context("database migration failed")?;
    auth::bootstrap_admin(&pool, config.clone()).await?;
    let jenkins = jenkins::JenkinsClient::new(config.clone());
    let state = app::AppState {
        db: pool,
        config: config.clone(),
        crypto,
        jenkins,
        instance_id: Uuid::new_v4(),
        worker_user_id: Arc::new(tokio::sync::RwLock::new(None)),
    };
    execution::spawn_background(state.clone());
    let application = app::router(state);
    let listener = TcpListener::bind(config.address())
        .await
        .with_context(|| format!("failed to bind {}", config.address()))?;
    info!(address=%config.address(),"API server listening");
    if config.open_browser {
        open_browser(&config.host, config.port);
    }
    axum::serve(listener, application)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .context("API server stopped unexpectedly")
}
fn open_browser(host: &str, port: u16) {
    let browser_host = match host {
        "0.0.0.0" | "::" => "127.0.0.1",
        value => value,
    };
    let url = format!("http://{browser_host}:{port}");
    #[cfg(target_os = "macos")]
    let result = Command::new("open").arg(&url).spawn();
    #[cfg(target_os = "windows")]
    let result = Command::new("cmd").args(["/C", "start", "", &url]).spawn();
    #[cfg(all(unix, not(target_os = "macos")))]
    let result = Command::new("xdg-open").arg(&url).spawn();
    if let Err(error) = result {
        tracing::warn!(%error, %url, "failed to open browser");
    }
}
fn load_dotenv() {
    if dotenvy::dotenv().is_ok() {
        return;
    }
    if let Ok(executable) = std::env::current_exe()
        && let Some(directory) = executable.parent()
    {
        dotenvy::from_path(directory.join(".env")).ok();
    }
}
fn init_tracing() {
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "goodnight=debug,tower_http=info".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();
}
async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };
    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("failed to install SIGTERM handler")
            .recv()
            .await;
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();
    tokio::select! {()=ctrl_c=>{},()=terminate=>{},}
}
