use axum::http::{header, StatusCode};

pub mod auth;
pub mod config;
pub mod email;
pub mod helpers;
pub mod publish;
pub mod subscribe;

type ServerResult<T, E = ServerError> = core::result::Result<T, E>;

#[derive(Debug, thiserror::Error)]
pub enum ServerError {
    #[error(transparent)]
    Auth(#[from] AuthError),
    #[error(transparent)]
    Unexpected(#[from] anyhow::Error),
}

impl ServerError {
    pub fn unexpected(err: impl Into<anyhow::Error>) -> Self {
        Self::Unexpected(err.into())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum AuthError {
    #[error("No such user is found")]
    UserNotFound,
    #[error("Password does not match")]
    IncorrectPassword,
    #[error(transparent)]
    Unexpected(#[from] anyhow::Error),
}

impl axum::response::IntoResponse for ServerError {
    fn into_response(self) -> axum::response::Response {
        match self {
            ServerError::Auth(e) => (
                StatusCode::UNAUTHORIZED,
                [(header::WWW_AUTHENTICATE, "Basic")],
                e.to_string(),
            )
                .into_response(),
            ServerError::Unexpected(e) => {
                (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response()
            }
        }
    }
}
