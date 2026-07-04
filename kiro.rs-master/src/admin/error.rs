//! Admin API error type definition

use std::fmt;

use axum::http::StatusCode;

use super::types::AdminErrorResponse;

/// Admin service error type
#[derive(Debug)]
pub enum AdminServiceError {
    /// credentialdoes not exist
    NotFound { id: u64 },

    /// The upstream service call failed (network,API erroretc.)
    UpstreamError(String),

    /// internal state error
    InternalError(String),

    /// The credential is invalid (validation failed).
    InvalidCredential(String),
}

impl fmt::Display for AdminServiceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AdminServiceError::NotFound { id } => {
                write!(f, "credentialdoes not exist: {}", id)
            }
            AdminServiceError::UpstreamError(msg) => write!(f, "upstream service error: {}", msg),
            AdminServiceError::InternalError(msg) => write!(f, "insideparterror: {}", msg),
            AdminServiceError::InvalidCredential(msg) => write!(f, "credential noneeffect: {}", msg),
        }
    }
}

impl std::error::Error for AdminServiceError {}

impl AdminServiceError {
    /// fetchcorrespondof HTTP status code
    pub fn status_code(&self) -> StatusCode {
        match self {
            AdminServiceError::NotFound { .. } => StatusCode::NOT_FOUND,
            AdminServiceError::UpstreamError(_) => StatusCode::BAD_GATEWAY,
            AdminServiceError::InternalError(_) => StatusCode::INTERNAL_SERVER_ERROR,
            AdminServiceError::InvalidCredential(_) => StatusCode::BAD_REQUEST,
        }
    }

    /// convert to API errorresponse
    pub fn into_response(self) -> AdminErrorResponse {
        match &self {
            AdminServiceError::NotFound { .. } => AdminErrorResponse::not_found(self.to_string()),
            AdminServiceError::UpstreamError(_) => AdminErrorResponse::api_error(self.to_string()),
            AdminServiceError::InternalError(_) => {
                AdminErrorResponse::internal_error(self.to_string())
            }
            AdminServiceError::InvalidCredential(_) => {
                AdminErrorResponse::invalid_request(self.to_string())
            }
        }
    }
}
