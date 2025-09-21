use actix_web::{http::StatusCode, HttpResponse, ResponseError};
use derive_more::Display;
use serde::Serialize;

#[derive(Debug, Display)]
pub enum ApiError {
    #[display("not found")]
    NotFound,
    #[display("bad request: {}", _0)]
    BadRequest(String),
    #[display("internal")]
    Internal,
}

#[derive(Serialize)]
struct ErrBody {
    error: String,
}

impl ResponseError for ApiError {
    fn status_code(&self) -> StatusCode {
        match self {
            Self::NotFound => StatusCode::NOT_FOUND,
            Self::BadRequest(_) => StatusCode::BAD_REQUEST,
            Self::Internal => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    fn error_response(&self) -> HttpResponse {
        HttpResponse::build(self.status_code()).json(ErrBody {
            error: self.to_string(),
        })
    }
}

#[derive(Debug, thiserror::Error)]
pub enum RepoErr {
    #[error("not found")]
    NotFound,
    #[error("precondition failed")]
    PreconditionFailed,
    #[error("duplicate client_order_id")]
    DuplicateClientOrderId,
}
