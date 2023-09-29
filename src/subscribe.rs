use crate::email::EmailAdderess;
use anyhow::{bail, Result};
use unicode_segmentation::UnicodeSegmentation;

#[derive(Debug, serde::Deserialize)]
#[allow(dead_code)]
pub struct SubscriptionForm {
    pub name: SubscriberName,
    pub email: EmailAdderess,
}

#[derive(Debug)]
pub struct SubscriberName(String);

impl SubscriberName {
    pub fn new(s: String) -> Result<Self> {
        let is_empty = s.trim().is_empty();
        let chars = s.graphemes(true).count();
        let has_forbidden_chars = s
            .chars()
            .any(|c| ['/', '(', ')', '"', '<', '>', '\\', '}', '{'].contains(&c));

        if is_empty || chars > 256 || has_forbidden_chars {
            bail!(
                "The given name is invalid, it must be non-empty, not longer than 256 characters, and not containing the following \
                characters '/', '(', ')', '\"', '<', '>', '\\', '}}', '{{'."
            )
        } else {
            Ok(Self(s))
        }
    }
}

impl AsRef<str> for SubscriberName {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl<'de> serde::Deserialize<'de> for SubscriberName {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        SubscriberName::new(s).map_err(serde::de::Error::custom)
    }
}
