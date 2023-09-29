use anyhow::{bail, Result};

#[derive(Debug)]
pub struct Email(String);

impl Email {
    pub fn new(s: String) -> Result<Self> {
        if validator::validate_email(&s) {
            Ok(Self(s))
        } else {
            bail!("The given email is invalid.")
        }
    }
}

impl AsRef<str> for Email {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl<'de> serde::Deserialize<'de> for Email {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        Email::new(s).map_err(serde::de::Error::custom)
    }
}
