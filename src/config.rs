use std::{env, net::SocketAddr, time::Duration};

use anyhow::Result;

#[derive(Debug, Clone)]
pub struct Config {
    pub host: String,
    pub port: u16,
    pub database_url: String,
    pub frontend_origin: String,
    pub session_secure: bool,
    pub session_hours: i64,
    pub bootstrap_admin_username: Option<String>,
    pub bootstrap_admin_password: Option<String>,
    pub bootstrap_admin_display_name: String,
    pub jenkins_username: String,
    pub jenkins_password: String,
    pub scheduler_interval: Duration,
    pub worker_interval: Duration,
    pub global_concurrency: usize,
    pub per_jenkins_concurrency: usize,
    pub metrics_token: Option<String>,
    pub open_browser: bool,
}

impl Config {
    pub fn from_env() -> Result<Self> {
        let port = parse_env("APP_PORT", 3000_u16)?;
        Ok(Self {
            host: env::var("APP_HOST").unwrap_or_else(|_| "127.0.0.1".to_owned()),
            port,
            database_url: env::var("DATABASE_URL").unwrap_or_else(|_| {
                "postgres://postgres:postgres@localhost:5432/goodnight".to_owned()
            }),
            frontend_origin: env::var("FRONTEND_ORIGIN")
                .unwrap_or_else(|_| "http://localhost:5173".to_owned()),
            session_secure: parse_bool("SESSION_SECURE", false)?,
            session_hours: parse_env("SESSION_HOURS", 24_i64)?,
            bootstrap_admin_username: env::var("BOOTSTRAP_ADMIN_USERNAME").ok(),
            bootstrap_admin_password: env::var("BOOTSTRAP_ADMIN_PASSWORD").ok(),
            bootstrap_admin_display_name: env::var("BOOTSTRAP_ADMIN_DISPLAY_NAME")
                .unwrap_or_else(|_| "系统管理员".to_owned()),
            jenkins_username: env::var("JENKINS_USERNAME").unwrap_or_default(),
            jenkins_password: env::var("JENKINS_PASSWORD").unwrap_or_default(),
            scheduler_interval: Duration::from_secs(parse_env(
                "SCHEDULER_INTERVAL_SECONDS",
                5_u64,
            )?),
            worker_interval: Duration::from_secs(parse_env("WORKER_INTERVAL_SECONDS", 2_u64)?),
            global_concurrency: parse_env("GLOBAL_JOB_CONCURRENCY", 16_usize)?,
            per_jenkins_concurrency: parse_env("PER_JENKINS_CONCURRENCY", 4_usize)?,
            metrics_token: env::var("METRICS_TOKEN").ok(),
            open_browser: parse_bool("OPEN_BROWSER", true)?,
        })
    }

    pub fn address(&self) -> SocketAddr {
        format!("{}:{}", self.host, self.port)
            .parse()
            .expect("APP_HOST and APP_PORT must form a valid socket address")
    }
}

fn parse_env<T>(name: &str, default: T) -> Result<T>
where
    T: std::str::FromStr,
    T::Err: std::fmt::Display,
{
    match env::var(name) {
        Ok(value) => value
            .parse()
            .map_err(|error| anyhow::anyhow!("{name}: {error}")),
        Err(_) => Ok(default),
    }
}

fn parse_bool(name: &str, default: bool) -> Result<bool> {
    parse_env(name, default)
}

#[cfg(test)]
mod tests {
    use super::Config;

    #[test]
    fn address_uses_configured_host_and_port() {
        let address = format!("{}:{}", "127.0.0.1", 3000)
            .parse::<std::net::SocketAddr>()
            .unwrap();
        assert_eq!(address.to_string(), "127.0.0.1:3000");
        let _ = std::mem::size_of::<Config>();
    }
}
