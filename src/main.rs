use anyhow::{Context, Result};
use axum::{
    body::Bytes,
    extract::{Form, MatchedPath, Query, State},
    http::{HeaderMap, Request, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
    Router,
};
use chrono::Utc;
use mailmule::{
    config::Config,
    email::{EmailAdderess, EmailClient},
    helpers::SocketAddr,
    subscribe::{SubscriptionConfirmQuery, SubscriptionForm, SubscriptionStatus},
};
use rand::Rng;
use sqlx::PgPool;
use std::{str::FromStr, sync::Arc, time::Duration};
use tokio::net::TcpListener;
use tower_http::{classify::ServerErrorsFailureClass, trace::TraceLayer};
use tracing::{error, info, info_span, instrument, Span};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};
use uuid::Uuid;

type ServerResult<T, E = ServerError> = core::result::Result<T, E>;

const SUBSCRIPTION_TOKEN_LEN: usize = 26;

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
            &format!("Open the link to confirm your newsletter subscription. {subscription_url}",),
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
        .map_err(|e| anyhow::anyhow!("Failed to send a confirmation email\nCause:\n\t{}", e))?;

    info!(%subscription_url, "Sent a confirmation email");

    Ok(())
}

#[derive(Debug, Clone)]
struct SubscribeState {
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
async fn subscribe(
    State(state): State<SubscribeState>,
    Form(form): Form<SubscriptionForm>,
) -> ServerResult<impl IntoResponse> {
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
async fn subscribe_confirm(
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

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::Registry::default()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| {
                // axum logs rejections from built-in extractors with the `axum::rejection`
                // target, at `TRACE` level. `axum::rejection=trace` enables showing those events
                "mailmule=debug,tower_http=debug,axum::rejection=trace".into()
            }),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    let mut cfg = Config::load()?;
    let email_client = Arc::new(EmailClient::try_from(cfg.email_client)?);

    let pg_opts = cfg.database.url.0;
    info!(pg_opts = ?pg_opts.clone().password("REDACTED"), "Connecting to the database");
    let pool = sqlx::PgPool::connect_with(pg_opts).await?;
    info!("Connected to the database");

    let app = Router::new()
        .route("/", get(|| async { "Hello, World!" }))
        .route("/health", get(|| async { StatusCode::OK }))
        .route(
            "/subscribe",
            post(subscribe).with_state(SubscribeState {
                pool: pool.clone(),
                email_client: email_client.clone(),
                subscribe_confirm_endpoint: cfg
                    .app
                    .base_url()?
                    .join("subscribe/")?
                    .join("confirm")?,
            }),
        )
        .route(
            "/subscribe/confirm",
            get(subscribe_confirm).with_state(pool.clone()),
        )
        .layer(
            TraceLayer::new_for_http()
                .make_span_with(|request: &Request<_>| {
                    // Log the matched route's path (with placeholders not filled in).
                    // Use request.uri() or OriginalUri if you want the real path.
                    let matched_path = request
                        .extensions()
                        .get::<MatchedPath>()
                        .map(MatchedPath::as_str);

                    info_span!(
                        "http_request",
                        method = ?request.method(),
                        matched_path,
                        some_other_field = tracing::field::Empty,
                    )
                })
                .on_request(|_request: &Request<_>, _span: &Span| {
                    // You can use `_span.record("some_other_field", value)` in one of these
                    // closures to attach a value to the initially empty field in the info_span
                    // created above.
                })
                .on_response(|_response: &Response, _latency: Duration, _span: &Span| {
                    // ...
                })
                .on_body_chunk(|_chunk: &Bytes, _latency: Duration, _span: &Span| {
                    // ...
                })
                .on_eos(
                    |_trailers: Option<&HeaderMap>, _stream_duration: Duration, _span: &Span| {
                        // ...
                    },
                )
                .on_failure(
                    |_error: ServerErrorsFailureClass, _latency: Duration, _span: &Span| {
                        // ...
                    },
                ),
        );

    let listener = TcpListener::bind(cfg.app.socket_addr.0).await?;
    // Updating socket after listening
    cfg.app.socket_addr = SocketAddr(listener.local_addr()?);

    info!(
        addr = format!("http://{}", cfg.app.socket_addr.0.to_string()),
        "Starting server on"
    );
    axum::Server::from_tcp(listener.into_std()?)?
        .serve(app.into_make_service())
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    Ok(())
}

fn subscription_token(len: usize) -> String {
    let mut rng = rand::thread_rng();
    std::iter::repeat_with(|| rng.sample(rand::distributions::Alphanumeric))
        .map(char::from)
        .take(len)
        .collect()
}

#[derive(Debug)]
struct ServerError(anyhow::Error);

impl IntoResponse for ServerError {
    fn into_response(self) -> Response {
        (StatusCode::INTERNAL_SERVER_ERROR, self.0.to_string()).into_response()
    }
}

impl<E> From<E> for ServerError
where
    E: Into<anyhow::Error>,
{
    fn from(value: E) -> Self {
        let err = value.into();
        error!(?err);
        Self(err)
    }
}

async fn shutdown_signal() {
    use tokio::signal;

    let ctrl_c = async {
        signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("failed to install signal handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }

    eprintln!("starting shutdown");
}
