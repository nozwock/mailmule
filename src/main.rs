mod config;

use anyhow::Result;
use axum::{
    extract::Form,
    http::StatusCode,
    routing::{get, post},
    Router,
};
use serde::Deserialize;
use std::net::TcpListener;

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct SubscriptionInfo {
    name: String,
    email: String,
}

/// Content-Type: application/x-www-form-urlencoded
async fn subscribe(Form(info): Form<SubscriptionInfo>) {
    eprintln!("{:?}", info);
}

#[tokio::main]
async fn main() -> Result<()> {
    let cfg = config::Config::load()?;
    let app = Router::new()
        .route("/", get(|| async { "Hello, World!" }))
        .route("/health", get(|| async { StatusCode::OK }))
        .route("/subscribe", post(subscribe));

    let listener = TcpListener::bind(format!("0.0.0.0:{}", cfg.server.port))?;
    let addr = listener.local_addr()?.to_string();
    eprintln!("Running on:");
    println!("http://{}", addr);

    axum::Server::from_tcp(listener)?
        .serve(app.into_make_service())
        .await?;

    Ok(())
}
