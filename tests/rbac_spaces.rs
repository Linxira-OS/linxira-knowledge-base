// SPDX-License-Identifier: MIT OR Apache-2.0
use axum::body::Body;
use axum::http::{header, Request, StatusCode};
use lkb::config::{AuthMode, Config};
use lkb::governance::{SpaceRole, UserRole};
use lkb::providers::ProviderSet;
use lkb::store::{self, SessionRecord, Store, TokenRecord};
use lkb::util::{now_rfc3339, random_hex, sha256_hex_str};
use lkb::web::{router, AppState};
use std::path::Path;
use std::sync::Arc;
use tower::ServiceExt;

fn test_config(path: &Path) -> Config {
    let mut config = Config::default();
    config.storage.sqlite_path = path.to_path_buf();
    config.auth.mode = AuthMode::None;
    config.spaces.allow_user_creation = true;
    config.groups.allow_user_creation = true;
    config
}

async fn open_store(config: &Config) -> Arc<dyn Store> {
    let store = store::open(config).await.unwrap();
    store::bootstrap_space(&store, config).await.unwrap();
    store
}

async fn add_user(store: &Arc<dyn Store>, tenant: &str, name: &str, role: UserRole) {
    store
        .add_user_with_role(tenant, name, "not-a-real-argon2-hash", role)
        .await
        .unwrap();
}

async fn add_token(store: &Arc<dyn Store>, tenant: &str, username: &str, scope: &str) -> String {
    let user = store.get_user(tenant, username).await.unwrap().unwrap();
    let raw = format!("lkb_{}", random_hex(16));
    store
        .create_token(&TokenRecord {
            token_hash: sha256_hex_str(&raw),
            token_id: format!("tok_{}", random_hex(6)),
            tenant_id: tenant.to_string(),
            user_id: user.user_id,
            scope: lkb::governance::TokenScope::parse(scope).unwrap(),
            kind: lkb::governance::PrincipalKind::Human,
            label: "test".into(),
            created_at: now_rfc3339(),
            expires_at: None,
            revoked_at: None,
        })
        .await
        .unwrap();
    raw
}

async fn body_json(response: axum::response::Response) -> serde_json::Value {
    let bytes = axum::body::to_bytes(response.into_body(), 1_000_000)
        .await
        .unwrap();
    serde_json::from_slice(&bytes).unwrap()
}

