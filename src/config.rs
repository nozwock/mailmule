use anyhow::Result;

#[derive(Debug, serde::Deserialize)]
#[allow(dead_code)]
pub struct Config {
    pub server: ServerConfig,
    pub db: DatabaseConfig,
}

#[derive(Debug, serde::Deserialize)]
#[allow(dead_code)]
pub struct ServerConfig {
    pub port: u16,
}

#[derive(Debug, serde::Deserialize)]
#[allow(dead_code)]
pub struct DatabaseConfig {
    pub username: String,
    pub password: String,
    pub host: String,
    pub port: u16,
    pub name: String,
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
    pub fn as_url(self) -> String {
        format!(
            "postgres://{}:{}@{}:{}/{}",
            self.username, self.password, self.host, self.port, self.name
        )
    }
}
