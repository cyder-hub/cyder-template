use std::{
    any::Any,
    backtrace::Backtrace,
    cell::RefCell,
    panic::{self, PanicHookInfo},
    pin::Pin,
    sync::{Arc, Once},
    task::{Context, Poll},
    time::{Duration, Instant},
};

use axum::{
    body::{Body, Bytes, HttpBody},
    extract::{MatchedPath, Request, State},
    http::{HeaderValue, StatusCode, header},
    middleware::Next,
    response::{IntoResponse, Response},
};
use http_body::{Frame, SizeHint};
use tokio::sync::Semaphore;
use tracing::Instrument;

use crate::{
    config::AppConfig,
    error::{
        ErrorResponse, HttpError, HttpErrorContext, PublicError, format_error_chain,
        redact_diagnostic,
    },
};

const CONTENT_SECURITY_POLICY: &str = "default-src 'self'; base-uri 'self'; object-src 'none'; frame-ancestors 'none'; form-action 'self'; script-src 'self'; style-src 'self'; img-src 'self' data:; font-src 'self' data:; connect-src 'self'";
const PERMISSIONS_POLICY: &str = "camera=(), microphone=(), geolocation=(), payment=(), usb=()";
const IMMUTABLE_CACHE_CONTROL: &str = "public, max-age=31536000, immutable";
const NO_CACHE: &str = "no-cache";
const NO_STORE: &str = "no-store";

#[derive(Debug, Clone)]
pub struct HttpProtection {
    timeout: Duration,
    concurrency: Arc<Semaphore>,
    max_request_body_bytes: usize,
}

impl HttpProtection {
    pub fn from_config(config: &AppConfig) -> Self {
        Self {
            timeout: Duration::from_millis(config.http_request_timeout_ms),
            concurrency: Arc::new(Semaphore::new(config.http_max_concurrent_requests as usize)),
            max_request_body_bytes: config.http_max_request_body_bytes as usize,
        }
    }

    pub fn max_request_body_bytes(&self) -> usize {
        self.max_request_body_bytes
    }
}

#[derive(Debug, Clone, Copy)]
enum BoundaryFailure {
    RequestTimeout,
    ServiceOverloaded,
}

#[derive(Debug, Clone)]
struct PanicReport {
    message: String,
    location: String,
    backtrace: String,
}

static INSTALL_REDACTING_PANIC_HOOK: Once = Once::new();

thread_local! {
    static PANIC_REPORT: RefCell<Option<PanicReport>> = const { RefCell::new(None) };
}

impl PanicReport {
    fn capture(payload: &(dyn Any + Send), location: Option<&panic::Location<'_>>) -> Self {
        Self {
            message: redact_diagnostic(panic_message(payload)),
            location: location
                .map(|location| {
                    format!(
                        "{}:{}:{}",
                        location.file(),
                        location.line(),
                        location.column()
                    )
                })
                .unwrap_or_else(|| "unknown location".to_string()),
            backtrace: Backtrace::force_capture().to_string(),
        }
    }
}

pub fn install_redacting_panic_hook() {
    INSTALL_REDACTING_PANIC_HOOK.call_once(|| {
        panic::set_hook(Box::new(|info: &PanicHookInfo<'_>| {
            let report = PanicReport::capture(info.payload(), info.location());
            let diagnostic = format_panic_diagnostic(&report);
            PANIC_REPORT.with(|slot| *slot.borrow_mut() = Some(report));

            // Do not call the previous/default hook: it receives the original payload and
            // would write secrets before the HTTP panic boundary can redact them.
            write_panic_diagnostic(&diagnostic);
        }));
    });
}

fn write_panic_diagnostic(diagnostic: &str) {
    #[cfg(test)]
    eprintln!("{diagnostic}");

    #[cfg(not(test))]
    {
        use std::io::Write;

        let _ = writeln!(std::io::stderr().lock(), "{diagnostic}");
    }
}

fn panic_message(payload: &(dyn Any + Send)) -> &str {
    if let Some(message) = payload.downcast_ref::<String>() {
        message
    } else if let Some(message) = payload.downcast_ref::<&str>() {
        message
    } else {
        "non-string panic payload"
    }
}

