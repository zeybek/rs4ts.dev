//! In-process integration tests for the tasks API.
//!
//! These build the real `Router` and drive it with `tower::ServiceExt::oneshot`
//! — no real TCP socket is opened. This is the Rust equivalent of `supertest`
//! against an Express app instance.

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use http_body_util::BodyExt; // for `.collect()` to read the response body
use rest_api::{TaskStore, app};
use serde_json::{Value, json};
use tower::ServiceExt; // for `oneshot`

/// Helper: send a request through the app and return (status, parsed JSON).
async fn send(req: Request<Body>) -> (StatusCode, Value) {
    let response = app(TaskStore::new())
        .oneshot(req)
        .await
        .expect("router error");
    let status = response.status();
    let bytes = response
        .into_body()
        .collect()
        .await
        .expect("read body")
        .to_bytes();
    let value: Value = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes).expect("valid json")
    };
    (status, value)
}

#[tokio::test]
async fn health_check_returns_ok() {
    let req = Request::builder()
        .uri("/health")
        .body(Body::empty())
        .unwrap();
    let (status, body) = send(req).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, json!({ "status": "ok" }));
}

#[tokio::test]
async fn list_is_empty_initially() {
    let req = Request::builder()
        .uri("/tasks")
        .body(Body::empty())
        .unwrap();
    let (status, body) = send(req).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, json!([]));
}

#[tokio::test]
async fn create_then_get_roundtrip() {
    let store = TaskStore::new();

    // Create a task.
    let create = Request::builder()
        .method("POST")
        .uri("/tasks")
        .header("content-type", "application/json")
        .body(Body::from(
            json!({ "title": "Write tests", "description": "cover the CRUD paths" })
                .to_string(),
        ))
        .unwrap();
    let response = app(store.clone()).oneshot(create).await.unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    let created: Value = serde_json::from_slice(&bytes).unwrap();
    let id = created["id"].as_str().unwrap().to_string();
    assert_eq!(created["title"], "Write tests");
    assert_eq!(created["completed"], false);

    // Fetch it back.
    let get = Request::builder()
        .uri(format!("/tasks/{id}"))
        .body(Body::empty())
        .unwrap();
    let response = app(store).oneshot(get).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn get_missing_returns_404() {
    let id = uuid::Uuid::new_v4();
    let req = Request::builder()
        .uri(format!("/tasks/{id}"))
        .body(Body::empty())
        .unwrap();
    let (status, body) = send(req).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body["error"]["code"], 404);
}

#[tokio::test]
async fn empty_title_is_rejected() {
    let req = Request::builder()
        .method("POST")
        .uri("/tasks")
        .header("content-type", "application/json")
        .body(Body::from(json!({ "title": "   " }).to_string()))
        .unwrap();
    let (status, body) = send(req).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(body["error"]["code"], 422);
}

#[tokio::test]
async fn malformed_json_is_bad_request() {
    let req = Request::builder()
        .method("POST")
        .uri("/tasks")
        .header("content-type", "application/json")
        .body(Body::from("{not json"))
        .unwrap();
    let (status, body) = send(req).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"]["code"], 400);
}

#[tokio::test]
async fn update_and_delete() {
    let store = TaskStore::new();

    // Seed a task.
    let create = Request::builder()
        .method("POST")
        .uri("/tasks")
        .header("content-type", "application/json")
        .body(Body::from(json!({ "title": "Original" }).to_string()))
        .unwrap();
    let response = app(store.clone()).oneshot(create).await.unwrap();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    let created: Value = serde_json::from_slice(&bytes).unwrap();
    let id = created["id"].as_str().unwrap().to_string();

    // Update title + completed.
    let update = Request::builder()
        .method("PUT")
        .uri(format!("/tasks/{id}"))
        .header("content-type", "application/json")
        .body(Body::from(
            json!({ "title": "Updated", "completed": true }).to_string(),
        ))
        .unwrap();
    let response = app(store.clone()).oneshot(update).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    let updated: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(updated["title"], "Updated");
    assert_eq!(updated["completed"], true);

    // Delete it -> 204.
    let delete = Request::builder()
        .method("DELETE")
        .uri(format!("/tasks/{id}"))
        .body(Body::empty())
        .unwrap();
    let response = app(store.clone()).oneshot(delete).await.unwrap();
    assert_eq!(response.status(), StatusCode::NO_CONTENT);

    // Deleting again -> 404.
    let delete_again = Request::builder()
        .method("DELETE")
        .uri(format!("/tasks/{id}"))
        .body(Body::empty())
        .unwrap();
    let response = app(store).oneshot(delete_again).await.unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}
