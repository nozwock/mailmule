use crate::{email::EmailAdderess, helpers};
use anyhow::Result;
use std::time::Duration;

#[derive(Debug, serde::Deserialize)]
#[allow(dead_code)]
pub struct Config {
    pub app: AppConfig,
    pub database: DatabaseConfig,
    pub email_client: EmailClientConfig,
}

#[derive(Debug, serde::Deserialize)]
#[allow(dead_code)]
pub struct AppConfig {
    pub socket_addr: helpers::SocketAddr,
    pub public_url: Option<helpers::Url>,
}

#[derive(Debug, serde::Deserialize)]
#[allow(dead_code)]
pub struct DatabaseConfig {
    pub url: helpers::PgConnectOptions,
}

#[serde_with::serde_as]
#[derive(Debug, serde::Deserialize)]
#[allow(dead_code)]
pub struct EmailClientConfig {
    pub api_url: helpers::Url,
    pub api_token: String,
    pub sender_email: EmailAdderess,
    #[serde_as(as = "serde_with::DurationMilliSeconds<u64>")]
    pub timeout_ms: Duration,
}

impl Config {
    pub fn load() -> Result<Config> {
        config::Config::builder()
            .add_source(config::File::with_name("mailmule"))
            .add_source(config::Environment::with_prefix("MM"))
            .set_default("email_client.api_url", "https://api.postmarkapp.com")?
            .set_default("email_client.api_token", "POSTMARK_API_TEST")?
            .set_default("email_client.timeout_ms", "10000")?
            .build()?
            .try_deserialize()
            .map_err(anyhow::Error::from)
    }
}

impl AppConfig {
    pub fn base_url(&self) -> Result<reqwest::Url> {
        Ok(self
            .public_url
            .as_ref()
            .map(|url| url.0.clone())
            .unwrap_or({
                reqwest::Url::parse(&format!("http://{}", self.socket_addr.0.to_string()))?
            }))
    }
}
