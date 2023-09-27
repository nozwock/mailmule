mod config;

use axum::{
    extract::{Form, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post},
    Router,
};
use chrono::Utc;
use serde::Deserialize;
use sqlx::PgPool;
use std::net::TcpListener;
use uuid::Uuid;

type ServerResult<T, E = ServerError> = core::result::Result<T, E>;

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct SubscriptionInfo {
    name: String,
    email: String,
}

/// Content-Type: application/x-www-form-urlencoded
async fn subscribe(
    State(poll): State<PgPool>,
    Form(form): Form<SubscriptionInfo>,
) -> ServerResult<StatusCode> {
    sqlx::query!(
        r#"
        INSERT INTO subscribers (id, email, name, subscribed_at)
        VALUES ($1, $2, $3, $4)
        "#,
        Uuid::new_v4(),
        form.email,
        form.name,
        Utc::now()
    )
    .execute(&poll)
    .await?;

    Ok(StatusCode::OK)
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cfg = config::Config::load()?;

    let pool = sqlx::PgPool::connect(&cfg.db.as_url(true)).await?;

    let app = Router::new()
        .route("/", get(|| async { "Hello, World!" }))
        .route("/health", get(|| async { StatusCode::OK }))
        .route("/subscribe", post(subscribe).with_state(pool));

    let listener = TcpListener::bind(format!("0.0.0.0:{}", cfg.server.port))?;
    let addr = listener.local_addr()?.to_string();
    eprintln!("Running on:");
    println!("http://{}", addr);

    axum::Server::from_tcp(listener)?
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
        Self(value.into())
    }
}
