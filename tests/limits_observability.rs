// SPDX-License-Identifier: MIT OR Apache-2.0
use assert_cmd::Command;
use axum::body::Body;
use axum::http::{header, Request, StatusCode};
use lkb::config::{AuthMode, Config};
use lkb::governance::UserRole;
use lkb::providers::ProviderSet;
use lkb::store::{self, QuotaRecord, QuotaScope, Store, TokenRecord};
use lkb::util::{now_rfc3339, random_hex, sha256_hex_str};
use lkb::web::{router, AppState};
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;
use tower::ServiceExt;

fn test_config(path: &Path) -> Config {
    let mut config = Config::default();
    config.storage.sqlite_path = path.to_path_buf();
    config.auth.mode = AuthMode::None;
    config.spaces.allow_user_creation = true;
    config.groups.allow_user_creation = true;
    config.limits.qps_ip = 100_000;
    config.limits.qps_instance = 100_000;
    config.limits.qps_tenant = 100_000;
    config.limits.qps_user = 100_000;
    config.limits.qps_space = 100_000;
    config
}

async fn open_store(config: &Config) -> Arc<dyn Store> {
    let store = store::open(config).await.unwrap();
    store::bootstrap_space(&store, config).await.unwrap();
    store
}

async fn add_user_token(store: &Arc<dyn Store>, tenant: &str, name: &str) -> String {
    store
        .add_user_with_role(tenant, name, "not-a-real-argon2-hash", UserRole::User)
        .await
        .unwrap();
    let user = store.get_user(tenant, name).await.unwrap().unwrap();
    let raw = format!("lkb_{}", random_hex(16));
    store
        .create_token(&TokenRecord {
            token_hash: sha256_hex_str(&raw),
            token_id: format!("tok_{}", random_hex(6)),
            tenant_id: tenant.to_string(),
            user_id: user.user_id,
            scope: lkb::governance::TokenScope::Write,
            kind: lkb::governance::PrincipalKind::Human,
            label: "limits".into(),
            created_at: now_rfc3339(),
            expires_at: None,
            revoked_at: None,
        })
        .await
        .unwrap();
    raw
}

async fn get(app: &axum::Router, uri: &str, token: Option<&str>) -> (StatusCode, Option<String>) {
    let mut builder = Request::builder()
        .uri(uri)
        .method("GET")
        .header(header::HOST, "localhost");
    if let Some(token) = token {
        builder = builder.header(header::AUTHORIZATION, format!("Bearer {token}"));
    }
    let response = app
        .clone()
        .oneshot(builder.body(Body::empty()).unwrap())
        .await
        .unwrap();
    let retry = response
        .headers()
        .get(header::RETRY_AFTER)
        .and_then(|value| value.to_str().ok())
        .map(|value| value.to_string());
    (response.status(), retry)
}

#[tokio::test]
async fn concurrent_queries_rate_limit_and_input_validation() {
    let temp = tempfile::tempdir().unwrap();
    let mut config = test_config(&temp.path().join("limits.sqlite"));
    config.limits.qps_token = 2;
    let store = open_store(&config).await;
    let token = add_user_token(&store, &config.storage.tenant_id, "agent").await;
    let app = router(AppState::new(
        store.clone(),
        Arc::new(config.clone()),
        Arc::new(ProviderSet::with_none()),
    ));

    let mut tasks = tokio::task::JoinSet::new();
    for _ in 0..100 {
        let app = app.clone();
        let token = token.clone();
        tasks.spawn(async move { get(&app, "/api/v1/spaces", Some(&token)).await.0 });
    }
    let mut ok = 0;
    let mut limited = 0;
    while let Some(status) = tasks.join_next().await {
        let status = status.unwrap();
        assert!(!status.is_server_error(), "unexpected 5xx: {status}");
        if status == StatusCode::OK {
            ok += 1;
        }
        if status == StatusCode::TOO_MANY_REQUESTS {
            limited += 1;
        }
    }
    assert!(ok > 0, "at least one request should succeed");
    assert!(limited > 0, "token QPS must reject some requests");

    let mut retry_seen = false;
    let mut rate_seen = false;
    for _ in 0..5 {
        let (status, retry) = get(&app, "/api/v1/spaces", Some(&token)).await;
        if status == StatusCode::TOO_MANY_REQUESTS {
            rate_seen = true;
            retry_seen = retry.is_some();
        }
    }
    assert!(rate_seen, "expected a 429 from a single token over QPS");
    assert!(retry_seen, "429 must carry Retry-After");

    let revoked = store
        .get_token(&sha256_hex_str(&token))
        .await
        .unwrap()
        .unwrap();
    store.revoke_token(&revoked.token_id).await.unwrap();
    let (status, _) = get(&app, "/api/v1/spaces", Some(&token)).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    let long = "x".repeat(config.limits.max_query_chars + 10);
    let (status, _) = get(&app, &format!("/api/search?q={long}"), None).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let (status, _) = get(&app, "/api/search?q=x&k=1000", None).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn quota_reservation_does_not_oversell_under_concurrency() {
    let temp = tempfile::tempdir().unwrap();
    let config = test_config(&temp.path().join("oversell.sqlite"));
    let store = open_store(&config).await;
    let tenant = &config.storage.tenant_id;
    let space = store
        .get_space(tenant, &config.storage.default_space)
        .await
        .unwrap()
        .unwrap();
    store
        .upsert_quota(&QuotaRecord {
            tenant_id: tenant.clone(),
            scope_type: QuotaScope::Space,
            scope_id: space.space_id.clone(),
            storage_bytes: Some(100),
            documents: None,
            chunks: None,
            members: None,
            spaces: None,
            enforcement: "hard".into(),
            updated_by: "test".into(),
            updated_at: now_rfc3339(),
        })
        .await
        .unwrap();

    let mut tasks = tokio::task::JoinSet::new();
    for _ in 0..16 {
        let store = store.clone();
        let space = space.clone();
        tasks.spawn(async move {
            let _guard = lkb::quota::write_guard().await;
            lkb::quota::reserve(
                &store,
                &space,
                &lkb::quota::QuotaDelta {
                    source_bytes: 80,
                    index_bytes: 0,
                    history_bytes: 0,
                    documents: 1,
                    chunks: 1,
                },
            )
            .await
            .is_ok()
        });
    }
    let mut successes = 0;
    while let Some(result) = tasks.join_next().await {
        if result.unwrap() {
            successes += 1;
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    }
    assert!(
        successes <= 1,
        "quota oversold: {successes} reservations for 100 bytes"
    );
}

#[test]
fn cli_json_log_has_required_fields_and_no_secrets() {
    let temp = tempfile::tempdir().unwrap();
    let output = Command::cargo_bin("lkb")
        .unwrap()
        .args(["config", "check"])
        .env("LKB_LOG_FORMAT", "json")
        .env("LKB_PROFILE", "solo")
        .env("LKB_SQLITE_PATH", temp.path().join("log.sqlite"))
        .output()
        .unwrap();
    assert!(output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    let line = stderr
        .lines()
        .rev()
        .find_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
        .expect("json log line");
    for key in [
        "timestamp",
        "level",
        "service",
        "request_id",
        "trace_id",
        "degraded_codes",
        "concurrency",
        "latency_ms",
    ] {
        assert!(line.get(key).is_some(), "missing log field {key}: {line}");
    }
    assert!(line["degraded_codes"].is_array());
    let text = line.to_string();
    for forbidden in [
        "api_key",
        "authorization",
        "query_text",
        "password",
        "cookie",
    ] {
        assert!(!text.contains(forbidden), "log leaked {forbidden}: {text}");
    }
}
