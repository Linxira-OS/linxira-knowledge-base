// SPDX-License-Identifier: MIT OR Apache-2.0
//! Layered quota feature gate: solo is read-only by default, team and
//! enterprise enable quota management by default, and an explicit override
//! always wins.

use axum::body::Body;
use axum::http::{header, Request, StatusCode};
use lkb::config::{AuthMode, Backend, Config, DeploymentMode, ProfileName};
use lkb::governance::{Acl, SOLO_SPACE, SOLO_TENANT};
use lkb::providers::ProviderSet;
use lkb::store::{self, SpaceRecord};
use lkb::util::{now_rfc3339, random_hex};
use lkb::web::{router, AppState};
use std::path::Path;
use std::sync::Arc;
use tower::ServiceExt;

fn sqlite_config(path: &Path) -> Config {
    let mut config = Config::default();
    config.storage.sqlite_path = path.to_path_buf();
    config.auth.mode = AuthMode::None;
    config
}

async fn open_sqlite_app(config: Config) -> axum::Router {
    let store = store::open(&config).await.unwrap();
    store::bootstrap_space(&store, &config).await.unwrap();
    router(AppState::new(
        store,
        Arc::new(config),
        Arc::new(ProviderSet::with_none()),
    ))
}

async fn body_json(response: axum::response::Response) -> serde_json::Value {
    let bytes = axum::body::to_bytes(response.into_body(), 1_000_000)
        .await
        .unwrap();
    serde_json::from_slice(&bytes).unwrap()
}

async fn send(
    app: &axum::Router,
    method: &str,
    uri: &str,
    body: Option<serde_json::Value>,
) -> (StatusCode, serde_json::Value) {
    let mut builder = Request::builder()
        .method(method)
        .uri(uri)
        .header(header::HOST, "localhost")
        .header(header::CONTENT_TYPE, "application/json");
    if method == "PUT" && uri.starts_with("/api/v1/pages/") {
        builder = builder.header(header::IF_MATCH, "*");
    }
    let body = match body {
        Some(value) => Body::from(value.to_string()),
        None => Body::empty(),
    };
    let response = app
        .clone()
        .oneshot(builder.body(body).unwrap())
        .await
        .unwrap();
    let status = response.status();
    (status, body_json(response).await)
}

