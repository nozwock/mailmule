use std::str::FromStr;

#[derive(Debug)]
pub struct Url(pub reqwest::Url);

impl std::fmt::Display for Url {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0.as_str())
    }
}

impl serde::Serialize for Url {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(self.0.as_str())
    }
}

impl<'de> serde::Deserialize<'de> for Url {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        reqwest::Url::parse(&String::deserialize(deserializer)?)
            .map(|inner| Self(inner))
            .map_err(serde::de::Error::custom)
    }
}

#[derive(Debug)]
pub struct SocketAddr(pub std::net::SocketAddr);

impl serde::Serialize for SocketAddr {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.0.to_string())
    }
}

impl<'de> serde::Deserialize<'de> for SocketAddr {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        String::deserialize(deserializer)?
            .parse::<std::net::SocketAddr>()
            .map(|inner| Self(inner))
            .map_err(serde::de::Error::custom)
    }
}

#[derive(Debug)]
pub struct PgConnectOptions(pub sqlx::postgres::PgConnectOptions);

impl<'de> serde::Deserialize<'de> for PgConnectOptions {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        sqlx::postgres::PgConnectOptions::from_str(&String::deserialize(deserializer)?)
            .map(|inner| Self(inner))
            .map_err(serde::de::Error::custom)
    }
}
