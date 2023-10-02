use argon2::{password_hash::SaltString, Argon2, PasswordHash, PasswordHasher, PasswordVerifier};
use axum::{
    extract::{rejection::TypedHeaderRejection, State, TypedHeader},
    headers::{authorization::Basic, Authorization},
    http,
    response::{IntoResponse, Response},
};
use sqlx::PgPool;
use tokio::task;
use tracing::{error, info, instrument};
use uuid::Uuid;

use crate::{AuthError, ServerError, ServerResult};

pub async fn argon2_hash(password: String) -> anyhow::Result<String> {
    Ok(task::spawn_blocking(move || {
        let salt = SaltString::generate(rand::thread_rng());
        Argon2::default()
            .hash_password(password.as_bytes(), &salt)
            .map(|hash| hash.to_string())
    })
    .await??)
}

pub async fn argon2_verify(password: String, hash: String) -> anyhow::Result<()> {
    task::spawn_blocking(move || -> anyhow::Result<()> {
        let parsed_hash = PasswordHash::new(&hash)?;
        Argon2::default().verify_password(password.as_bytes(), &parsed_hash)?;
        Ok(())
    })
    .await??;

    Ok(())
}

#[instrument(skip(pool, auth),
fields(
    username = %auth.as_ref().map(|header| {
        let TypedHeader(auth) = header;
        format!("\"{:#}\"", auth.username())
    }).unwrap_or("None".into())
))]
pub async fn singup(
    auth: Result<TypedHeader<Authorization<Basic>>, TypedHeaderRejection>,
    State(pool): State<PgPool>,
) -> ServerResult<Response> {
    match auth {
        Ok(TypedHeader(auth)) => {
            let password_hash = argon2_hash(auth.password().into()).await?;
            let uuid = Uuid::new_v4();
            sqlx::query!(
                r#"
                INSERT INTO users (id, username, password_hash)
                VALUES ($1, $2, $3)
                "#,
                uuid,
                auth.username(),
                password_hash
            )
            .execute(&pool)
            .await
            .map_err(ServerError::unexpected)?;

            info!(?uuid, "New user signed-up");

            Ok(http::StatusCode::OK.into_response())
        }
        Err(_err) => Ok(http::StatusCode::BAD_REQUEST.into_response()),
    }
}

#[instrument(skip(pool, auth),
fields(
    username = %auth.as_ref().map(|header| {
        let TypedHeader(auth) = header;
        format!("\"{}\"", auth.username())
    }).unwrap_or("None".into())
))]
pub async fn login(
    auth: Result<TypedHeader<Authorization<Basic>>, TypedHeaderRejection>,
    State(pool): State<PgPool>,
) -> ServerResult<Response> {
    match auth {
        Ok(TypedHeader(auth)) => {
            let password_hash = sqlx::query!(
                r#"
                SELECT password_hash FROM users
                WHERE username = $1
                "#,
                auth.username(),
            )
            .fetch_optional(&pool)
            .await
            .map_err(|e| AuthError::Unexpected(e.into()))?
            .ok_or_else(|| AuthError::UserNotFound)?
            .password_hash;

            argon2_verify(auth.password().into(), password_hash)
                .await
                .map_err(|_| AuthError::IncorrectPassword)?;

            info!("User logged in");

            Ok(http::StatusCode::OK.into_response())
        }
        Err(err) => {
            let err = err.into();
            error!(%err); // TODO: need to figure out how to log all the bubbled up errors
            Err(AuthError::Unexpected(err).into())
        }
    }
}