fn format_panic_diagnostic(report: &PanicReport) -> String {
    format!(
        "panic at {}:\n{}\nstack backtrace:\n{}",
        report.location, report.message, report.backtrace
    )
}

pub async fn protect_request(
    State(protection): State<HttpProtection>,
    request: Request,
    next: Next,
) -> Response {
    let Ok(_permit) = protection.concurrency.clone().try_acquire_owned() else {
        return boundary_failure_response(
            StatusCode::SERVICE_UNAVAILABLE,
            BoundaryFailure::ServiceOverloaded,
        );
    };

    match tokio::time::timeout(protection.timeout, next.run(request)).await {
        Ok(response) => response,
        Err(_) => {
            boundary_failure_response(StatusCode::REQUEST_TIMEOUT, BoundaryFailure::RequestTimeout)
        }
    }
}

fn boundary_failure_response(status: StatusCode, failure: BoundaryFailure) -> Response {
    let mut response = status.into_response();
    response.extensions_mut().insert(failure);
    response
}

pub fn panic_response(payload: Box<dyn Any + Send + 'static>) -> Response {
    let message = redact_diagnostic(panic_message(payload.as_ref()));
    let report = PANIC_REPORT
        .with(|slot| slot.borrow_mut().take())
        .filter(|report| report.message == message)
        .unwrap_or_else(|| PanicReport::capture(payload.as_ref(), None));
    let mut response = StatusCode::INTERNAL_SERVER_ERROR.into_response();
    response.extensions_mut().insert(report);
    response
}

pub async fn observe_request(mut request: Request, next: Next) -> Response {
    let request_id = resolve_request_id(request.headers().get("x-request-id"));
    request.headers_mut().insert(
        "x-request-id",
        HeaderValue::from_str(&request_id).expect("generated request ID must be a valid header"),
    );

    let method = request.method().clone();
    let path = request.uri().path().to_string();
    let route = request
        .extensions()
        .get::<MatchedPath>()
        .map(|matched| matched.as_str().to_string())
        .unwrap_or_else(|| path.clone());
    let started = Instant::now();
    let span = tracing::info_span!(
        "http_request",
        request_id = %request_id,
        method = %method,
        route = %route,
    );
    let mut response = next.run(request).instrument(span).await;

    let http_error = response
        .extensions_mut()
        .remove::<HttpErrorContext>()
        .map(|context| context.0);
    let boundary_failure = response.extensions_mut().remove::<BoundaryFailure>();
    let panic_report = response.extensions_mut().remove::<PanicReport>();

    let public_error = http_error
        .as_ref()
        .map(|error| error.public_error())
        .or_else(|| boundary_failure.map(public_error_for_boundary))
        .or_else(|| panic_report.as_ref().map(|_| PublicError::internal()))
        .or_else(|| public_error_for_response(&path, response.status()));

    if let Some(error) = http_error.as_ref() {
        log_http_error(&request_id, error);
    }
    if let Some(failure) = boundary_failure {
        log_boundary_failure(&request_id, failure);
    }
    if let Some(report) = panic_report.as_ref() {
        tracing::error!(
            event = "http_request_panicked",
            request_id = %request_id,
            panic = %report.message,
            panic_location = %report.location,
            backtrace = %report.backtrace,
            "HTTP request panicked"
        );
    } else if http_error.is_none()
        && boundary_failure.is_none()
        && response.status().is_server_error()
    {
        tracing::error!(
            event = "http_request_failed",
            request_id = %request_id,
            status = response.status().as_u16(),
            error_code = "internal_error",
            "HTTP request failed without a typed error source"
        );
    }

    if let Some(public_error) = public_error {
        replace_with_error_json(&mut response, public_error, &request_id);
    }
    apply_cache_control(&mut response, &path);
    apply_security_headers(&mut response);
    response.headers_mut().insert(
        "x-request-id",
        HeaderValue::from_str(&request_id).expect("request ID must remain a valid header"),
    );

    let status = response.status();
    let dispatcher = tracing::dispatcher::get_default(Clone::clone);
    let (parts, body) = response.into_parts();
    Response::from_parts(
        parts,
        Body::new(ObservedBody::new(
            body,
            AccessLog {
                started,
                request_id,
                method: method.to_string(),
                route,
                status,
                response_bytes: 0,
                logged: false,
                dispatcher,
            },
        )),
    )
}

