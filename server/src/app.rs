use std::{convert::Infallible, path::PathBuf, sync::Arc};

use axum::{
    Router,
    body::Body,
    extract::DefaultBodyLimit,
    http::{HeaderMap, Method, Request, StatusCode, header},
    middleware,
    response::{IntoResponse, Response},
    routing::{any, get},
};
use tower::{ServiceExt, service_fn};
use tower_http::{
    catch_panic::CatchPanicLayer,
    limit::RequestBodyLimitLayer,
    services::{ServeDir, ServeFile},
};

use crate::{
    config::AppConfig,
    controller, database,
    error::{AppResult, HttpError},
    http_middleware::{self, HttpProtection},
    id::IdGenerator,
    shutdown::Lifecycle,
};

pub const APP_NAME: &str = "cyder-template";
pub const FRONTEND_DIR: &str = "front/dist";
const SINGLE_INSTANCE_WORKER_ID: u64 = 1;

#[derive(Clone)]
pub struct AppState {
    database: database::DbPool,
    lifecycle: Lifecycle,
    #[allow(dead_code)]
    id_generator: Arc<IdGenerator>,
    http_protection: HttpProtection,
}

impl AppState {
    pub async fn new(config: AppConfig) -> AppResult<Self> {
        let database_options = database::DbPoolOptions::new(
            config.database_pool_size,
            config.database_acquire_timeout_ms,
            config.sqlite_busy_timeout_ms,
        );
        let database =
            database::DbPool::connect(config.database_url.as_str(), database_options).await?;
        let id_generator = IdGenerator::for_worker(SINGLE_INSTANCE_WORKER_ID)?;
        let http_protection = HttpProtection::from_config(&config);

        Ok(Self {
            database,
            lifecycle: Lifecycle::new(),
            id_generator: Arc::new(id_generator),
            http_protection,
        })
    }

    pub fn database(&self) -> &database::DbPool {
        &self.database
    }

    pub fn lifecycle(&self) -> &Lifecycle {
        &self.lifecycle
    }

    #[allow(dead_code)]
    pub fn id_generator(&self) -> &IdGenerator {
        &self.id_generator
    }

    pub fn http_protection(&self) -> &HttpProtection {
        &self.http_protection
    }
}

pub fn build_app(state: AppState) -> Router {
    build_app_with_frontend_dir(state, PathBuf::from(FRONTEND_DIR))
}

fn build_app_with_frontend_dir(state: AppState, frontend_dir: PathBuf) -> Router {
    http_middleware::install_redacting_panic_hook();

    let index_file = frontend_dir.join("index.html");
    let static_files = ServeDir::new(frontend_dir)
        .precompressed_br()
        .precompressed_gzip()
        .fallback(service_fn(move |request: Request<Body>| {
            let index_file = index_file.clone();
            async move { Ok::<_, Infallible>(spa_fallback(request, index_file).await) }
        }));

    let protection = state.http_protection().clone();
    let max_request_body_bytes = protection.max_request_body_bytes();
    let protected_routes = Router::new()
        .route("/readyz", get(controller::health::readyz))
        // template-example:start
        .route(
            "/api/items",
            get(controller::items::list_items).post(controller::items::create_item),
        )
        .route(
            "/api/items/{id}",
            get(controller::items::get_item).delete(controller::items::delete_item),
        )
        .route(
            "/api/users",
            get(controller::users::list_users).post(controller::users::create_user),
        )
        .route(
            "/api/users/{id}",
            get(controller::users::get_user).delete(controller::users::delete_user),
        )
        // template-example:end
        .route("/api", any(api_not_found))
        .route("/api/", any(api_not_found))
        .route("/api/{*path}", any(api_not_found));
    #[cfg(test)]
    let protected_routes = protected_routes
        .route("/api/__test/slow", get(test_slow_handler))
        .route("/api/__test/hold", get(test_hold_handler))
        .route("/api/__test/panic", get(test_panic_handler))
        .route("/api/__test/internal", get(test_internal_error_handler))
        .route("/api/__test/json", axum::routing::post(test_json_handler))
        .route("/api/__test/echo", axum::routing::post(test_echo_handler));
    let protected_routes = protected_routes
        .layer(DefaultBodyLimit::disable())
        .layer(RequestBodyLimitLayer::new(max_request_body_bytes))
        .layer(middleware::from_fn_with_state(
            protection,
            http_middleware::protect_request,
        ));

    Router::new()
        .route("/healthz", get(controller::health::healthz))
        .merge(protected_routes)
        .fallback_service(static_files)
        .with_state(state)
        .layer(CatchPanicLayer::custom(http_middleware::panic_response))
        .layer(middleware::from_fn(http_middleware::observe_request))
}

