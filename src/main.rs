use anyhow::Result;
use axum::{
    body::Bytes,
    extract::MatchedPath,
    http::{HeaderMap, Request, StatusCode},
    response::Response,
    routing::{get, post},
    Router,
};
use mailmule::{config::Config, email::EmailClient, helpers::SocketAddr, publish::PublishState};
use mailmule::{
    publish::publish,
    subscribe::{subscribe, subscribe_confirm, SubscribeState},
};
use std::{sync::Arc, time::Duration};
use tokio::net::TcpListener;
use tower_http::{classify::ServerErrorsFailureClass, trace::TraceLayer};
use tracing::{info, info_span, Span};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

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
        .route(
            "/publish",
            post(publish).with_state(PublishState {
                pool: pool.clone(),
                email_client: email_client.clone(),
            }),
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
