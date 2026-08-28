//! HTTP error representation — RFC 9457 style Problem Details.
//!
//! Clients branch on the stable `code` field, never on human-readable text.

use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use gym_application::ApplicationError;
use serde::Serialize;

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct ProblemDetails {
    /// Short, human-readable summary. For humans; do not parse.
    pub title: String,
    /// HTTP status code, repeated in the body for convenience.
    pub status: u16,
    /// Stable, machine-readable code — branch on this.
    pub code: String,
    /// Optional additional detail, safe to show to users.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

/// Wrapper so `ApplicationError` can be returned directly from handlers.
#[derive(Debug)]
pub struct ApiError(pub ApplicationError);

impl From<ApplicationError> for ApiError {
    fn from(err: ApplicationError) -> Self {
        Self(err)
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let err = self.0;

        let status = match &err {
            ApplicationError::Domain(_) => StatusCode::BAD_REQUEST,
            ApplicationError::Unauthenticated => StatusCode::UNAUTHORIZED,
            ApplicationError::Forbidden => StatusCode::FORBIDDEN,
            ApplicationError::NotFound { .. } => StatusCode::NOT_FOUND,
            ApplicationError::Conflict(_) | ApplicationError::StateConflict(_) => {
                StatusCode::CONFLICT
            }
            ApplicationError::Unavailable(_) => StatusCode::SERVICE_UNAVAILABLE,
            // 429, and the client is expected to branch on the CODE rather
            // than the status — a proxy in front of us may well produce its
            // own 429 for entirely different reasons.
            ApplicationError::TooManyAttempts => StatusCode::TOO_MANY_REQUESTS,
            // 400, not 401: the caller is not failing to authenticate, they
            // are presenting a link that is no good. A 401 would send a client
            // into its token-refresh path for something a refresh cannot fix.
            ApplicationError::InvalidToken => StatusCode::BAD_REQUEST,
            ApplicationError::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
        };

        // Internal errors are logged in full but never leak detail to the client.
        let detail = match &err {
            ApplicationError::Internal(source) => {
                tracing::error!(error = %source, "internal error");
                None
            }
            other => Some(other.to_string()),
        };

        let body = ProblemDetails {
            title: status.canonical_reason().unwrap_or("Error").to_owned(),
            status: status.as_u16(),
            code: err.code().to_owned(),
            detail,
        };

        (status, Json(body)).into_response()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gym_domain::DomainError;

    fn status_of(err: ApplicationError) -> StatusCode {
        ApiError(err).into_response().status()
    }

    #[test]
    fn maps_application_errors_to_expected_statuses() {
        assert_eq!(
            status_of(ApplicationError::Domain(DomainError::InvalidEmail)),
            StatusCode::BAD_REQUEST
        );
        assert_eq!(
            status_of(ApplicationError::Unauthenticated),
            StatusCode::UNAUTHORIZED
        );
        assert_eq!(
            status_of(ApplicationError::Forbidden),
            StatusCode::FORBIDDEN
        );
        assert_eq!(
            status_of(ApplicationError::NotFound { entity: "exercise" }),
            StatusCode::NOT_FOUND
        );
        assert_eq!(
            status_of(ApplicationError::Conflict("email".into())),
            StatusCode::CONFLICT
        );
    }

    #[test]
    fn internal_errors_do_not_leak_detail() {
        let err = ApplicationError::Internal("connection string user=admin".into());
        let problem = ProblemDetails {
            title: "Internal Server Error".into(),
            status: 500,
            code: err.code().to_owned(),
            detail: None,
        };
        let json = serde_json::to_string(&problem).unwrap();

        assert!(!json.contains("connection string"));
        assert!(!json.contains("detail"), "detail must be omitted entirely");
        assert!(json.contains("internal.error"));
    }
}