#[tokio::test]
async fn solo_quota_writes_are_feature_disabled_but_usage_stays_readable() {
    let temp = tempfile::tempdir().unwrap();
    let app = open_sqlite_app(sqlite_config(&temp.path().join("solo.sqlite"))).await;

    let (status, json) = send(&app, "GET", "/api/v1/features", None).await;
    assert_eq!(status, StatusCode::OK, "{json}");
    assert_eq!(json["features"]["quotas"], false);

    for (method, uri, body) in [
        (
            "PUT",
            format!("/api/v1/spaces/{SOLO_SPACE}/quota"),
            Some(serde_json::json!({"storage_bytes": 1024})),
        ),
        (
            "POST",
            "/api/v1/admin/quotas".to_string(),
            Some(serde_json::json!({
                "scope_type": "user", "scope_id": "local", "period": "month",
                "limit_tokens": 100
            })),
        ),
        (
            "DELETE",
            "/api/v1/admin/quotas/tq_missing".to_string(),
            None,
        ),
        (
            "POST",
            "/api/v1/admin/quota-overrides".to_string(),
            Some(serde_json::json!({
                "scope_type": "tenant", "scope_id": SOLO_TENANT,
                "dimension": "storage_bytes", "limit_value": 1024,
                "reason": "test override"
            })),
        ),
    ] {
        let (status, json) = send(&app, method, &uri, body).await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{method} {uri}: {json}");
        assert_eq!(json["error"], "feature_disabled", "{method} {uri}: {json}");
    }

    let (status, json) = send(
        &app,
        "GET",
        &format!("/api/v1/spaces/{SOLO_SPACE}/usage"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{json}");
    assert!(json["usage"]["space_id"].is_string(), "{json}");

    let (status, json) = send(&app, "GET", "/api/v1/admin/quotas", None).await;
    assert_eq!(status, StatusCode::OK, "{json}");
    assert!(json["quotas"].is_array(), "{json}");
}

#[tokio::test]
async fn solo_can_opt_in_to_quota_management() {
    let temp = tempfile::tempdir().unwrap();
    let mut config = sqlite_config(&temp.path().join("solo-opt-in.sqlite"));
    // Advanced solo: explicitly opt in instead of the optional enterprise
    // wallet/entitlement path.
    config.features.quotas = Some(true);
    let app = open_sqlite_app(config).await;

    let (status, json) = send(
        &app,
        "PUT",
        &format!("/api/v1/spaces/{SOLO_SPACE}/quota"),
        Some(serde_json::json!({"storage_bytes": 1_000_000, "enforcement": "soft"})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{json}");
    assert_eq!(json["quota"]["storage_bytes"], 1_000_000);

    let (status, json) = send(
        &app,
        "POST",
        "/api/v1/admin/quotas",
        Some(serde_json::json!({
            "scope_type": "user", "scope_id": "local", "period": "month",
            "limit_tokens": 1000
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{json}");
    let quota_id = json["quota"]["quota_id"].as_str().unwrap().to_string();

    let (status, json) = send(
        &app,
        "POST",
        "/api/v1/admin/quota-overrides",
        Some(serde_json::json!({
            "scope_type": "tenant", "scope_id": SOLO_TENANT,
            "dimension": "storage_bytes", "limit_value": 5_000_000,
            "reason": "opt-in solo override", "expires_at": null
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{json}");
    assert_eq!(json["limit_value"], 5_000_000);

    let (status, json) = send(
        &app,
        "DELETE",
        &format!("/api/v1/admin/quotas/{quota_id}"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{json}");
    assert_eq!(json["removed"], true);
}

#[tokio::test]
async fn normalized_solo_config_creates_notes_spaces_and_pages() {
    let temp = tempfile::tempdir().unwrap();
    let sqlite_path = temp.path().join("normalized.sqlite");
    let config_path = temp.path().join("lkb.toml");
    let sqlite_literal = sqlite_path.to_string_lossy().replace('\\', "/");
    std::fs::write(
        &config_path,
        format!(
            "profile = \"solo\"\n[storage]\nsqlite_path = '{sqlite_literal}'\n[auth]\nmode = \"none\"\n"
        ),
    )
    .unwrap();
    let config = Config::load(&config_path).unwrap();
    assert!(config.spaces.allow_user_creation);
    assert!(!config.groups.allow_user_creation);
    assert_eq!(config.storage.tenant_id, SOLO_TENANT);
    assert_eq!(config.storage.default_space, SOLO_SPACE);

    let store = store::open(&config).await.unwrap();
    store::bootstrap_space(&store, &config).await.unwrap();
    let app = router(AppState::new(
        store.clone(),
        Arc::new(config.clone()),
        Arc::new(ProviderSet::with_none()),
    ));

    let (status, json) = send(
        &app,
        "PUT",
        &format!("/api/v1/pages/{SOLO_SPACE}/welcome"),
        Some(serde_json::json!({"markdown": "# Welcome\n\nsolo note", "title": "Welcome"})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{json}");
    assert_eq!(json["space"], SOLO_SPACE);

    let (status, json) = send(
        &app,
        "POST",
        "/api/v1/spaces",
        Some(serde_json::json!({"name": "My notes"})),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{json}");
    let space = json["space"]["space_id"].as_str().unwrap().to_string();
    assert_eq!(json["space"]["owner_id"], "local");

    let (status, json) = send(
        &app,
        "PUT",
        &format!("/api/v1/pages/{space}/daily"),
        Some(serde_json::json!({"markdown": "# Daily\n\npersonal note", "title": "Daily"})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{json}");
    assert_eq!(json["space"], space);
}

#[tokio::test]
async fn team_deployment_keeps_quota_management_enabled_by_default() {
    if std::env::var("LKB_TEST_PG_URL")
        .map(|value| value.trim().is_empty())
        .unwrap_or(true)
    {
        eprintln!(
            "SKIP team quota router test: LKB_TEST_PG_URL is not set; \
             refusing to pretend it passed"
        );
        return;
    }
    let tenant = format!("t_q_{}", random_hex(4));
    let space_id = format!("s_q_{}", random_hex(4));
    let mut config = Config {
        profile: Some(ProfileName::Team),
        ..Default::default()
    };
    config.storage.backend = Backend::Postgres;
    config.storage.url_env = "LKB_TEST_PG_URL".to_string();
    config.storage.tenant_id = tenant.clone();
    config.storage.default_space = space_id.clone();
    config.deployment.mode = Some(DeploymentMode::Team);
    config.auth.mode = AuthMode::None;
    assert!(config.feature_enabled(lkb::config::Feature::Quotas));

    let store = store::open(&config).await.unwrap();
    store
        .ensure_space(&SpaceRecord::new(
            &tenant,
            &space_id,
            "Team notes",
            &Acl::allow_all(),
            "local",
            &now_rfc3339(),
        ))
        .await
        .unwrap();
    let app = router(AppState::new(
        store.clone(),
        Arc::new(config.clone()),
        Arc::new(ProviderSet::with_none()),
    ));

    let (status, json) = send(
        &app,
        "PUT",
        &format!("/api/v1/spaces/{space_id}/quota"),
        Some(serde_json::json!({"storage_bytes": 10 * 1024 * 1024 * 1024i64})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{json}");

    let (status, json) = send(
        &app,
        "POST",
        "/api/v1/admin/quotas",
        Some(serde_json::json!({
            "scope_type": "tenant", "scope_id": tenant, "period": "month",
            "limit_tokens": 10_000_000
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{json}");
    let quota_id = json["quota"]["quota_id"].as_str().unwrap().to_string();

    let (status, json) = send(
        &app,
        "POST",
        "/api/v1/admin/quota-overrides",
        Some(serde_json::json!({
            "scope_type": "space", "scope_id": space_id,
            "dimension": "documents", "limit_value": 100_000,
            "reason": "team default contract"
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{json}");

    let (status, json) = send(
        &app,
        "DELETE",
        &format!("/api/v1/admin/quotas/{quota_id}"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{json}");
    assert_eq!(json["removed"], true);
}
