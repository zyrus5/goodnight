use std::{env, net::SocketAddr};

use anyhow::{Context, Result};

#[derive(Debug, Clone)]
pub struct Config {
    pub host: String,
    pub port: u16,
    pub database_url: String,
    pub frontend_origin: String,
}

impl Config {
    pub fn from_env() -> Result<Self> {
        let port = env::var("APP_PORT")
            .unwrap_or_else(|_| "3000".to_owned())
            .parse()
            .context("APP_PORT must be a valid port number")?;

        Ok(Self {
            host: env::var("APP_HOST").unwrap_or_else(|_| "127.0.0.1".to_owned()),
            port,
            database_url: env::var("DATABASE_URL").unwrap_or_else(|_| {
                "postgres://postgres:postgres@localhost:5432/goodnight".to_owned()
            }),
            frontend_origin: env::var("FRONTEND_ORIGIN")
                .unwrap_or_else(|_| "http://localhost:5173".to_owned()),
        })
    }

    pub fn address(&self) -> SocketAddr {
        format!("{}:{}", self.host, self.port)
            .parse()
            .expect("APP_HOST and APP_PORT must form a valid socket address")
    }
}

#[cfg(test)]
mod tests {
    use super::Config;

    #[test]
    fn default_address_is_localhost_port_3000() {
        let config = Config {
            host: "127.0.0.1".to_owned(),
            port: 3000,
            database_url: String::new(),
            frontend_origin: String::new(),
        };

        assert_eq!(config.address().to_string(), "127.0.0.1:3000");
    }
}