struct ObservedBody {
    inner: Pin<Box<Body>>,
    access: AccessLog,
}

impl ObservedBody {
    fn new(inner: Body, access: AccessLog) -> Self {
        Self {
            inner: Box::pin(inner),
            access,
        }
    }
}

impl HttpBody for ObservedBody {
    type Data = Bytes;
    type Error = axum::Error;

    fn poll_frame(
        mut self: Pin<&mut Self>,
        context: &mut Context<'_>,
    ) -> Poll<Option<Result<Frame<Self::Data>, Self::Error>>> {
        match self.inner.as_mut().poll_frame(context) {
            Poll::Ready(Some(Ok(frame))) => {
                if let Some(data) = frame.data_ref() {
                    self.access.response_bytes =
                        self.access.response_bytes.saturating_add(data.len() as u64);
                }
                if self.inner.is_end_stream() {
                    self.access.finish();
                }
                Poll::Ready(Some(Ok(frame)))
            }
            Poll::Ready(None) => {
                self.access.finish();
                Poll::Ready(None)
            }
            other => other,
        }
    }

    fn is_end_stream(&self) -> bool {
        self.inner.is_end_stream()
    }

    fn size_hint(&self) -> SizeHint {
        self.inner.size_hint()
    }
}

impl Drop for ObservedBody {
    fn drop(&mut self) {
        self.access.finish();
    }
}

struct AccessLog {
    started: Instant,
    request_id: String,
    method: String,
    route: String,
    status: StatusCode,
    response_bytes: u64,
    logged: bool,
    dispatcher: tracing::Dispatch,
}

impl AccessLog {
    fn finish(&mut self) {
        if self.logged {
            return;
        }
        self.logged = true;
        tracing::dispatcher::with_default(&self.dispatcher, || {
            tracing::info!(
                event = "http_request_completed",
                request_id = %self.request_id,
                method = %self.method,
                route = %self.route,
                status = self.status.as_u16(),
                latency_ms = self.started.elapsed().as_secs_f64() * 1_000.0,
                response_bytes = self.response_bytes,
                "HTTP request completed"
            );
        });
    }
}

fn resolve_request_id(value: Option<&HeaderValue>) -> String {
    value
        .filter(|value| valid_request_id(value.as_bytes()))
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned)
        .unwrap_or_else(|| uuid::Uuid::new_v4().hyphenated().to_string())
}

fn valid_request_id(value: &[u8]) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .iter()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
}

fn public_error_for_boundary(failure: BoundaryFailure) -> PublicError {
    match failure {
        BoundaryFailure::RequestTimeout => PublicError {
            status: StatusCode::REQUEST_TIMEOUT,
            code: "request_timeout",
            message: "request timed out".to_string(),
        },
        BoundaryFailure::ServiceOverloaded => PublicError {
            status: StatusCode::SERVICE_UNAVAILABLE,
            code: "service_overloaded",
            message: "service is overloaded".to_string(),
        },
    }
}

fn public_error_for_response(path: &str, status: StatusCode) -> Option<PublicError> {
    if status.is_server_error() {
        return Some(PublicError::internal());
    }
    if !(is_api_path(path) || path == "/readyz") || !status.is_client_error() {
        return None;
    }

    let (code, message) = match status {
        StatusCode::BAD_REQUEST | StatusCode::UNPROCESSABLE_ENTITY => {
            ("invalid_request", "request is invalid")
        }
        StatusCode::NOT_FOUND => ("not_found", "API route was not found"),
        StatusCode::METHOD_NOT_ALLOWED => ("method_not_allowed", "HTTP method is not allowed"),
        StatusCode::PAYLOAD_TOO_LARGE => ("payload_too_large", "request body is too large"),
        StatusCode::UNSUPPORTED_MEDIA_TYPE => (
            "unsupported_media_type",
            "content type must be application/json",
        ),
        _ => ("request_failed", "request failed"),
    };
    Some(PublicError {
        status,
        code,
        message: message.to_string(),
    })
}

