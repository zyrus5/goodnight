use anyhow::{Context, Result};
use sqlx::{PgPool, postgres::PgPoolOptions};

pub fn connect(database_url: &str) -> Result<PgPool> {
    PgPoolOptions::new()
        .max_connections(10)
        .connect_lazy(database_url)
        .context("DATABASE_URL is invalid")
}
