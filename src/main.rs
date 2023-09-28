mod config;

use axum::{
    body::Bytes,
    extract::{Form, MatchedPath, State},
    http::{HeaderMap, Request, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
    Router,
};
use chrono::Utc;
use serde::Deserialize;
use sqlx::PgPool;
use std::time::Duration;
use tokio::net::TcpListener;
use tower_http::{classify::ServerErrorsFailureClass, trace::TraceLayer};
use tracing::{error, info, info_span, instrument, Span};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};
use uuid::Uuid;

type ServerResult<T, E = ServerError> = core::result::Result<T, E>;

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct SubscriptionInfo {
    name: String,
    email: String,
}

/// Content-Type: application/x-www-form-urlencoded
#[instrument(
    skip(pool, form),
    fields(
        email = %form.email,
        name = %form.name
    )
)]
async fn subscribe(
    State(pool): State<PgPool>,
    Form(form): Form<SubscriptionInfo>,
) -> ServerResult<StatusCode> {
    let uuid = Uuid::new_v4();
    sqlx::query!(
        r#"
        INSERT INTO subscribers (id, email, name, subscribed_at)
        VALUES ($1, $2, $3, $4)
        "#,
        uuid,
        form.email,
        form.name,
        Utc::now()
    )
    .execute(&pool)
    .await?;

    info!(?uuid, "Subscriber added to the database");

    Ok(StatusCode::OK)
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
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

    let cfg = config::Config::load()?;

    let db_url = cfg.db.as_url(true);
    let pool = sqlx::PgPool::connect(&db_url).await?;

    info!(db_url, "Connected to the database");

    let app = Router::new()
        .route("/", get(|| async { "Hello, World!" }))
        .route("/health", get(|| async { StatusCode::OK }))
        .route("/subscribe", post(subscribe).with_state(pool))
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

    let listener = TcpListener::bind(format!("0.0.0.0:{}", cfg.server.port)).await?;
    let addr = listener.local_addr()?.to_string();

    info!(addr = format!("http://{}", addr), "Starting server on");

    axum::Server::from_tcp(listener.into_std()?)?
        .serve(app.into_make_service())
        .await?;

    Ok(())
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
        error!(%err);
        Self(err)
    }
}
