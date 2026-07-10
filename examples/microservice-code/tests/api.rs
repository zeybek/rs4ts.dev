//! End-to-end HTTP tests that drive the real router in-process.
//!
//! These spin up the service on an ephemeral port (`127.0.0.1:0`), fire real
//! HTTP requests with `reqwest`, and assert on the responses — the Rust
//! analogue of a `supertest` suite against an Express app.

use std::net::SocketAddr;

use tokio::net::TcpListener;

/// Boot the app on a random free port and return its base URL.
async fn spawn_app() -> String {
    // Force a deterministic, machine-friendly config for the test process.
    unsafe {
        std::env::set_var("HOST", "127.0.0.1");
        std::env::set_var("PORT", "0");
        std::env::set_var("LOG_FORMAT", "pretty");
        std::env::set_var("RUST_LOG", "warn");
    }

    let settings =
        url_shortener::config::Settings::from_env().expect("test env config must be valid");
    let state = url_shortener::state::AppState::new(settings);
    let app = url_shortener::routes::build_router(state);

    let listener = TcpListener::bind(SocketAddr::from(([127, 0, 0, 1], 0)))
        .await
        .unwrap();
    let addr = listener.local_addr().unwrap();

    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    format!("http://{addr}")
}

#[tokio::test]
async fn shorten_then_redirect() {
    let base = spawn_app().await;
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .unwrap();

    // Create a short link.
    let resp = client
        .post(format!("{base}/shorten"))
        .json(&serde_json::json!({ "url": "https://www.rust-lang.org" }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    let body: serde_json::Value = resp.json().await.unwrap();
    let code = body["code"].as_str().unwrap().to_string();
    assert_eq!(body["target"], "https://www.rust-lang.org/");

    // Following the code yields a redirect to the target.
    let redirect = client.get(format!("{base}/{code}")).send().await.unwrap();
    assert_eq!(redirect.status(), 307);
    assert_eq!(
        redirect.headers().get("location").unwrap(),
        "https://www.rust-lang.org/"
    );
}

#[tokio::test]
async fn rejects_invalid_url() {
    let base = spawn_app().await;
    let client = reqwest::Client::new();

    let resp = client
        .post(format!("{base}/shorten"))
        .json(&serde_json::json!({ "url": "not-a-url" }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 400);

    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["error"], "invalid_url");
}

#[tokio::test]
async fn rejects_url_with_control_characters() {
    let base = spawn_app().await;
    let client = reqwest::Client::new();

    let resp = client
        .post(format!("{base}/shorten"))
        .json(&serde_json::json!({ "url": "https://example.com\ninvalid" }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 400);

    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["error"], "invalid_url");
}

#[tokio::test]
async fn unknown_code_is_404() {
    let base = spawn_app().await;
    let client = reqwest::Client::new();

    let resp = client
        .get(format!("{base}/doesnotexist"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 404);
}

#[tokio::test]
async fn health_endpoint_reports_ok() {
    let base = spawn_app().await;
    let resp = reqwest::get(format!("{base}/health")).await.unwrap();
    assert_eq!(resp.status(), 200);

    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["status"], "ok");
}
