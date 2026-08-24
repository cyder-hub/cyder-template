use std::{error::Error, net::AddrParseError, sync::Arc};

use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde::Serialize;

pub type AppResult<T> = Result<T, AppError>;
pub type HttpResult<T> = Result<T, HttpError>;

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("configuration error: {source}")]
    Config {
        #[from]
        source: crate::config::ConfigError,
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
    #[error("id generator initialization failed: {source}")]
    Id {
        #[from]
        source: crate::id::IdError,
    },
    #[error("readiness probe failed: {message}")]
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

#[derive(Debug, thiserror::Error)]
pub enum HttpError {
    #[error("database request failed: {source}")]
    Database {
        #[from]
        source: crate::database::DatabaseError,
    },
    #[error("request ID generation failed: {source}")]
    Id {
        #[from]
        source: crate::id::IdError,
    },
    // template-example:start
    #[error("{resource} {id} was not found")]
    NotFound { resource: &'static str, id: i64 },
    // template-example:end
    #[error("API route was not found")]
    ApiNotFound,
    #[error("readiness check failed: {source}")]
    Readiness {
        #[source]
        source: DiagnosticError,
    },
    // template-example:start
    #[error("system clock is before the Unix epoch: {source}")]
    Clock {
        #[source]
        source: std::time::SystemTimeError,
    },
    #[error("current timestamp exceeds the signed 64-bit range")]
    TimestampOverflow,
    // template-example:end
}

impl HttpError {
    pub fn readiness(message: impl Into<String>) -> Self {
        Self::Readiness {
            source: DiagnosticError::new(message),
        }
    }

    pub fn readiness_with_source(
        message: impl Into<String>,
        source: impl Error + Send + Sync + 'static,
    ) -> Self {
        Self::Readiness {
            source: DiagnosticError::with_source(message, source),
        }
    }

    pub(crate) fn public_error(&self) -> PublicError {
        match self {
            // template-example:start
            Self::NotFound { .. } => PublicError {
                status: StatusCode::NOT_FOUND,
                code: "not_found",
                message: self.to_string(),
            },
            // template-example:end
            Self::ApiNotFound => PublicError {
                status: StatusCode::NOT_FOUND,
                code: "not_found",
                message: "API route was not found".to_string(),
            },
            Self::Readiness { .. } => PublicError {
                status: StatusCode::SERVICE_UNAVAILABLE,
                code: "readiness_failed",
                message: "service is not ready".to_string(),
            },
            Self::Database { .. } | Self::Id { .. } => PublicError::internal(),
            // template-example:start
            Self::Clock { .. } | Self::TimestampOverflow => PublicError::internal(),
            // template-example:end
        }
    }
}

#[derive(Debug, thiserror::Error)]
#[error("{message}")]
pub struct DiagnosticError {
    message: String,
    #[source]
    source: Option<Box<dyn Error + Send + Sync>>,
}

impl DiagnosticError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            source: None,
        }
    }

    fn with_source(message: impl Into<String>, source: impl Error + Send + Sync + 'static) -> Self {
        Self {
            message: message.into(),
            source: Some(Box::new(source)),
        }
    }
}

impl IntoResponse for HttpError {
    fn into_response(self) -> Response {
        let status = self.public_error().status;
        let mut response = status.into_response();
        response
            .extensions_mut()
            .insert(HttpErrorContext(Arc::new(self)));
        response
    }
}

#[derive(Debug, Clone)]
pub(crate) struct HttpErrorContext(pub Arc<HttpError>);

#[derive(Debug, Clone)]
pub(crate) struct PublicError {
    pub status: StatusCode,
    pub code: &'static str,
    pub message: String,
}

impl PublicError {
    pub(crate) fn internal() -> Self {
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            code: "internal_error",
            message: "internal server error".to_string(),
        }
    }
}

#[derive(Debug, Serialize)]
pub struct ErrorResponse {
    pub error: String,
    pub message: String,
    pub request_id: String,
}

pub(crate) fn format_error_chain(error: &(dyn Error + 'static)) -> String {
    let mut messages = Vec::new();
    let mut current = Some(error);
    while let Some(source) = current.take() {
        messages.push(redact_diagnostic(&source.to_string()));
        if messages.len() == 16 {
            messages.push("additional causes omitted".to_string());
            break;
        }
        current = source.source();
    }
    messages.join(": ")
}

pub(crate) fn redact_diagnostic(value: &str) -> String {
    let mut redacted = value.to_string();
    for scheme in ["postgres://", "postgresql://"] {
        while let Some(start) = redacted.find(scheme) {
            let end = redacted[start..]
                .find(|character: char| {
                    character.is_whitespace()
                        || matches!(character, '\'' | '"' | ')' | ']' | '}' | ',')
                })
                .map_or(redacted.len(), |offset| start + offset);
            redacted.replace_range(start..end, "[REDACTED_DATABASE_URL]");
        }
    }
    redacted
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn internal_http_errors_have_one_public_contract() {
        let error = HttpError::Database {
            source: crate::database::DatabaseError::PoolCheckout {
                backend: "postgres",
                source: crate::database::DatabaseDiagnostic::new("connection unavailable"),
            },
        };
        let public = error.public_error();

        assert_eq!(public.status, StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(public.code, "internal_error");
        assert_eq!(public.message, "internal server error");
    }

    #[test]
    fn diagnostic_chains_redact_database_urls() {
        let secret = "unique-database-secret-marker";
        let error = AppError::Internal {
            message: format!("failed for postgres://app:{secret}@database/app"),
        };
        let chain = format_error_chain(&error);

        assert!(!chain.contains(secret));
        assert!(chain.contains("[REDACTED_DATABASE_URL]"));
    }
}
