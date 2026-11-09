use axum::body::{to_bytes, Body};
use axum::http::{header, Request, StatusCode};
use lkb::config::{Backend, Config, ProfileName};
use lkb::governance::{SOLO_SPACE, SOLO_TENANT};
use lkb::providers::ProviderSet;
use lkb::store;
use lkb::web::{router, AppState};
use std::sync::Arc;
use tower::ServiceExt;

fn test_config(path: &std::path::Path) -> Config {
    let mut config = Config {
        profile: Some(ProfileName::Solo),
        ..Default::default()
    };
    config.storage.backend = Backend::Sqlite;
    config.storage.sqlite_path = path.to_path_buf();
    config.storage.tenant_id = SOLO_TENANT.to_string();
    config.storage.default_space = SOLO_SPACE.to_string();
    config
}

fn form_encode(value: &str) -> String {
    let mut out = String::new();
    for &byte in value.as_bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char)
            }
            b' ' => out.push('+'),
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

#[tokio::test]
async fn health_root_and_wiki_save_work_through_router() {
    let temp = tempfile::tempdir().unwrap();
    let config = test_config(&temp.path().join("lkb.sqlite"));
    let store = store::open(&config).await.unwrap();
    store::bootstrap_space(&store, &config).await.unwrap();
    let state = AppState::new(store, Arc::new(config), Arc::new(ProviderSet::with_none()));
    let app = router(state);

    let health = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/health")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(health.status(), StatusCode::OK);
    let bytes = to_bytes(health.into_body(), 1_000_000).await.unwrap();
    let text = String::from_utf8(bytes.to_vec()).unwrap();
    assert!(text.contains("\"status\":\"ok\""));

    let root = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/")
                .header(header::HOST, "localhost")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(root.status(), StatusCode::OK);
    let set_cookie = root
        .headers()
        .get(header::SET_COOKIE)
        .expect("csrf cookie")
        .to_str()
        .unwrap()
        .to_string();
    let csrf = set_cookie
        .split(';')
        .next()
        .unwrap()
        .trim_start_matches("lkb_csrf=")
        .to_string();
    assert!(!csrf.is_empty());

    let body = format!(
        "title={}&markdown={}&csrf_token={}",
        form_encode("Test page"),
        form_encode("# Hello\n\nbody"),
        form_encode(&csrf)
    );
    let save = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/w/s_default/test")
                .header(header::HOST, "localhost")
                .header(header::ORIGIN, "http://localhost")
                .header(header::COOKIE, format!("lkb_csrf={csrf}"))
                .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(save.status(), StatusCode::SEE_OTHER);

    let page = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/w/s_default/test")
                .header(header::HOST, "localhost")
                .header(header::COOKIE, format!("lkb_csrf={csrf}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(page.status(), StatusCode::OK);
    let bytes = to_bytes(page.into_body(), 1_000_000).await.unwrap();
    let html = String::from_utf8(bytes.to_vec()).unwrap();
    assert!(html.contains("Hello"));
    assert!(html.contains("body"));

    let api = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/search?q=Hello")
                .header(header::HOST, "localhost")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(api.status(), StatusCode::OK);
    let bytes = to_bytes(api.into_body(), 1_000_000).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert!(!json["hits"].as_array().unwrap().is_empty());

    let ask = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/ask")
                .header(header::HOST, "localhost")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(r#"{"query":"Hello"}"#.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(ask.status(), StatusCode::OK);
    let bytes = to_bytes(ask.into_body(), 1_000_000).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert!(!json["citations"].as_array().unwrap().is_empty());
}

#[tokio::test]
async fn origin_mismatch_is_rejected_on_save() {
    let temp = tempfile::tempdir().unwrap();
    let config = test_config(&temp.path().join("lkb.sqlite"));
    let store = store::open(&config).await.unwrap();
    store::bootstrap_space(&store, &config).await.unwrap();
    let state = AppState::new(store, Arc::new(config), Arc::new(ProviderSet::with_none()));
    let app = router(state);

    let root = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/")
                .header(header::HOST, "localhost")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let set_cookie = root
        .headers()
        .get(header::SET_COOKIE)
        .unwrap()
        .to_str()
        .unwrap()
        .to_string();
    let csrf = set_cookie
        .split(';')
        .next()
        .unwrap()
        .trim_start_matches("lkb_csrf=")
        .to_string();
    let body = format!("title=x&markdown=y&csrf_token={}", form_encode(&csrf));
    let save = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/w/s_default/test")
                .header(header::HOST, "localhost")
                .header(header::ORIGIN, "http://evil.example")
                .header(header::COOKIE, format!("lkb_csrf={csrf}"))
                .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(save.status(), StatusCode::FORBIDDEN);
}
