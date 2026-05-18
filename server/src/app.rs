use std::{convert::Infallible, io::ErrorKind, path::PathBuf, sync::Arc};

use axum::{
    Json, Router,
    body::Body,
    http::{HeaderMap, Method, Request, StatusCode, header},
    response::{Html, IntoResponse, Response},
    routing::{any, get},
};
use tower::service_fn;
use tower_http::services::ServeDir;

use crate::{
    config::AppConfig,
    controller, database,
    error::{AppResult, ErrorResponse},
    id::IdGenerator,
};

pub const APP_NAME: &str = "cyder-template";

#[derive(Clone)]
pub struct AppState {
    config: Arc<AppConfig>,
    database: database::DbPool,
    #[allow(dead_code)]
    id_generator: Arc<IdGenerator>,
}

impl AppState {
    pub fn new(config: AppConfig) -> AppResult<Self> {
        let database = database::DbPool::connect(&config.database_url, config.database_pool_size)?;
        let id_generator = IdGenerator::for_worker(config.id_worker_id)?;

        Ok(Self {
            config: Arc::new(config),
            database,
            id_generator: Arc::new(id_generator),
        })
    }

    pub fn config(&self) -> &AppConfig {
        self.config.as_ref()
    }

    pub fn database(&self) -> &database::DbPool {
        &self.database
    }

    #[allow(dead_code)]
    pub fn id_generator(&self) -> &IdGenerator {
        &self.id_generator
    }
}

pub fn build_app(state: AppState) -> Router {
    let public_dir = state.config().public_dir.clone();
    let index_file = PathBuf::from(&public_dir).join("index.html");
    let static_files =
        ServeDir::new(public_dir).fallback(service_fn(move |request: Request<Body>| {
            let index_file = index_file.clone();
            async move { Ok::<_, Infallible>(spa_fallback(request, index_file).await) }
        }));

    Router::new()
        .route("/healthz", get(controller::health::healthz))
        .route("/readyz", get(controller::health::readyz))
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
        .route("/api", any(api_not_found))
        .route("/api/", any(api_not_found))
        .route("/api/{*path}", any(api_not_found))
        .fallback_service(static_files)
        .with_state(state)
}

async fn api_not_found() -> (StatusCode, Json<ErrorResponse>) {
    (
        StatusCode::NOT_FOUND,
        Json(ErrorResponse {
            error: "not_found".to_string(),
            message: "API route was not found".to_string(),
        }),
    )
}

