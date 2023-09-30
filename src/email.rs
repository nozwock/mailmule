use crate::config::EmailClientConfig;
use anyhow::{bail, Result};
use std::time::Duration;

#[derive(Debug, Clone, serde::Serialize)]
pub struct EmailAdderess(String);

impl EmailAdderess {
    pub fn new(s: String) -> Result<Self> {
        if validator::validate_email(&s) {
            Ok(Self(s))
        } else {
            bail!("The given email is invalid.")
        }
    }
}

impl AsRef<str> for EmailAdderess {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl<'de> serde::Deserialize<'de> for EmailAdderess {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        EmailAdderess::new(s).map_err(serde::de::Error::custom)
    }
}

/// PostMark is used for this EmailClient.
#[derive(Debug, Clone)]
pub struct EmailClient {
    pub client: reqwest::Client,
    pub api_url: String,
    pub api_token: String,
    pub sender_email: EmailAdderess,
}

impl EmailClient {
    pub fn new(
        timeout: Duration,
        api_url: String,
        api_token: String,
        sender_email: EmailAdderess,
    ) -> Result<Self> {
        Ok(Self {
            client: reqwest::Client::builder().timeout(timeout).build()?,
            api_url,
            api_token,
            sender_email,
        })
    }
}

impl TryFrom<EmailClientConfig> for EmailClient {
    type Error = anyhow::Error;

    fn try_from(value: EmailClientConfig) -> std::result::Result<Self, Self::Error> {
        Self::new(
            value.timeout_ms,
            value.api_url,
            value.api_token,
            value.sender_email,
        )
    }
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct EmailRequestBody<'a> {
    pub from: &'a str,
    pub to: &'a str,
    pub subject: &'a str,
    pub text_body: &'a str,
    pub html_body: &'a str,
}

impl EmailClient {
    /// https://postmarkapp.com/developer/user-guide/send-email-with-api/send-a-single-email
    pub async fn email(
        &self,
        to: &EmailAdderess,
        subject: &str,
        text_body: &str,
        html_body: &str,
    ) -> Result<()> {
        let request_body = EmailRequestBody {
            from: self.sender_email.as_ref(),
            to: to.as_ref(),
            subject,
            text_body,
            html_body,
        };

        let send_email_api_endpoint = format!("{}/email", self.api_url);
        let resp = self
            .client
            .post(&send_email_api_endpoint)
            .header("X-Postmark-Server-Token", &self.api_token)
            .json(&request_body)
            .send()
            .await?
            .error_for_status()?;

        // TODO: parse json response body. and evaluate ErrorCode
        tracing::debug!(response_body = %resp.text().await.unwrap_or("Empty response body".into()));

        Ok(())
    }
}
