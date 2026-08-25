use axum::{
    Router,
    body::{Body, to_bytes},
    http::{Method, Request, StatusCode, header},
};
use serde_json::{Value, json};
use tower::ServiceExt;

use super::{AppConfig, AppState, build_app};

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

fn json_id_as_i64(value: &Value) -> i64 {
    value
        .as_str()
        .and_then(|id| id.parse::<i64>().ok())
        .expect("json id should be a signed 64-bit integer string")
}

#[tokio::test(flavor = "multi_thread")]
async fn items_api_creates_lists_reads_deletes_and_returns_404() {
    let app = build_app(test_state().await);

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

#[tokio::test(flavor = "multi_thread")]
async fn users_api_creates_lists_reads_deletes_and_returns_404() {
    let app = build_app(test_state().await);

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
