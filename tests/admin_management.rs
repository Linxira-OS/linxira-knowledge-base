use axum::body::Body;
use axum::http::{header, Request, StatusCode};
use axum::routing::post;
use axum::{Json, Router};
use lkb::billing::{Billing, SubscriptionStatus};
use lkb::config::Config;
use lkb::providers::ProviderSet;
use lkb::store::{self};
use lkb::web::{router, AppState};
use std::sync::Arc;
use tower::ServiceExt;

fn test_config(path: &std::path::Path) -> Config {
    let mut config = Config::default();
    config.storage.sqlite_path = path.to_path_buf();
    config.storage.tenant_id = "t_local".into();
    config.storage.default_space = "s_default".into();
    config.features.audit = Some(true);
    config.features.billing = Some(true);
    config
}

async fn json_request(
    app: &Router,
    method: &str,
    uri: &str,
    body: serde_json::Value,
) -> (StatusCode, serde_json::Value) {
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method(method)
                .uri(uri)
                .header(header::HOST, "localhost")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let json = serde_json::from_slice(
        &axum::body::to_bytes(response.into_body(), 2_000_000)
            .await
            .unwrap(),
    )
    .unwrap_or(serde_json::Value::Null);
    (status, json)
}

async fn get(app: &Router, uri: &str) -> (StatusCode, serde_json::Value) {
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(uri)
                .header(header::HOST, "localhost")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let json = serde_json::from_slice(
        &axum::body::to_bytes(response.into_body(), 2_000_000)
            .await
            .unwrap(),
    )
    .unwrap_or(serde_json::Value::Null);
    (status, json)
}

#[tokio::test]
async fn admin_user_create_reset_disable_and_last_login() {
    let temp = tempfile::tempdir().unwrap();
    let config = test_config(&temp.path().join("admin-users.sqlite"));
    let store = store::open(&config).await.unwrap();
    store::bootstrap_space(&store, &config).await.unwrap();
    let app = router(AppState::new(
        store.clone(),
        Arc::new(config.clone()),
        Arc::new(ProviderSet::with_none()),
    ));

    let (status, json) = json_request(&app, "POST", "/api/v1/admin/users", serde_json::json!({
        "username": "newuser", "password": "secret123", "display_name": "New User", "role": "admin"
    })).await;
    assert_eq!(status, StatusCode::OK, "{json}");
    let user_id = json["user"]["user_id"].as_str().unwrap().to_string();
    assert_eq!(json["user"]["role"], "admin");
    assert_eq!(json["user"]["display_name"], "New User");
    assert_eq!(json["user"]["source"], "local");
    let hash_before = store
        .get_user("t_local", "newuser")
        .await
        .unwrap()
        .unwrap()
        .password_hash;

    let (duplicate, _) = json_request(
        &app,
        "POST",
        "/api/v1/admin/users",
        serde_json::json!({
            "username": "newuser", "password": "secret123"
        }),
    )
    .await;
    assert_eq!(duplicate, StatusCode::CONFLICT);

    let reset = json_request(
        &app,
        "POST",
        &format!("/api/v1/admin/users/{user_id}/reset-password"),
        serde_json::json!({
            "password": "newsecret123"
        }),
    )
    .await;
    assert_eq!(reset.0, StatusCode::OK);
    let hash_after = store
        .get_user("t_local", "newuser")
        .await
        .unwrap()
        .unwrap()
        .password_hash;
    assert_ne!(hash_before, hash_after);

    store
        .touch_user_login("t_local", &user_id, "2026-01-01T00:00:00Z")
        .await
        .unwrap();
    let (_, users) = get(&app, "/api/v1/admin/users?limit=10").await;
    let listed = users["users"].as_array().unwrap();
    let user = listed
        .iter()
        .find(|item| item["username"] == "newuser")
        .unwrap();
    assert_eq!(user["last_login_at"], "2026-01-01T00:00:00Z");
    assert!(user.get("email").is_some() && user.get("source").is_some());

    let delete = app
        .clone()
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri(format!("/api/v1/admin/users/{user_id}"))
                .header(header::HOST, "localhost")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(delete.status(), StatusCode::OK);
    assert!(
        store
            .get_user_by_id("t_local", &user_id)
            .await
            .unwrap()
            .unwrap()
            .disabled
    );
}

