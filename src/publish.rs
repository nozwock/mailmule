use crate::{
    email::{EmailAdderess, EmailClient},
    subscribe::SubscriptionStatus,
    ServerError, ServerResult,
};
use axum::response::IntoResponse;
use axum::{
    extract::{Json, State},
    http::StatusCode,
    response::Response,
};
use futures::future;
use sqlx::PgPool;
use std::sync::Arc;
use tracing::{info, instrument, warn};

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

#[instrument(
    skip(state, body),
    fields(title = body.title)
)]
pub async fn publish(
    State(state): State<PublishState>,
    Json(body): Json<PublishBody>,
) -> ServerResult<Response> {
    let mut valids = 0usize;
    let mut total = 0usize;
    let mut fails = 0usize;

    let confirmed_emails: Vec<EmailAdderess> = sqlx::query!(
        r#"
        SELECT email FROM subscribers
        WHERE status = $1
        "#,
        SubscriptionStatus::Confirmed.to_string()
    )
    .fetch_all(&state.pool)
    .await
    .map_err(ServerError::unexpected)?
    .into_iter()
    .map(|obj| EmailAdderess::new(obj.email))
    .inspect(|res| {
        if res.is_ok() {
            valids = valids + 1;
        }
        total = total + 1;
    })
    .filter_map(|res| match res {
        Ok(email) => Some(email),
        Err(err) => {
            warn!(err = ?err.context("Skipping subscriber due to invalid data"));
            None
        }
    })
    .collect();

    info!("Evaluated valid email addresses, now sending emails");

    for (i, res) in future::join_all(confirmed_emails.iter().map(|email| {
        state
            .email_client
            .send_email(&email, &body.title, &body.content.text, &body.content.html)
    }))
    .await
    .into_iter()
    .enumerate()
    {
        if let Err(err) = res {
            warn!(to = confirmed_emails[i].as_ref(), err = ?err.context("Failed to send email"));
            fails = fails + 1;
        }
    }

    info!(valids, fails, total, "Dispatched content to subscribers");

    Ok((
        StatusCode::OK,
        format!(
            "Dispatched content to {}/{total} subscribers.",
            valids - fails
        ),
    )
        .into_response())
}