async fn oneshot_json(
    app: &axum::Router,
    method: &str,
    uri: &str,
    token: Option<&str>,
    body: Option<serde_json::Value>,
) -> (StatusCode, serde_json::Value) {
    let mut builder = Request::builder()
        .method(method)
        .uri(uri)
        .header(header::HOST, "localhost")
        .header(header::CONTENT_TYPE, "application/json");
    if let Some(token) = token {
        builder = builder.header(header::AUTHORIZATION, format!("Bearer {token}"));
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

async fn create_group(app: &axum::Router, token: &str, name: &str) -> String {
    let (status, json) = oneshot_json(
        app,
        "POST",
        "/api/v1/groups",
        Some(token),
        Some(serde_json::json!({"name": name})),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{json}");
    json["group"]["group_id"].as_str().unwrap().to_string()
}

async fn create_space(
    app: &axum::Router,
    token: &str,
    owner_type: Option<&str>,
    owner_id: Option<&str>,
    name: &str,
) -> String {
    let mut body = serde_json::json!({"name": name});
    if let Some(owner_type) = owner_type {
        body["owner_type"] = serde_json::json!(owner_type);
    }
    if let Some(owner_id) = owner_id {
        body["owner_id"] = serde_json::json!(owner_id);
    }
    let (status, json) = oneshot_json(app, "POST", "/api/v1/spaces", Some(token), Some(body)).await;
    assert_eq!(status, StatusCode::CREATED, "{json}");
    json["space"]["space_id"].as_str().unwrap().to_string()
}

#[tokio::test]
async fn group_and_explicit_roles_follow_precedence() {
    let temp = tempfile::tempdir().unwrap();
    let config = test_config(&temp.path().join("rbac.sqlite"));
    let store = open_store(&config).await;
    let tenant = &config.storage.tenant_id;
    add_user(&store, tenant, "owner", UserRole::Owner).await;
    add_user(&store, tenant, "alice", UserRole::User).await;
    add_user(&store, tenant, "bob", UserRole::User).await;
    let owner_token = add_token(&store, tenant, "owner", "admin").await;
    let alice_token = add_token(&store, tenant, "alice", "write").await;
    let bob_token = add_token(&store, tenant, "bob", "write").await;

    let app = router(AppState::new(
        store.clone(),
        Arc::new(config.clone()),
        Arc::new(ProviderSet::with_none()),
    ));
    let group = create_group(&app, &alice_token, "eng").await;
    let (status, json) = oneshot_json(
        &app,
        "POST",
        &format!("/api/v1/groups/{group}/members"),
        Some(&alice_token),
        Some(serde_json::json!({"principal_id": store.get_user(tenant, "bob").await.unwrap().unwrap().user_id})),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{json}");

    let space = create_space(
        &app,
        &owner_token,
        Some("group"),
        Some(&group),
        "group space",
    )
    .await;
    let (_, alice_spaces) =
        oneshot_json(&app, "GET", "/api/v1/spaces", Some(&alice_token), None).await;
    let alice_role = alice_spaces["spaces"]
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item["space_id"] == space)
        .expect("alice can see the group space")["role"]
        .as_str()
        .unwrap()
        .to_string();
    assert_eq!(alice_role, "space_admin");

    let (_, bob_spaces) = oneshot_json(&app, "GET", "/api/v1/spaces", Some(&bob_token), None).await;
    let bob_role = bob_spaces["spaces"]
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item["space_id"] == space)
        .expect("bob can see the group space")["role"]
        .as_str()
        .unwrap()
        .to_string();
    assert_eq!(bob_role, "viewer");

    // Explicit member role wins over the group-derived viewer.
    let bob_user_id = store
        .get_user(tenant, "bob")
        .await
        .unwrap()
        .unwrap()
        .user_id;
    let (status, _) = oneshot_json(
        &app,
        "POST",
        &format!("/api/v1/spaces/{space}/members"),
        Some(&owner_token),
        Some(serde_json::json!({
            "principal_type": "user",
            "principal_id": bob_user_id,
            "role": "editor",
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);

    let space_record = store.get_space(tenant, &space).await.unwrap().unwrap();
    let bob_principal = lkb::governance::Principal::local_user_with_role(
        &bob_user_id,
        tenant,
        vec![space.clone()],
        Vec::new(),
        UserRole::User,
    );
    let access = lkb::access::effective_space_role(&store, &bob_principal, &space_record)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(access.role, SpaceRole::Editor);
    let usage = lkb::quota::refresh(&store, tenant, &space).await.unwrap();
    assert_eq!(
        usage.members, 2,
        "group membership must be expanded in usage"
    );
}

#[tokio::test]
async fn member_isolation_and_last_owner_protection() {
    let temp = tempfile::tempdir().unwrap();
    let config = test_config(&temp.path().join("members.sqlite"));
    let store = open_store(&config).await;
    let tenant = &config.storage.tenant_id;
    add_user(&store, tenant, "owner", UserRole::Owner).await;
    add_user(&store, tenant, "carol", UserRole::User).await;
    let owner_token = add_token(&store, tenant, "owner", "admin").await;
    let carol_token = add_token(&store, tenant, "carol", "write").await;
    let app = router(AppState::new(
        store.clone(),
        Arc::new(config.clone()),
        Arc::new(ProviderSet::with_none()),
    ));
    let space = create_space(&app, &owner_token, None, None, "private space").await;
    let carol_id = store
        .get_user(tenant, "carol")
        .await
        .unwrap()
        .unwrap()
        .user_id;

    let (status, _) = oneshot_json(
        &app,
        "GET",
        &format!("/api/v1/spaces/{space}"),
        Some(&carol_token),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);

    let (status, _) = oneshot_json(
        &app,
        "POST",
        &format!("/api/v1/spaces/{space}/members"),
        Some(&owner_token),
        Some(serde_json::json!({
            "principal_type": "user",
            "principal_id": carol_id,
            "role": "viewer",
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let (status, _) = oneshot_json(
        &app,
        "GET",
        &format!("/api/v1/spaces/{space}"),
        Some(&carol_token),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let owner_id = store
        .get_user(tenant, "owner")
        .await
        .unwrap()
        .unwrap()
        .user_id;
    let (status, json) = oneshot_json(
        &app,
        "DELETE",
        &format!("/api/v1/spaces/{space}/members/user/{owner_id}"),
        Some(&owner_token),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "{json}");
    assert_eq!(json["error"], "last_space_owner_protected");
}

#[tokio::test]
async fn scope_401_and_403_are_audited() {
    let temp = tempfile::tempdir().unwrap();
    let mut config = test_config(&temp.path().join("scope.sqlite"));
    config.auth.mode = AuthMode::Session;
    let store = open_store(&config).await;
    let tenant = &config.storage.tenant_id;
    add_user(&store, tenant, "reader", UserRole::User).await;
    add_user(&store, tenant, "owner", UserRole::Owner).await;
    let reader_token = add_token(&store, tenant, "reader", "read").await;
    let owner_token = add_token(&store, tenant, "owner", "admin").await;
    let app = router(AppState::new(
        store.clone(),
        Arc::new(config.clone()),
        Arc::new(ProviderSet::with_none()),
    ));

    let (status, _) = oneshot_json(&app, "GET", "/api/v1/spaces", None, None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    let (status, json) = oneshot_json(
        &app,
        "POST",
        "/api/v1/spaces",
        Some(&reader_token),
        Some(serde_json::json!({"name": "nope"})),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{json}");
    let (status, json) = oneshot_json(
        &app,
        "GET",
        "/api/v1/admin/audit?action=space.create",
        Some(&owner_token),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{json}");
    assert!(!json["events"].as_array().unwrap().is_empty(), "{json}");
}

#[tokio::test]
async fn space_crud_quota_and_audit_smoke() {
    let temp = tempfile::tempdir().unwrap();
    let config = test_config(&temp.path().join("crud.sqlite"));
    let store = open_store(&config).await;
    let tenant = &config.storage.tenant_id;
    add_user(&store, tenant, "owner", UserRole::Owner).await;
    add_user(&store, tenant, "target", UserRole::User).await;
    let owner_token = add_token(&store, tenant, "owner", "admin").await;
    let app = router(AppState::new(
        store.clone(),
        Arc::new(config.clone()),
        Arc::new(ProviderSet::with_none()),
    ));
    let space = create_space(&app, &owner_token, None, None, "ops").await;
    let (status, json) = oneshot_json(
        &app,
        "PATCH",
        &format!("/api/v1/spaces/{space}"),
        Some(&owner_token),
        Some(serde_json::json!({"name": "ops-renamed", "visibility": "internal"})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{json}");
    assert_eq!(json["space"]["name"], "ops-renamed");

    let (status, _) = oneshot_json(
        &app,
        "POST",
        &format!("/api/v1/spaces/{space}/archive"),
        Some(&owner_token),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let (status, _) = oneshot_json(
        &app,
        "POST",
        &format!("/api/v1/spaces/{space}/restore"),
        Some(&owner_token),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let (status, json) = oneshot_json(
        &app,
        "PUT",
        &format!("/api/v1/spaces/{space}/quota"),
        Some(&owner_token),
        Some(serde_json::json!({"storage_bytes": 8})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{json}");
    let record = store.get_space(tenant, &space).await.unwrap().unwrap();
    let err = lkb::quota::reserve(
        &store,
        &record,
        &lkb::quota::QuotaDelta {
            source_bytes: 64,
            index_bytes: 0,
            history_bytes: 0,
            documents: 1,
            chunks: 1,
        },
    )
    .await
    .unwrap_err();
    assert_eq!(err.status, StatusCode::PAYLOAD_TOO_LARGE);
    assert_eq!(err.code, "space_quota_exceeded");

    let form = format!(
        "title=Quota&markdown={}&csrf_token=csrf-token",
        "x".repeat(128)
    );
    let request = Request::builder()
        .method("POST")
        .uri(format!("/w/{space}/quota-page"))
        .header(header::HOST, "localhost")
        .header(header::ORIGIN, "http://localhost")
        .header(header::AUTHORIZATION, format!("Bearer {owner_token}"))
        .header(header::COOKIE, "lkb_csrf=csrf-token")
        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
        .body(Body::from(form))
        .unwrap();
    let response = app.clone().oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::PAYLOAD_TOO_LARGE);
    let json = body_json(response).await;
    assert_eq!(json["error"], "space_quota_exceeded");

    let target_id = store
        .get_user(tenant, "target")
        .await
        .unwrap()
        .unwrap()
        .user_id;
    let (status, json) = oneshot_json(
        &app,
        "POST",
        &format!("/api/v1/spaces/{space}/transfer"),
        Some(&owner_token),
        Some(serde_json::json!({"owner_type": "user", "owner_id": target_id})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{json}");
    assert_eq!(json["space"]["owner_id"], target_id);

    let (status, json) = oneshot_json(
        &app,
        "GET",
        "/api/v1/admin/audit?action=ownership.transferred",
        Some(&owner_token),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{json}");
    assert!(!json["events"].as_array().unwrap().is_empty(), "{json}");
}

#[tokio::test]
async fn session_write_requires_csrf_header() {
    let temp = tempfile::tempdir().unwrap();
    let mut config = test_config(&temp.path().join("session.sqlite"));
    config.auth.mode = AuthMode::Session;
    let store = open_store(&config).await;
    let tenant = &config.storage.tenant_id;
    add_user(&store, tenant, "owner", UserRole::Owner).await;
    let user = store.get_user(tenant, "owner").await.unwrap().unwrap();
    let raw_session = random_hex(16);
    store
        .create_session(&SessionRecord {
            session_hash: sha256_hex_str(&raw_session),
            tenant_id: tenant.clone(),
            user_id: user.user_id,
            created_at: now_rfc3339(),
            expires_at: (chrono::Utc::now() + chrono::Duration::hours(1))
                .to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
        })
        .await
        .unwrap();
    let app = router(AppState::new(
        store.clone(),
        Arc::new(config.clone()),
        Arc::new(ProviderSet::with_none()),
    ));
    let cookie = format!("{}={raw_session}; lkb_csrf=token", config.auth.cookie_name);

    let without = Request::builder()
        .method("POST")
        .uri("/api/v1/spaces")
        .header(header::HOST, "localhost")
        .header(header::COOKIE, &cookie)
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(r#"{"name":"csrf"}"#))
        .unwrap();
    let response = app.clone().oneshot(without).await.unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);

    let with = Request::builder()
        .method("POST")
        .uri("/api/v1/spaces")
        .header(header::HOST, "localhost")
        .header(header::COOKIE, &cookie)
        .header("x-csrf-token", "token")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(r#"{"name":"csrf"}"#))
        .unwrap();
    let response = app.oneshot(with).await.unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);
}