async fn spa_fallback(request: Request<Body>, index_file: PathBuf) -> Response {
    if !should_serve_spa_index(&request) {
        return StatusCode::NOT_FOUND.into_response();
    }

    match tokio::fs::read_to_string(index_file).await {
        Ok(index) => Html(index).into_response(),
        Err(source) if source.kind() == ErrorKind::NotFound => {
            StatusCode::NOT_FOUND.into_response()
        }
        Err(source) => {
            tracing::error!(error = %source, "failed to read SPA index fallback");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
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

#[cfg(test)]
mod tests {
    use axum::{
        body::{Body, to_bytes},
        http::{Method, Request, StatusCode, header},
    };
    use serde_json::{Value, json};
    use tower::ServiceExt;

    use super::*;

    fn test_state() -> AppState {
        AppState::new(AppConfig {
            database_url: ":memory:".to_string(),
            ..AppConfig::default()
        })
        .expect("test app state should initialize")
    }

    fn test_state_with_sqlite_file(file_name: &str) -> (tempfile::TempDir, AppState) {
        let temp_dir = tempfile::tempdir().expect("temp dir should be created");
        let database_url = temp_dir
            .path()
            .join(file_name)
            .to_string_lossy()
            .into_owned();

        let state = AppState::new(AppConfig {
            database_url,
            database_pool_size: 1,
            ..AppConfig::default()
        })
        .expect("test app state should initialize");

        (temp_dir, state)
    }

    async fn request_json(
        app: Router,
        method: Method,
        uri: &str,
        body: Option<Value>,
    ) -> (StatusCode, Value) {
        let mut builder = Request::builder().method(method).uri(uri);
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

    fn json_id_as_i64(value: &Value) -> i64 {
        value
            .as_str()
            .and_then(|id| id.parse::<i64>().ok())
            .expect("json id should be a signed 64-bit integer string")
    }

    #[tokio::test]
    async fn healthz_returns_ok() {
        let response = build_app(test_state())
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

    #[tokio::test]
    async fn readyz_checks_database() {
        let response = build_app(test_state())
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

    #[tokio::test]
    async fn items_api_creates_lists_reads_deletes_and_returns_404() {
        let (_temp_dir, state) = test_state_with_sqlite_file("items-api.sqlite");
        let app = build_app(state);

        let (status, body) = request_json(app.clone(), Method::GET, "/api/items", None).await;
        assert_eq!(status, StatusCode::OK, "list body: {body}");
        assert_eq!(body, json!([]));

        let (status, created) = request_json(
            app.clone(),
            Method::POST,
            "/api/items",
            Some(json!({
                "title": "Ship CRUD",
                "description": "Wire HTTP handlers",
                "completed": true
            })),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "create body: {created}");
        let item_id = json_id_as_i64(&created["id"]);
        assert!(item_id > 0);
        assert!(created["id"].is_string());
        assert_eq!(created["title"], "Ship CRUD");
        assert_eq!(created["description"], "Wire HTTP handlers");
        assert_eq!(created["completed"], true);

        let (status, fetched) = request_json(
            app.clone(),
            Method::GET,
            &format!("/api/items/{item_id}"),
            None,
        )
        .await;
        assert_eq!(status, StatusCode::OK, "get body: {fetched}");
        assert_eq!(fetched, created);

        let (status, listed) = request_json(app.clone(), Method::GET, "/api/items", None).await;
        assert_eq!(status, StatusCode::OK, "list body: {listed}");
        assert_eq!(listed, json!([created]));

        let (status, deleted) = request_json(
            app.clone(),
            Method::DELETE,
            &format!("/api/items/{item_id}"),
            None,
        )
        .await;
        assert_eq!(status, StatusCode::OK, "delete body: {deleted}");
        assert_eq!(deleted, json!({ "deleted": true }));

        let (status, missing) =
            request_json(app, Method::GET, &format!("/api/items/{item_id}"), None).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "missing body: {missing}");
        assert_eq!(missing["error"], "not_found");
    }

    #[tokio::test]
    async fn users_api_creates_lists_reads_deletes_and_returns_404() {
        let (_temp_dir, state) = test_state_with_sqlite_file("users-api.sqlite");
        let app = build_app(state);

        let (status, body) = request_json(app.clone(), Method::GET, "/api/users", None).await;
        assert_eq!(status, StatusCode::OK, "list body: {body}");
        assert_eq!(body, json!([]));

        let (status, created) = request_json(
            app.clone(),
            Method::POST,
            "/api/users",
            Some(json!({
                "name": "Template Operator",
                "email": "operator@example.com",
                "active": false
            })),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "create body: {created}");
        let user_id = json_id_as_i64(&created["id"]);
        assert!(user_id > 0);
        assert!(created["id"].is_string());
        assert_eq!(created["name"], "Template Operator");
        assert_eq!(created["email"], "operator@example.com");
        assert_eq!(created["active"], false);

        let (status, fetched) = request_json(
            app.clone(),
            Method::GET,
            &format!("/api/users/{user_id}"),
            None,
        )
        .await;
        assert_eq!(status, StatusCode::OK, "get body: {fetched}");
        assert_eq!(fetched, created);

        let (status, listed) = request_json(app.clone(), Method::GET, "/api/users", None).await;
        assert_eq!(status, StatusCode::OK, "list body: {listed}");
        assert_eq!(listed, json!([created]));

        let (status, deleted) = request_json(
            app.clone(),
            Method::DELETE,
            &format!("/api/users/{user_id}"),
            None,
        )
        .await;
        assert_eq!(status, StatusCode::OK, "delete body: {deleted}");
        assert_eq!(deleted, json!({ "deleted": true }));

        let (status, missing) =
            request_json(app, Method::DELETE, &format!("/api/users/{user_id}"), None).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "missing body: {missing}");
        assert_eq!(missing["error"], "not_found");
    }

    #[tokio::test]
    async fn frontend_history_routes_fallback_to_index_without_shadowing_api_404s() {
        let temp_dir = tempfile::tempdir().expect("temp dir should be created");
        std::fs::write(temp_dir.path().join("index.html"), "<div id=\"app\"></div>")
            .expect("index file should be written");

        let state = AppState::new(AppConfig {
            database_url: ":memory:".to_string(),
            public_dir: temp_dir.path().to_string_lossy().into_owned(),
            ..AppConfig::default()
        })
        .expect("test app state should initialize");
        let app = build_app(state);

        let (status, body) = request_text(app.clone(), "/items").await;
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
}
