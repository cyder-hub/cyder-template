use std::sync::Arc;

use axum::{Router, routing::get};
use tower_http::services::ServeDir;

use crate::{config::AppConfig, controller, database, error::AppResult, id::IdGenerator};

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
        .fallback_service(ServeDir::new(public_dir))
        .with_state(state)
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
        let item_id = created["id"]
            .as_i64()
            .expect("created item id should exist");
        assert!(item_id > 0);
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
        let user_id = created["id"]
            .as_i64()
            .expect("created user id should exist");
        assert!(user_id > 0);
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
}