#[tokio::test]
async fn admin_plans_crud_subscriptions_and_in_use_guard() {
    let temp = tempfile::tempdir().unwrap();
    let config = test_config(&temp.path().join("admin-plans.sqlite"));
    let store = store::open(&config).await.unwrap();
    store::bootstrap_space(&store, &config).await.unwrap();
    let app = router(AppState::new(
        store.clone(),
        Arc::new(config.clone()),
        Arc::new(ProviderSet::with_none()),
    ));

    let (status, created) = json_request(&app, "POST", "/api/v1/admin/billing/plans", serde_json::json!({
        "plan_id": "teamx", "name": "Team X", "price_micros": 1000, "included_tokens": 100, "currency": "USD"
    })).await;
    assert_eq!(status, StatusCode::OK, "{created}");
    let (_, plans) = get(&app, "/api/v1/admin/billing/plans").await;
    assert!(plans["plans"]
        .as_array()
        .unwrap()
        .iter()
        .any(|plan| plan["plan_id"] == "teamx"));

    let (status, patched) = json_request(
        &app,
        "PATCH",
        "/api/v1/admin/billing/plans/teamx",
        serde_json::json!({
            "price_micros": 2000, "status": "active"
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(patched["plan"]["price_micros"], 2000);

    let billing = Billing::new(store.clone(), config.billing.clone());
    billing.subscribe("t_local", "teamx", 0).await.unwrap();
    let (_, subs) = get(&app, "/api/v1/admin/billing/subscriptions?status=active").await;
    assert_eq!(subs["subscriptions"].as_array().unwrap().len(), 1);

    let delete = app
        .clone()
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri("/api/v1/admin/billing/plans/teamx")
                .header(header::HOST, "localhost")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(delete.status(), StatusCode::CONFLICT);
    billing
        .transition("t_local", SubscriptionStatus::Cancelled)
        .await
        .unwrap();
    let delete = app
        .clone()
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri("/api/v1/admin/billing/plans/teamx")
                .header(header::HOST, "localhost")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(delete.status(), StatusCode::OK);
    assert!(store.billing_get_plan("teamx").await.unwrap().is_none());
}

async fn fake_provider() -> Json<serde_json::Value> {
    Json(
        serde_json::json!({"choices": [{"message": {"role": "assistant", "content": "pong"}}], "usage": {"prompt_tokens": 1, "completion_tokens": 1}}),
    )
}

#[tokio::test]
async fn admin_provider_models_and_provider_test() {
    let temp = tempfile::tempdir().unwrap();
    let config = test_config(&temp.path().join("admin-models.sqlite"));
    let store = store::open(&config).await.unwrap();
    store::bootstrap_space(&store, &config).await.unwrap();
    let provider = Router::new().route("/chat/completions", post(fake_provider));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, provider).await.unwrap() });
    let app = router(AppState::new(
        store.clone(),
        Arc::new(config.clone()),
        Arc::new(ProviderSet::with_none()),
    ));

    let (_, created_provider) = json_request(&app, "POST", "/api/v1/admin/providers", serde_json::json!({
        "provider_id": "p_admin", "kind": "openai_compatible", "base_url": format!("http://{addr}"), "status": "active"
    })).await;
    assert_eq!(created_provider["provider"]["provider_id"], "p_admin");
    let (status, created_model) = json_request(&app, "POST", "/api/v1/admin/provider-models", serde_json::json!({
        "provider_model_id": "pm_admin", "provider_id": "p_admin", "upstream_model": "mock-model", "capabilities_json": "[\"chat\"]"
    })).await;
    assert_eq!(status, StatusCode::CREATED, "{created_model}");

    let debug = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/admin/providers/p_admin/test")
                .header(header::HOST, "localhost")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from("{}"))
                .unwrap(),
        )
        .await
        .unwrap();
    let debug_status = debug.status();
    let debug_body = String::from_utf8_lossy(
        &axum::body::to_bytes(debug.into_body(), 1_000_000)
            .await
            .unwrap(),
    )
    .to_string();
    eprintln!("provider test debug: {debug_status} {debug_body:?}");
    let test_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/admin/providers/p_admin/test")
                .header(header::HOST, "localhost")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from("{}"))
                .unwrap(),
        )
        .await
        .unwrap();
    let test_status = test_response.status();
    let test_body = axum::body::to_bytes(test_response.into_body(), 1_000_000)
        .await
        .unwrap();
    eprintln!(
        "provider test second: {test_status} {:?}",
        String::from_utf8_lossy(&test_body)
    );
    let test = (
        test_status,
        serde_json::from_slice::<serde_json::Value>(&test_body).unwrap_or(serde_json::Value::Null),
    );
    assert_eq!(test.0, StatusCode::OK, "body={:?}", test.1);
    assert_eq!(test.1["ok"], true);
    assert!(test.1["latency_ms"].is_number());

    let (status, patched) = json_request(
        &app,
        "PATCH",
        "/api/v1/admin/provider-models/pm_admin",
        serde_json::json!({
            "upstream_model": "mock-model-2", "status": "active"
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(patched["model"]["upstream_model"], "mock-model-2");

    let delete = app
        .clone()
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri("/api/v1/admin/provider-models/pm_admin")
                .header(header::HOST, "localhost")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(delete.status(), StatusCode::OK);
    let no_model = json_request(
        &app,
        "POST",
        "/api/v1/admin/providers/p_admin/test",
        serde_json::json!({}),
    )
    .await;
    assert_eq!(no_model.1["ok"], false);
    assert_eq!(no_model.1["code"], "provider_not_configured");
}

#[tokio::test]
async fn admin_overview_and_audit_detail_filters_cursor() {
    let temp = tempfile::tempdir().unwrap();
    let config = test_config(&temp.path().join("admin-overview.sqlite"));
    let store = store::open(&config).await.unwrap();
    store::bootstrap_space(&store, &config).await.unwrap();
    let app = router(AppState::new(
        store.clone(),
        Arc::new(config.clone()),
        Arc::new(ProviderSet::with_none()),
    ));

    json_request(
        &app,
        "POST",
        "/api/v1/admin/users",
        serde_json::json!({"username": "audituser", "password": "secret123"}),
    )
    .await;
    let (status, overview) = get(&app, "/api/v1/admin/overview").await;
    assert_eq!(status, StatusCode::OK, "{overview}");
    assert!(overview["counts"]["users"].as_i64().unwrap() >= 1);
    assert!(overview["counts"]["spaces"].as_i64().unwrap() >= 1);
    assert!(overview["last_24h"]["requests"].as_i64().is_some());
    assert!(overview["last_24h"]["cache_hit_rate"].as_f64().is_some());
    assert!(overview["recent_audit"].is_array());

    let (_, page) = get(&app, "/api/v1/admin/audit?action=admin.user.create&limit=1").await;
    let events = page["events"].as_array().unwrap();
    assert_eq!(events.len(), 1);
    let event_id = events[0]["event_id"].as_str().unwrap();
    let (status, detail) = get(&app, format!("/api/v1/admin/audit/{event_id}").as_str()).await;
    assert_eq!(status, StatusCode::OK, "{detail}");
    assert_eq!(detail["event"]["event_id"], event_id);

    let (_, filtered) = get(&app, "/api/v1/admin/audit?actor=local&space=&from=2020-01-01T00:00:00Z&to=2030-01-01T00:00:00Z&limit=100").await;
    assert!(filtered["events"].is_array());
}
