use crate::email::{EmailAdderess, EmailClient};
use crate::ServerResult;
use anyhow::{bail, Context, Result};
use axum::extract::Query;
use axum::{extract::State, http::StatusCode, response::IntoResponse, Form};
use chrono::Utc;
use rand::Rng;
use sqlx::PgPool;
use std::str::FromStr;
use std::sync::Arc;
use tracing::{info, instrument};
use unicode_segmentation::UnicodeSegmentation;
use uuid::Uuid;

const SUBSCRIPTION_TOKEN_LEN: usize = 26;

fn subscription_token(len: usize) -> String {
    let mut rng = rand::thread_rng();
    std::iter::repeat_with(|| rng.sample(rand::distributions::Alphanumeric))
        .map(char::from)
        .take(len)
        .collect()
}

#[derive(Debug, serde::Deserialize)]
#[allow(dead_code)]
pub struct SubscriptionForm {
    pub name: SubscriberName,
    pub email: EmailAdderess,
}

#[derive(Debug, serde::Deserialize)]
#[allow(dead_code)]
pub struct SubscriptionConfirmQuery {
    pub token: String,
}

#[derive(Debug, Default, strum::Display, strum::EnumString)]
pub enum SubscriptionStatus {
    #[default]
    Pending,
    Confirmed,
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

#[derive(Debug, Clone)]
pub struct SubscribeState {
    pub pool: PgPool,
    pub email_client: Arc<EmailClient>,
    pub subscribe_confirm_endpoint: reqwest::Url,
}

/// Content-Type: application/x-www-form-urlencoded
#[instrument(
    skip(state, form),
    fields(
        email = form.email.as_ref(),
        name = form.name.as_ref()
    )
)]
pub async fn subscribe(
    State(state): State<SubscribeState>,
    Form(form): Form<SubscriptionForm>,
) -> ServerResult<impl IntoResponse> {
    async fn email_subscription_confirmation(
        email_client: Arc<EmailClient>,
        to: &EmailAdderess,
        mut subscription_url: reqwest::Url,
        subscription_token: &str,
    ) -> Result<()> {
        subscription_url.set_query(Some(&format!("token={}", subscription_token)));
        email_client
            .email(
                to,
                "Newsletter subscription confirmation",
                &format!(
                    "Open the link to confirm your newsletter subscription. {subscription_url}",
                ),
                &format!(
                    "
                <p>
                    Open the link to confirm your newsletter subscription.<br />
                    <a href='{0}'>{0}</a>
                </p>",
                    subscription_url
                ),
            )
            .await
            .context("Failed to send a confirmation email, please try again later.")?;

        info!(%subscription_url, "Sent a confirmation email");

        Ok(())
    }

    match sqlx::query!(
        r#"
        SELECT status FROM subscribers
        WHERE email = $1
        "#,
        form.email.as_ref()
    )
    .fetch_optional(&state.pool)
    .await?
    .map(|obj| SubscriptionStatus::from_str(&obj.status).expect("Stored value must be valid"))
    {
        Some(SubscriptionStatus::Confirmed) => {
            info!("Already subscribed and confirmed");
            Ok((
                StatusCode::OK,
                format!(
                    "{} is already subscribed and confirmed",
                    form.email.as_ref()
                ),
            ))
        }
        // Send a new confirmation email
        Some(SubscriptionStatus::Pending) => {
            let uuid = sqlx::query!(
                r#"
                SELECT id FROM subscribers
                WHERE email = $1
                "#,
                form.email.as_ref(),
            )
            .fetch_optional(&state.pool)
            .await?
            .map(|obj| obj.id)
            .expect("Subscriber must exist if we're in the Status::Pending branch");

            let subscription_token = subscription_token(SUBSCRIPTION_TOKEN_LEN);
            sqlx::query!(
                r#"
                UPDATE subscription_tokens
                SET subscription_token = $1
                WHERE subscriber_id = $2
                "#,
                subscription_token,
                uuid
            )
            .execute(&state.pool)
            .await?;

            email_subscription_confirmation(
                state.email_client,
                &form.email,
                state.subscribe_confirm_endpoint.clone(),
                &subscription_token,
            )
            .await?;

            Ok((
                StatusCode::OK,
                "A confirmation email has been sent again.".into(),
            ))
        }
        // Add subscriber
        None => {
            let mut transaction = state.pool.begin().await?;

            let uuid = Uuid::new_v4();
            sqlx::query!(
                r#"
                INSERT INTO subscribers (id, email, name, status, subscribed_at)
                VALUES ($1, $2, $3, $4, $5)
                "#,
                uuid,
                form.email.as_ref(),
                form.name.as_ref(),
                SubscriptionStatus::default().to_string(),
                Utc::now()
            )
            .execute(&mut *transaction)
            .await?;

            let subscription_token = subscription_token(SUBSCRIPTION_TOKEN_LEN);
            sqlx::query!(
                r#"
                INSERT INTO subscription_tokens (subscription_token, subscriber_id)
                VALUES ($1, $2)
                "#,
                subscription_token,
                uuid
            )
            .execute(&mut *transaction)
            .await?;

            transaction.commit().await?;

            info!(
                ?uuid,
                subscription_token, "Subscriber added to the database"
            );

            email_subscription_confirmation(
                state.email_client,
                &form.email,
                state.subscribe_confirm_endpoint.clone(),
                &subscription_token,
            )
            .await?;

            Ok((StatusCode::OK, "A confirmation email has been sent.".into()))
        }
    }
}

#[instrument(
    skip(pool, query),
    fields(token = query.token)
)]
pub async fn subscribe_confirm(
    State(pool): State<PgPool>,
    Query(query): Query<SubscriptionConfirmQuery>,
) -> ServerResult<impl IntoResponse> {
    let uuid = sqlx::query!(
        r#"
        SELECT subscriber_id FROM subscription_tokens
        WHERE subscription_token = $1
        "#,
        query.token
    )
    .fetch_optional(&pool)
    .await?
    .map(|obj| obj.subscriber_id)
    .context("No such subscription token found.")?;

    sqlx::query!(
        r#"
        UPDATE subscribers
        SET status = $1
        WHERE id = $2
        "#,
        SubscriptionStatus::Confirmed.to_string(),
        uuid
    )
    .execute(&pool)
    .await?;

    info!(?uuid, "Subscription confirmed");

    Ok((StatusCode::OK, "Subscription Confirmed!"))
}
