use crate::{
    email::{EmailAdderess, EmailClient},
    subscribe::SubscriptionStatus,
    ServerResult,
};
use axum::response::IntoResponse;
use axum::{
    extract::{Json, State},
    http::StatusCode,
    response::Response,
};
use sqlx::PgPool;
use std::sync::Arc;
use tracing::instrument;

#[derive(Debug, serde::Deserialize)]
pub struct PublishBody {
    title: String,
    content: PublishContent,
}

#[derive(Debug, serde::Deserialize)]
pub struct PublishContent {
    text: String,
    html: String,
}

#[derive(Debug, Clone)]
pub struct PublishState {
    pub pool: PgPool,
    pub email_client: Arc<EmailClient>,
}

#[instrument(skip(state))]
pub async fn publish(
    State(state): State<PublishState>,
    Json(body): Json<PublishBody>,
) -> ServerResult<Response> {
    let confirmed_emails: Vec<EmailAdderess> = sqlx::query!(
        r#"
        SELECT email FROM subscribers
        WHERE status = $1
        "#,
        SubscriptionStatus::Confirmed.to_string()
    )
    .fetch_all(&state.pool)
    .await?
    .into_iter()
    .filter_map(|obj| match EmailAdderess::new(obj.email) {
        Ok(email) => Some(email),
        Err(err) => {
            tracing::warn!(err = ?err.context("Skipping subscriber due to invalid data"));
            None
        }
    })
    .collect();

    for email in confirmed_emails {
        if let Err(err) = state
            .email_client
            .email(&email, &body.title, &body.content.text, &body.content.html)
            .await
        {
            tracing::warn!(to = email.as_ref(), err = ?err.context("Failed to send email"));
        }
    }

    Ok((StatusCode::OK, "Dispatched news to subscribers.").into_response())
}
