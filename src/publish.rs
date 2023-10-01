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

#[instrument(skip(state, body))]
pub async fn publish(
    State(state): State<PublishState>,
    Json(body): Json<PublishBody>,
) -> ServerResult<Response> {
    let mut valids = 0usize;
    let mut totals = 0usize;

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
    .map(|obj| EmailAdderess::new(obj.email))
    .inspect(|res| {
        if res.is_ok() {
            valids = valids + 1;
        }
        totals = totals + 1;
    })
    .filter_map(|res| match res {
        Ok(email) => Some(email),
        Err(err) => {
            tracing::warn!(err = ?err.context("Skipping subscriber due to invalid data"));
            None
        }
    })
    .collect();

    for (i, res) in futures::future::join_all(confirmed_emails.iter().map(|email| {
        state
            .email_client
            .email(&email, &body.title, &body.content.text, &body.content.html)
    }))
    .await
    .into_iter()
    .enumerate()
    {
        if let Err(err) = res {
            tracing::warn!(to = confirmed_emails[i].as_ref(), err = ?err.context("Failed to send email"));
        }
    }

    Ok((
        StatusCode::OK,
        format!("Dispatched content to {valids}/{totals} subscribers.",),
    )
        .into_response())
}
