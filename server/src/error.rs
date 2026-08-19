use std::net::AddrParseError;

use axum::{Json, http::StatusCode, response::IntoResponse};
use serde::Serialize;

pub type AppResult<T> = Result<T, AppError>;

#[derive(Debug, Serialize)]
pub struct ErrorResponse {
    pub error: String,
    pub message: String,
}

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("configuration error: {source}")]
    Config {
        #[from]
        source: config::ConfigError,
    },
    #[error("invalid configuration: {source}")]
    ConfigValidation {
        #[from]
        source: crate::config::ConfigValidationError,
    },
    #[error("invalid bind address: {source}")]
    BindAddress {
        #[from]
        source: AddrParseError,
    },
    #[error("server error: {source}")]
    Server {
        #[from]
        source: std::io::Error,
    },
    #[error("database initialization failed: {source}")]
    DatabaseInit {
        #[from]
        source: crate::database::DatabaseInitError,
    },
    #[error("database error: {source}")]
    Database {
        #[from]
        source: crate::database::DatabaseError,
    },
    #[error("id generation failed: {source}")]
    Id {
        #[from]
        source: crate::id::IdError,
    },
    // template-example:start
    #[error("{resource} {id} was not found")]
    NotFound { resource: &'static str, id: i64 },
    // template-example:end
    #[error("readiness check failed: {message}")]
    Readiness { message: String },
    #[error("failed to install shutdown signal handler: {source}")]
    ShutdownSignal { source: std::io::Error },
    #[error("graceful shutdown timed out after {timeout_ms} ms")]
    ShutdownTimeout { timeout_ms: u64 },
    #[error("graceful shutdown was forced by a second {signal} signal")]
    ShutdownForced { signal: &'static str },
    #[error("{message}")]
    Internal { message: String },
}

impl IntoResponse for AppError {
    fn into_response(self) -> axum::response::Response {
        let status = self.status_code();
        let body = ErrorResponse {
            error: self.error_code().to_string(),
            message: self.to_string(),
        };
        (status, Json(body)).into_response()
    }
}

impl AppError {
    fn status_code(&self) -> StatusCode {
        match self {
            AppError::Readiness { .. } => StatusCode::SERVICE_UNAVAILABLE,
            // template-example:start
            AppError::NotFound { .. } => StatusCode::NOT_FOUND,
            // template-example:end
            AppError::Config { .. }
            | AppError::ConfigValidation { .. }
            | AppError::BindAddress { .. } => StatusCode::BAD_REQUEST,
            AppError::Server { .. }
            | AppError::DatabaseInit { .. }
            | AppError::Database { .. }
            | AppError::Id { .. }
            | AppError::ShutdownSignal { .. }
            | AppError::ShutdownTimeout { .. }
            | AppError::ShutdownForced { .. }
            | AppError::Internal { .. } => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    fn error_code(&self) -> &'static str {
        match self {
            AppError::Config { .. } => "config_error",
            AppError::ConfigValidation { .. } => "config_validation_error",
            AppError::BindAddress { .. } => "bind_address_error",
            AppError::Server { .. } => "server_error",
            AppError::DatabaseInit { .. } => "database_init_error",
            AppError::Database { .. } => "database_error",
            AppError::Id { .. } => "id_error",
            // template-example:start
            AppError::NotFound { .. } => "not_found",
            // template-example:end
            AppError::Readiness { .. } => "readiness_failed",
            AppError::ShutdownSignal { .. } => "shutdown_signal_error",
            AppError::ShutdownTimeout { .. } => "shutdown_timeout",
            AppError::ShutdownForced { .. } => "shutdown_forced",
            AppError::Internal { .. } => "internal_error",
        }
    }
}
