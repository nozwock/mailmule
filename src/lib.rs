pub mod config;
pub mod email;
pub mod helpers;
pub mod publish;
pub mod subscribe;

type ServerResult<T, E = ServerError> = core::result::Result<T, E>;

#[derive(Debug)]
pub struct ServerError(pub anyhow::Error);

impl axum::response::IntoResponse for ServerError {
    fn into_response(self) -> axum::response::Response {
        (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            self.0.to_string(),
        )
            .into_response()
    }
}

impl<E> From<E> for ServerError
where
    E: Into<anyhow::Error>,
{
    fn from(value: E) -> Self {
        let err = value.into();
        tracing::error!(?err);
        Self(err)
    }
}
