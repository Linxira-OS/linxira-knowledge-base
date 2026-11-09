// SPDX-License-Identifier: MIT OR Apache-2.0
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::json;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum StoreError {
    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),
    #[error("serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
    #[error("FTS5 is not available in this SQLite build")]
    FtsUnavailable,
    #[error("record already exists: {0}")]
    Conflict(String),
    #[error("invalid data: {0}")]
    Invalid(String),
    #[error("not found: {0}")]
    NotFound(String),
    #[error("query timeout: {0}")]
    Timeout(String),
    #[error("quota exceeded: {0}")]
    QuotaExceeded(String),
}

impl StoreError {
    pub fn is_fts_unavailable(&self) -> bool {
        matches!(self, StoreError::FtsUnavailable)
    }

    pub fn is_unique_violation(&self) -> bool {
        match self {
            StoreError::Database(sqlx::Error::Database(db)) => db.is_unique_violation(),
            _ => false,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct ErrorCode(pub &'static str);

#[derive(Debug)]
pub struct AppError {
    status: StatusCode,
    code: &'static str,
    message: String,
    retry_after: Option<u64>,
}

impl AppError {
    pub fn new(status: StatusCode, code: &'static str, message: impl Into<String>) -> Self {
        let fallback = message.into();
        let message =
            crate::i18n::error_message(&crate::i18n::current_locale(), code).unwrap_or(fallback);
        Self {
            status,
            code,
            message,
            retry_after: None,
        }
    }

    pub fn with_retry_after(mut self, seconds: u64) -> Self {
        self.retry_after = Some(seconds);
        self
    }

    pub fn unauthorized(message: impl Into<String>) -> Self {
        Self::new(StatusCode::UNAUTHORIZED, "unauthorized", message)
    }

    pub fn forbidden(message: impl Into<String>) -> Self {
        Self::new(StatusCode::FORBIDDEN, "forbidden", message)
    }

    pub fn bad_request(message: impl Into<String>) -> Self {
        Self::new(StatusCode::BAD_REQUEST, "bad_request", message)
    }

    pub fn invalid_query(message: impl Into<String>) -> Self {
        Self::new(StatusCode::BAD_REQUEST, "invalid_query", message)
    }

    pub fn invalid_top_k(message: impl Into<String>) -> Self {
        Self::new(StatusCode::BAD_REQUEST, "invalid_top_k", message)
    }

    pub fn not_found(message: impl Into<String>) -> Self {
        Self::new(StatusCode::NOT_FOUND, "not_found", message)
    }

    pub fn internal(message: impl Into<String>) -> Self {
        Self::new(StatusCode::INTERNAL_SERVER_ERROR, "internal", message)
    }

    pub fn status(&self) -> StatusCode {
        self.status
    }

    pub fn code(&self) -> &'static str {
        self.code
    }

    pub fn message(&self) -> &str {
        &self.message
    }

    pub fn quota(status: StatusCode, code: &'static str, message: impl Into<String>) -> Self {
        Self::new(status, code, message)
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let retry_after = self.retry_after;
        let mut response = (
            self.status,
            Json(json!({
                "error": self.code,
                "message": self.message,
            })),
        )
            .into_response();
        response.extensions_mut().insert(ErrorCode(self.code));
        if let Some(seconds) = retry_after {
            if let Ok(value) = axum::http::HeaderValue::from_str(&seconds.to_string()) {
                response
                    .headers_mut()
                    .insert(axum::http::header::RETRY_AFTER, value);
            }
        }
        response
    }
}

impl From<anyhow::Error> for AppError {
    fn from(err: anyhow::Error) -> Self {
        tracing::error!(error = %err, "internal error");
        AppError::internal("internal error")
    }
}

impl From<StoreError> for AppError {
    fn from(err: StoreError) -> Self {
        match err {
            StoreError::FtsUnavailable => AppError::internal("keyword index unavailable"),
            StoreError::Conflict(message) => {
                AppError::new(StatusCode::CONFLICT, "conflict", message)
            }
            StoreError::Invalid(message) => AppError::bad_request(message),
            StoreError::NotFound(message) => AppError::not_found(message),
            StoreError::Timeout(message) => {
                AppError::new(StatusCode::SERVICE_UNAVAILABLE, "timeout", message)
            }
            StoreError::QuotaExceeded(message) => {
                AppError::new(StatusCode::TOO_MANY_REQUESTS, "quota_exceeded", message)
            }
            other => {
                tracing::error!(error = %other, "store error");
                AppError::internal("storage failure")
            }
        }
    }
}