async fn api_not_found() -> HttpError {
    HttpError::ApiNotFound
}

#[cfg(test)]
static TEST_SLOW_HANDLER_STARTED: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

#[cfg(test)]
async fn test_slow_handler() -> &'static str {
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    "completed"
}

#[cfg(test)]
async fn test_hold_handler() -> &'static str {
    TEST_SLOW_HANDLER_STARTED.store(true, std::sync::atomic::Ordering::SeqCst);
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    "completed"
}

#[cfg(test)]
async fn test_panic_handler() -> &'static str {
    panic!("test-only HTTP panic marker postgres://app:panic-secret@database/app")
}

#[cfg(test)]
async fn test_internal_error_handler() -> HttpError {
    HttpError::Database {
        source: crate::database::DatabaseError::PoolCheckout {
            backend: "postgres",
            source: crate::database::DatabaseDiagnostic::new(
                "postgres://app:test-only-secret@database/app",
            ),
        },
    }
}

#[cfg(test)]
async fn test_echo_handler(body: axum::body::Bytes) -> axum::Json<serde_json::Value> {
    axum::Json(serde_json::json!({ "bytes": body.len() }))
}

#[cfg(test)]
#[derive(serde::Deserialize)]
struct TestJsonBody {
    title: String,
}

#[cfg(test)]
async fn test_json_handler(
    axum::Json(body): axum::Json<TestJsonBody>,
) -> axum::Json<serde_json::Value> {
    axum::Json(serde_json::json!({ "title": body.title }))
}

async fn spa_fallback(request: Request<Body>, index_file: PathBuf) -> Response {
    if !should_serve_spa_index(&request) {
        return StatusCode::NOT_FOUND.into_response();
    }

    match ServeFile::new(index_file)
        .precompressed_br()
        .precompressed_gzip()
        .oneshot(request)
        .await
    {
        Ok(response) => response.map(Body::new),
        Err(error) => match error {},
    }
}

fn should_serve_spa_index(request: &Request<Body>) -> bool {
    if !matches!(request.method(), &Method::GET | &Method::HEAD) {
        return false;
    }

    let path = request.uri().path();
    if path == "/api" || path.starts_with("/api/") || path.starts_with("/assets/") {
        return false;
    }

    let last_segment = path.rsplit('/').next().unwrap_or_default();
    !last_segment.contains('.') && accepts_html(request.headers())
}

fn accepts_html(headers: &HeaderMap) -> bool {
    let Some(accept) = headers.get(header::ACCEPT) else {
        return true;
    };

    let Ok(accept) = accept.to_str() else {
        return false;
    };

    accept.split(',').any(|part| {
        let mime = part.split(';').next().unwrap_or_default().trim();
        matches!(mime, "text/html" | "application/xhtml+xml" | "*/*")
    })
}

// template-example:start
#[cfg(test)]
#[path = "app_example_tests.rs"]
mod example_tests;
// template-example:end

#[cfg(test)]
mod tests {
    use std::{
        io::Write,
        sync::{Arc as TestArc, Mutex},
    };

    use axum::{
        body::{Body, to_bytes},
        http::{Method, Request, StatusCode, header},
    };
    use serde_json::{Value, json};
    use tower::ServiceExt;
    use tracing::instrument::WithSubscriber;
    use tracing_subscriber::fmt::MakeWriter;

    use super::*;

    static TRACING_TEST_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

    #[derive(Clone, Default)]
    struct CapturedLogs(TestArc<Mutex<Vec<u8>>>);

    impl CapturedLogs {
        fn text(&self) -> String {
            String::from_utf8(
                self.0
                    .lock()
                    .expect("captured log lock should not be poisoned")
                    .clone(),
            )
            .expect("captured logs should be UTF-8")
        }
    }