fn replace_with_error_json(response: &mut Response, public_error: PublicError, request_id: &str) {
    let body = ErrorResponse {
        error: public_error.code.to_string(),
        message: public_error.message,
        request_id: request_id.to_string(),
    };
    let bytes = serde_json::to_vec(&body).expect("error response strings must serialize");
    *response.status_mut() = public_error.status;
    *response.body_mut() = Body::from(bytes.clone());
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/json"),
    );
    response.headers_mut().insert(
        header::CONTENT_LENGTH,
        HeaderValue::from_str(&bytes.len().to_string())
            .expect("serialized error length must be a valid header"),
    );
    for name in [
        header::CONTENT_ENCODING,
        header::CONTENT_RANGE,
        header::ETAG,
        header::LAST_MODIFIED,
    ] {
        response.headers_mut().remove(name);
    }
}

fn log_http_error(request_id: &str, error: &HttpError) {
    let public = error.public_error();
    match error {
        HttpError::Readiness { .. } => tracing::warn!(
            event = "http_request_failed",
            request_id = %request_id,
            status = public.status.as_u16(),
            error_code = public.code,
            error = %format_error_chain(error),
            "HTTP readiness request failed"
        ),
        HttpError::ApiNotFound => {}
        _ => tracing::error!(
            event = "http_request_failed",
            request_id = %request_id,
            status = public.status.as_u16(),
            error_code = public.code,
            error = %format_error_chain(error),
            "HTTP request failed"
        ),
    }
}

fn log_boundary_failure(request_id: &str, failure: BoundaryFailure) {
    let public = public_error_for_boundary(failure);
    tracing::warn!(
        event = "http_request_failed",
        request_id = %request_id,
        status = public.status.as_u16(),
        error_code = public.code,
        "HTTP request was rejected by a protection boundary"
    );
}

fn apply_cache_control(response: &mut Response, path: &str) {
    let cache_control =
        if response.status().is_client_error() || response.status().is_server_error() {
            Some(NO_STORE)
        } else if is_api_path(path) || matches!(path, "/healthz" | "/readyz") {
            response
                .headers()
                .get(header::CACHE_CONTROL)
                .is_none()
                .then_some(NO_STORE)
        } else if path.starts_with("/assets/")
            && (response.status().is_success() || response.status() == StatusCode::NOT_MODIFIED)
        {
            Some(IMMUTABLE_CACHE_CONTROL)
        } else if response.status().is_success() || response.status() == StatusCode::NOT_MODIFIED {
            Some(NO_CACHE)
        } else {
            None
        };

    if let Some(value) = cache_control {
        response
            .headers_mut()
            .insert(header::CACHE_CONTROL, HeaderValue::from_static(value));
    }
}

fn apply_security_headers(response: &mut Response) {
    for (name, value) in [
        ("content-security-policy", CONTENT_SECURITY_POLICY),
        ("x-content-type-options", "nosniff"),
        ("x-frame-options", "DENY"),
        ("referrer-policy", "strict-origin-when-cross-origin"),
        ("permissions-policy", PERMISSIONS_POLICY),
        ("cross-origin-opener-policy", "same-origin"),
        ("cross-origin-resource-policy", "same-origin"),
    ] {
        response
            .headers_mut()
            .insert(name, HeaderValue::from_static(value));
    }
}

fn is_api_path(path: &str) -> bool {
    path == "/api" || path.starts_with("/api/")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn panic_hook_diagnostics_redact_payloads_before_writing() {
        let payload = "handler failed for postgres://app:panic-secret@database/app";
        let report = PanicReport::capture(&payload, None);
        let diagnostic = format_panic_diagnostic(&report);

        assert!(diagnostic.contains("handler failed for [REDACTED_DATABASE_URL]"));
        assert!(!diagnostic.contains("panic-secret"));
    }

    #[test]
    fn incoming_request_ids_use_a_bounded_safe_alphabet() {
        assert!(valid_request_id(b"proxy-ABC_123.example"));
        assert!(valid_request_id(&[b'a'; 128]));
        assert!(!valid_request_id(b""));
        assert!(!valid_request_id(b"contains space"));
        assert!(!valid_request_id(&[b'a'; 129]));
    }

    #[test]
    fn invalid_request_ids_are_replaced_with_uuid_v4_values() {
        let invalid = HeaderValue::from_static("not allowed");
        let request_id = resolve_request_id(Some(&invalid));
        let parsed = uuid::Uuid::parse_str(&request_id).expect("generated ID should be a UUID");

        assert_eq!(parsed.get_version_num(), 4);
    }
}
