use anyhow::Result;
use sqlx::postgres::PgConnectOptions;

use crate::email::Email;

#[derive(Debug, serde::Deserialize)]
#[allow(dead_code)]
pub struct Config {
    pub app: AppConfig,
    pub db: DatabaseConfig,
    pub email_client: EmailClientConfig,
}

#[derive(Debug, serde::Deserialize)]
#[allow(dead_code)]
pub struct AppConfig {
    pub host: String,
    pub port: u16,
}

#[derive(Debug, serde::Deserialize)]
#[allow(dead_code)]
pub struct DatabaseConfig {
    pub username: String,
    pub password: String,
    pub host: String,
    pub port: u16,
    pub database: String,
}

#[derive(Debug, serde::Deserialize)]
#[allow(dead_code)]
pub struct EmailClientConfig {
    pub api_url: String,
    pub sender_email: Email,
}

impl Config {
    pub fn load() -> Result<Config> {
        config::Config::builder()
            .add_source(config::File::with_name("mailmule"))
            .add_source(config::Environment::with_prefix("MM"))
            .build()?
            .try_deserialize()
            .map_err(anyhow::Error::from)
    }
}

impl DatabaseConfig {
    pub fn as_url(&self, with_db: bool) -> PgConnectOptions {
        let options = PgConnectOptions::new()
            .host(&self.host)
            .port(self.port)
            .username(&self.username)
            .password(&self.password);

        if with_db {
            options.database(&self.database)
        } else {
            options
        }
    }
}