    impl Write for CapturedLogs {
        fn write(&mut self, buffer: &[u8]) -> std::io::Result<usize> {
            self.0
                .lock()
                .expect("captured log lock should not be poisoned")
                .extend_from_slice(buffer);
            Ok(buffer.len())
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    impl<'writer> MakeWriter<'writer> for CapturedLogs {
        type Writer = Self;

        fn make_writer(&'writer self) -> Self::Writer {
            self.clone()
        }
    }

    async fn test_state() -> AppState {
        AppState::new(AppConfig {
            database_url: crate::config::DatabaseUrl::sqlite_memory(),
            ..AppConfig::default()
        })
        .await
        .expect("test app state should initialize")
    }

    async fn request_json(
        app: Router,
        method: Method,
        uri: &str,
        body: Option<Value>,
    ) -> (StatusCode, Value) {
        let mut builder = Request::builder()
            .method(method)
            .uri(uri)
            .header("x-request-id", "test-request-id");
        let body = if let Some(body) = body {
            builder = builder.header(header::CONTENT_TYPE, "application/json");
            Body::from(body.to_string())
        } else {
            Body::empty()
        };

        let response = app
            .oneshot(builder.body(body).expect("request should build"))
            .await
            .expect("request should succeed");
        let status = response.status();
        let bytes = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("response body should read");
        let body = if bytes.is_empty() {
            Value::Null
        } else {
            serde_json::from_slice(&bytes).unwrap_or_else(|source| {
                panic!(
                    "response body should be json: {source}; body={}",
                    String::from_utf8_lossy(&bytes)
                )
            })
        };

        (status, body)
    }

    async fn request_text(app: Router, uri: &str) -> (StatusCode, String) {
        let response = app
            .oneshot(
                Request::builder()
                    .uri(uri)
                    .header("x-request-id", "test-request-id")
                    .body(Body::empty())
                    .expect("request should build"),
            )
            .await
            .expect("request should succeed");
        let status = response.status();
        let bytes = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("response body should read");

        (status, String::from_utf8_lossy(&bytes).into_owned())
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn healthz_returns_ok() {
        let response = build_app(test_state().await)
            .oneshot(
                Request::builder()
                    .uri("/healthz")
                    .body(Body::empty())
                    .expect("request should build"),
            )
            .await
            .expect("health request should succeed");

        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn readyz_checks_database() {
        let response = build_app(test_state().await)
            .oneshot(
                Request::builder()
                    .uri("/readyz")
                    .body(Body::empty())
                    .expect("request should build"),
            )
            .await
            .expect("ready request should succeed");

        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn shutdown_disables_readiness_but_keeps_liveness() {
        let state = test_state().await;
        state.lifecycle().begin_shutdown();
        let app = build_app(state);

        let (status, body) = request_json(app.clone(), Method::GET, "/readyz", None).await;
        assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(
            body,
            json!({
                "error": "readiness_failed",
                "message": "service is not ready",
                "request_id": "test-request-id"
            })
        );

        let (status, body) = request_json(app, Method::GET, "/healthz", None).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(
            body,
            json!({
                "status": "ok",
                "service": APP_NAME
            })
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn frontend_history_routes_fallback_to_index_without_shadowing_api_404s() {
        let temp_dir = tempfile::tempdir().expect("temp dir should be created");
        std::fs::write(temp_dir.path().join("index.html"), "<div id=\"app\"></div>")
            .expect("index file should be written");

        let state = AppState::new(AppConfig {
            database_url: crate::config::DatabaseUrl::sqlite_memory(),
            ..AppConfig::default()
        })
        .await
        .expect("test app state should initialize");
        let app = build_app_with_frontend_dir(state, temp_dir.path().to_path_buf());

        let (status, body) = request_text(app.clone(), "/dashboard").await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body, "<div id=\"app\"></div>");

        let (status, body) = request_json(app.clone(), Method::GET, "/api/", None).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "api root body: {body}");
        assert_eq!(body["error"], "not_found");

        let (status, body) = request_json(app.clone(), Method::GET, "/api/missing", None).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "missing body: {body}");
        assert_eq!(body["error"], "not_found");

        let (status, body) = request_text(app, "/assets/old.js").await;
        assert_eq!(status, StatusCode::NOT_FOUND);
        assert!(!body.contains("<div id=\"app\"></div>"));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn request_ids_security_headers_and_internal_error_redaction_share_one_boundary() {
        let _tracing_guard = TRACING_TEST_LOCK.lock().await;
        let captured = CapturedLogs::default();
        let subscriber = tracing_subscriber::fmt()
            .with_ansi(false)
            .without_time()
            .with_max_level(tracing::Level::TRACE)
            .with_writer(captured.clone())
            .finish();
        let response = build_app(test_state().await)
            .oneshot(
                Request::builder()
                    .uri("/api/__test/internal")
                    .header("x-request-id", "proxy-request_123")
                    .body(Body::empty())
                    .expect("request should build"),
            )
            .with_subscriber(subscriber)
            .await
            .expect("request should succeed");

        assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(response.headers()["x-request-id"], "proxy-request_123");
        assert_eq!(response.headers()[header::CACHE_CONTROL], "no-store");
        assert_eq!(response.headers()["x-content-type-options"], "nosniff");
        assert_eq!(response.headers()["x-frame-options"], "DENY");
        assert!(
            response
                .headers()
                .get("strict-transport-security")
                .is_none()
        );
        assert!(response.headers().get("retry-after").is_none());
        let body = to_bytes(response.into_body(), 4_096)
            .await
            .expect("response body should read");
        let body: Value = serde_json::from_slice(&body).expect("body should be JSON");
        assert_eq!(
            body,
            json!({
                "error": "internal_error",
                "message": "internal server error",
                "request_id": "proxy-request_123"
            })
        );
        assert!(!body.to_string().contains("test-only-secret"));
        let logs = captured.text();
        assert!(logs.contains("http_request_failed"));
        assert!(logs.contains("proxy-request_123"));
        assert!(logs.contains("[REDACTED_DATABASE_URL]"));
        assert!(!logs.contains("test-only-secret"));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn invalid_request_ids_are_replaced_and_returned_on_success() {
        let response = build_app(test_state().await)
            .oneshot(
                Request::builder()
                    .uri("/healthz")
                    .header("x-request-id", "contains spaces")
                    .body(Body::empty())
                    .expect("request should build"),
            )
            .await
            .expect("request should succeed");

        assert_eq!(response.status(), StatusCode::OK);
        let request_id = response.headers()["x-request-id"]
            .to_str()
            .expect("request ID should be UTF-8");
        let uuid = uuid::Uuid::parse_str(request_id).expect("replacement should be a UUID");
        assert_eq!(uuid.get_version_num(), 4);
        assert_eq!(response.headers()[header::CACHE_CONTROL], "no-store");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn access_logs_use_the_matched_route_and_count_streamed_response_bytes() {
        let _tracing_guard = TRACING_TEST_LOCK.lock().await;
        let captured = CapturedLogs::default();
        let subscriber = tracing_subscriber::fmt()
            .with_ansi(false)
            .without_time()
            .with_max_level(tracing::Level::TRACE)
            .with_writer(captured.clone())
            .finish();
        tracing::subscriber::set_global_default(subscriber)
            .expect("global tracing subscriber should only be installed by this test");
        let body = async {
            let response = build_app(test_state().await)
                .oneshot(
                    Request::builder()
                        .uri("/healthz?query-secret-marker")
                        .header("x-request-id", "access-log-test")
                        .body(Body::empty())
                        .expect("request should build"),
                )
                .await
                .expect("request should succeed");
            let mut body = response.into_body();
            let mut body_len = 0;
            while let Some(frame) = std::future::poll_fn(|context| {
                http_body::Body::poll_frame(std::pin::Pin::new(&mut body), context)
            })
            .await
            {
                if let Ok(data) = frame.expect("response frame should read").into_data() {
                    body_len += data.len();
                }
            }
            body_len
        }
        .await;

        let logs = captured.text();
        assert!(
            logs.contains("event=\"http_request_completed\""),
            "captured logs: {logs}"
        );
        assert!(logs.contains("request_id=access-log-test"));
        assert!(logs.contains("route=/healthz"));
        assert!(logs.contains(&format!("response_bytes={body}")));
        assert!(!logs.contains("query-secret-marker"));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn api_rejections_use_stable_json_contracts() {
        let app = build_app(test_state().await);

        let (status, body) = request_json(
            app.clone(),
            Method::POST,
            "/api/__test/json",
            Some(json!({ "title": 7 })),
        )
        .await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(body["error"], "invalid_request");
        assert_eq!(body["request_id"], "test-request-id");

        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/api/__test/json")
                    .header("x-request-id", "test-request-id")
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from("{"))
                    .expect("request should build"),
            )
            .await
            .expect("request should succeed");
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        let body = to_bytes(response.into_body(), 4_096)
            .await
            .expect("response body should read");
        let body: Value = serde_json::from_slice(&body).expect("body should be JSON");
        assert_eq!(body["error"], "invalid_request");
        assert_eq!(body["message"], "request is invalid");

        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/api/__test/json")
                    .header("x-request-id", "test-request-id")
                    .body(Body::from("{}"))
                    .expect("request should build"),
            )
            .await
            .expect("request should succeed");
        assert_eq!(response.status(), StatusCode::UNSUPPORTED_MEDIA_TYPE);
        let body = to_bytes(response.into_body(), 4_096)
            .await
            .expect("response body should read");
        let body: Value = serde_json::from_slice(&body).expect("body should be JSON");
        assert_eq!(body["error"], "unsupported_media_type");

        let (status, body) = request_json(app, Method::PATCH, "/api/__test/json", None).await;
        assert_eq!(status, StatusCode::METHOD_NOT_ALLOWED);
        assert_eq!(body["error"], "method_not_allowed");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn request_timeout_and_body_limit_are_deterministic() {
        let state = AppState::new(AppConfig {
            database_url: crate::config::DatabaseUrl::sqlite_memory(),
            http_request_timeout_ms: 10,
            http_max_request_body_bytes: 8,
            ..AppConfig::default()
        })
        .await
        .expect("test app state should initialize");
        let app = build_app(state);

        let (status, body) = request_json(app.clone(), Method::GET, "/api/__test/slow", None).await;
        assert_eq!(status, StatusCode::REQUEST_TIMEOUT);
        assert_eq!(body["error"], "request_timeout");

        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/api/__test/slow")
                    .body(Body::empty())
                    .expect("request should build"),
            )
            .await
            .expect("request should succeed");
        assert_eq!(response.status(), StatusCode::REQUEST_TIMEOUT);
        assert!(response.headers().get(header::RETRY_AFTER).is_none());

        let response = app
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/api/__test/echo")
                    .header("x-request-id", "test-request-id")
                    .body(Body::from("123456789"))
                    .expect("request should build"),
            )
            .await
            .expect("request should succeed");
        assert_eq!(response.status(), StatusCode::PAYLOAD_TOO_LARGE);
        let body = to_bytes(response.into_body(), 4_096)
            .await
            .expect("response body should read");
        let body: Value = serde_json::from_slice(&body).expect("body should be JSON");
        assert_eq!(body["error"], "payload_too_large");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn concurrency_limit_rejects_protected_routes_without_blocking_health() {
        TEST_SLOW_HANDLER_STARTED.store(false, std::sync::atomic::Ordering::SeqCst);
        let state = AppState::new(AppConfig {
            database_url: crate::config::DatabaseUrl::sqlite_memory(),
            http_request_timeout_ms: 1_000,
            http_max_concurrent_requests: 1,
            ..AppConfig::default()
        })
        .await
        .expect("test app state should initialize");
        let app = build_app(state);
        let slow_app = app.clone();
        let slow_request = tokio::spawn(async move {
            slow_app
                .oneshot(
                    Request::builder()
                        .uri("/api/__test/hold")
                        .body(Body::empty())
                        .expect("request should build"),
                )
                .await
                .expect("slow request should succeed")
        });
        for _ in 0..100 {
            if TEST_SLOW_HANDLER_STARTED.load(std::sync::atomic::Ordering::SeqCst) {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(1)).await;
        }
        assert!(TEST_SLOW_HANDLER_STARTED.load(std::sync::atomic::Ordering::SeqCst));

        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/readyz")
                    .body(Body::empty())
                    .expect("request should build"),
            )
            .await
            .expect("ready request should succeed");
        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
        assert!(response.headers().get(header::RETRY_AFTER).is_none());
        let body = to_bytes(response.into_body(), 4_096)
            .await
            .expect("response body should read");
        let body: Value = serde_json::from_slice(&body).expect("body should be JSON");
        assert_eq!(body["error"], "service_overloaded");

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/healthz")
                    .body(Body::empty())
                    .expect("request should build"),
            )
            .await
            .expect("health request should succeed");
        assert_eq!(response.status(), StatusCode::OK);

        assert_eq!(
            slow_request.await.expect("slow task should join").status(),
            StatusCode::OK
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn request_panics_are_isolated_and_the_process_keeps_serving() {
        let _tracing_guard = TRACING_TEST_LOCK.lock().await;
        let captured = CapturedLogs::default();
        let subscriber = tracing_subscriber::fmt()
            .with_ansi(false)
            .without_time()
            .with_max_level(tracing::Level::TRACE)
            .with_writer(captured.clone())
            .finish();
        let app = build_app(test_state().await);
        let (status, body) =
            async { request_json(app.clone(), Method::GET, "/api/__test/panic", None).await }
                .with_subscriber(subscriber)
                .await;
        assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(body["error"], "internal_error");
        assert!(!body.to_string().contains("panic marker"));
        let logs = captured.text();
        assert!(logs.contains("event=\"http_request_panicked\""));
        assert!(logs.contains("request_id=test-request-id"));
        assert!(logs.contains("test-only HTTP panic marker"));
        assert!(logs.contains("[REDACTED_DATABASE_URL]"));
        assert!(!logs.contains("panic-secret"));
        assert!(logs.contains("backtrace="));
        assert!(logs.contains("test_panic_handler"));

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/healthz")
                    .body(Body::empty())
                    .expect("request should build"),
            )
            .await
            .expect("health request should succeed after panic");
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn static_files_negotiate_precompression_etags_and_cache_identity() {
        let temporary_directory = tempfile::tempdir().expect("temp dir should be created");
        let assets = temporary_directory.path().join("assets");
        std::fs::create_dir_all(&assets).expect("assets directory should be created");
        std::fs::write(
            temporary_directory.path().join("index.html"),
            "index response",
        )
        .expect("index should be written");
        std::fs::write(assets.join("app-HASH.js"), "plain response")
            .expect("plain asset should be written");
        std::fs::write(assets.join("app-HASH.js.br"), "brotli response")
            .expect("Brotli asset should be written");
        std::fs::write(assets.join("app-HASH.js.gz"), "gzip response")
            .expect("gzip asset should be written");
        let app = build_app_with_frontend_dir(
            test_state().await,
            temporary_directory.path().to_path_buf(),
        );

        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/assets/app-HASH.js")
                    .header(header::ACCEPT_ENCODING, "br, gzip")
                    .body(Body::empty())
                    .expect("request should build"),
            )
            .await
            .expect("asset request should succeed");
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(response.headers()[header::CONTENT_ENCODING], "br");
        assert_eq!(response.headers()[header::VARY], "accept-encoding");
        assert_eq!(
            response.headers()[header::CACHE_CONTROL],
            "public, max-age=31536000, immutable"
        );
        let etag = response.headers()[header::ETAG].clone();
        assert!(!etag.as_bytes().starts_with(b"W/"));
        let body = to_bytes(response.into_body(), 1_024)
            .await
            .expect("asset body should read");
        assert_eq!(body.as_ref(), b"brotli response");

        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/assets/app-HASH.js")
                    .header(header::ACCEPT_ENCODING, "br")
                    .header(header::IF_NONE_MATCH, etag)
                    .body(Body::empty())
                    .expect("request should build"),
            )
            .await
            .expect("conditional request should succeed");
        assert_eq!(response.status(), StatusCode::NOT_MODIFIED);
        assert_eq!(
            response.headers()[header::CACHE_CONTROL],
            "public, max-age=31536000, immutable"
        );

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/dashboard")
                    .header(header::ACCEPT, "text/html")
                    .body(Body::empty())
                    .expect("request should build"),
            )
            .await
            .expect("SPA request should succeed");
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(response.headers()[header::CACHE_CONTROL], "no-cache");
        assert!(response.headers().get(header::ETAG).is_some());
    }
}
