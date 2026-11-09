// SPDX-License-Identifier: MIT OR Apache-2.0
//! Per-principal private notes spaces: idempotent ensure, owner-only ACL and
//! isolation from self-service space policy.

use axum::body::Body;
use axum::http::{header, Request, StatusCode};
use lkb::config::{AuthMode, Backend, Config, DeploymentMode, ProfileName};
use lkb::governance::{Acl, PrincipalKind, TokenScope, UserRole, SOLO_SPACE};
use lkb::providers::ProviderSet;
use lkb::store::{self, AuditFilter, SpaceRecord, Store, TokenRecord};
use lkb::util::{now_rfc3339, random_hex, sha256_hex_str};
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

fn pg_team_config() -> Config {
    let mut config = Config {
        profile: Some(ProfileName::Team),
        ..Default::default()
    };
    config.storage.backend = Backend::Postgres;
    config.storage.url_env = "LKB_TEST_PG_URL".to_string();
    config.storage.tenant_id = format!("t_notes_{}", random_hex(4));
    config.storage.default_space = format!("s_notes_{}", random_hex(4));
    config.deployment.mode = Some(DeploymentMode::Team);
    config.auth.mode = AuthMode::None;
    config
}

async fn add_user_token(
    store: &Arc<dyn Store>,
    tenant: &str,
    username: &str,
    role: UserRole,
    scope: TokenScope,
) -> (String, String) {
    let user = store
        .add_user_with_role(tenant, username, "not-a-real-argon2-hash", role)
        .await
        .unwrap();
    let raw = format!("lkb_{}", random_hex(16));
    store
        .create_token(&TokenRecord {
            token_hash: sha256_hex_str(&raw),
            token_id: format!("tok_{}", random_hex(6)),
            tenant_id: tenant.to_string(),
            user_id: user.user_id.clone(),
            scope,
            kind: PrincipalKind::Human,
            label: "notes-test".into(),
            created_at: now_rfc3339(),
            expires_at: None,
            revoked_at: None,
        })
        .await
        .unwrap();
    (user.user_id, raw)
}

async fn send(
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
    let bytes = axum::body::to_bytes(response.into_body(), 1_000_000)
        .await
        .unwrap();
    let json = serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null);
    (status, json)
}

async fn notes_post(app: &axum::Router, token: &str) -> (StatusCode, serde_json::Value) {
    send(app, "POST", "/api/v1/notes/space", Some(token), None).await
}

async fn assert_notes_space_immutable(
    app: &axum::Router,
    token: Option<&str>,
    space_id: &str,
    transfer_owner: &str,
) {
    for (method, uri, body) in [
        ("DELETE", format!("/api/v1/spaces/{space_id}"), None),
        ("POST", format!("/api/v1/spaces/{space_id}/archive"), None),
        ("POST", format!("/api/v1/spaces/{space_id}/restore"), None),
        (
            "POST",
            format!("/api/v1/spaces/{space_id}/transfer"),
            Some(serde_json::json!({"owner_type": "user", "owner_id": transfer_owner})),
        ),
    ] {
        let (status, json) = send(app, method, &uri, token, body).await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{method} {uri}: {json}");
        assert_eq!(
            json["error"], "notes_space_immutable",
            "{method} {uri}: {json}"
        );
    }
}

#[tokio::test]
async fn solo_notes_space_is_idempotent_and_bypasses_space_policy() {
    let temp = tempfile::tempdir().unwrap();
    let mut config = sqlite_config(&temp.path().join("solo-notes.sqlite"));
    // A notes space is a system capability, not self-service: both gates are
    // deliberately hostile here.
    config.spaces.allow_user_creation = false;
    config.spaces.max_per_user = 1;
    let store = store::open(&config).await.unwrap();
    store::bootstrap_space(&store, &config).await.unwrap();
    let app = router(AppState::new(
        store.clone(),
        Arc::new(config.clone()),
        Arc::new(ProviderSet::with_none()),
    ));

    let (status, json) = send(&app, "GET", "/api/v1/notes/space", None, None).await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{json}");
    assert_eq!(json["error"], "not_found");

    let (status, json) = send(&app, "POST", "/api/v1/notes/space", None, None).await;
    assert_eq!(status, StatusCode::OK, "{json}");
    let first = json["space"].clone();
    assert_eq!(first["space_id"], "notes_local");
    assert_eq!(first["name"], "My notes");
    assert_eq!(first["visibility"], "private");
    assert_eq!(first["owner_type"], "user");
    assert_eq!(first["owner_id"], "local");
    assert_eq!(first["status"], "active");

    let (status, json) = send(&app, "POST", "/api/v1/notes/space", None, None).await;
    assert_eq!(status, StatusCode::OK, "{json}");
    assert_eq!(json["space"]["space_id"], first["space_id"]);
    assert_eq!(json["space"]["created_at"], first["created_at"]);

    let (status, json) = send(&app, "GET", "/api/v1/notes/space", None, None).await;
    assert_eq!(status, StatusCode::OK, "{json}");
    assert_eq!(json["space"]["space_id"], first["space_id"]);

    let (status, json) = send(
        &app,
        "PATCH",
        "/api/v1/spaces/notes_local",
        None,
        Some(serde_json::json!({"name": "Hijacked", "settings": {}})),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{json}");

    // The solo local principal is allow_all but its own notes pages are
    // owner-only; visibility must still include the principal's `|u:local|`.
    let note_uri = format!(
        "/api/v1/pages/{}/hello-note?lang=zh",
        first["space_id"].as_str().unwrap()
    );
    let (status, json) = send(
        &app,
        "PUT",
        &note_uri,
        None,
        Some(serde_json::json!({"markdown": "# Hello\n\nnote", "title": "Hello", "lang": "zh"})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{json}");
    let (status, json) = send(&app, "GET", &note_uri, None, None).await;
    assert_eq!(status, StatusCode::OK, "{json}");
    assert!(json["markdown"].as_str().unwrap().contains("note"));
    let list_uri = format!(
        "/api/v1/pages?space={}&lang=zh",
        first["space_id"].as_str().unwrap()
    );
    let (status, json) = send(&app, "GET", &list_uri, None, None).await;
    assert_eq!(status, StatusCode::OK, "{json}");
    assert_eq!(json["pages"].as_array().unwrap().len(), 1, "{json}");

    let target = store
        .add_user_with_role(&config.storage.tenant_id, "target", "x", UserRole::User)
        .await
        .unwrap();
    assert_notes_space_immutable(&app, None, "notes_local", &target.user_id).await;

    // Immutability attempts must not affect note content.
    let (status, json) = send(&app, "GET", &note_uri, None, None).await;
    assert_eq!(status, StatusCode::OK, "{json}");
    let (status, json) = send(
        &app,
        "PUT",
        &note_uri,
        None,
        Some(serde_json::json!({"markdown": "# Hello\n\nupdated note", "title": "Hello", "lang": "zh"})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{json}");
    assert_eq!(json["version"], 2);

    // Individual note pages remain deletable; the notes space itself stays.
    let (status, json) = send(
        &app,
        "DELETE",
        "/api/v1/pages/notes_local/hello-note?lang=zh",
        None,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{json}");
    let (status, json) = send(&app, "GET", "/api/v1/notes/space", None, None).await;
    assert_eq!(status, StatusCode::OK, "{json}");

    // Ordinary space lifecycle stays available; notes-kind conversion is denied.
    let normal_id = "normal_lifecycle";
    store
        .ensure_space(&SpaceRecord::new(
            &config.storage.tenant_id,
            normal_id,
            "Normal",
            &Acl::allow_all(),
            "local",
            &now_rfc3339(),
        ))
        .await
        .unwrap();
    let (status, json) = send(
        &app,
        "PATCH",
        &format!("/api/v1/spaces/{normal_id}"),
        None,
        Some(serde_json::json!({"settings": {"kind": "notes"}})),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{json}");
    let (status, json) = send(
        &app,
        "PATCH",
        &format!("/api/v1/spaces/{normal_id}"),
        None,
        Some(serde_json::json!({"name": "Normal renamed"})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{json}");
    let (status, json) = send(
        &app,
        "POST",
        &format!("/api/v1/spaces/{normal_id}/archive"),
        None,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{json}");
    let (status, json) = send(
        &app,
        "POST",
        &format!("/api/v1/spaces/{normal_id}/restore"),
        None,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{json}");
    let (status, json) = send(
        &app,
        "POST",
        &format!("/api/v1/spaces/{normal_id}/transfer"),
        None,
        Some(serde_json::json!({"owner_type": "user", "owner_id": target.user_id})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{json}");
    let (status, json) = send(
        &app,
        "DELETE",
        &format!("/api/v1/spaces/{normal_id}"),
        None,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{json}");

    // The default solo space stays untouched and writable.
    let (status, json) = send(
        &app,
        "PUT",
        &format!("/api/v1/pages/{SOLO_SPACE}/welcome"),
        None,
        Some(serde_json::json!({"markdown": "# Welcome\n\nsolo note", "title": "Welcome"})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{json}");
    assert_eq!(json["space"], SOLO_SPACE);

    let audits = store
        .list_audit(&AuditFilter {
            tenant_id: config.storage.tenant_id.clone(),
            action: Some("notes.space.ensure".to_string()),
            limit: 10,
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(audits.len(), 2, "{audits:?}");
    assert!(audits.iter().all(|event| event.result == "allow"));
    assert!(audits
        .iter()
        .any(|event| event.detail_json.contains("\"created\":true")));
    assert!(audits
        .iter()
        .any(|event| event.detail_json.contains("\"created\":false")));
}

async fn multi_user_notes_flow(config: Config) {
    let tenant = config.storage.tenant_id.clone();
    let store = store::open(&config).await.unwrap();
    let (alice_id, alice) =
        add_user_token(&store, &tenant, "alice", UserRole::User, TokenScope::Write).await;
    let (bob_id, bob) =
        add_user_token(&store, &tenant, "bob", UserRole::User, TokenScope::Write).await;
    let (_admin_id, admin) =
        add_user_token(&store, &tenant, "admin", UserRole::Owner, TokenScope::Admin).await;
    let app = router(AppState::new(
        store.clone(),
        Arc::new(config.clone()),
        Arc::new(ProviderSet::with_none()),
    ));

    let (status, json) = send(&app, "GET", "/api/v1/features", Some(&alice), None).await;
    assert_eq!(status, StatusCode::OK, "token auth failed: {json}");

    let (status, json) = notes_post(&app, &alice).await;
    assert_eq!(status, StatusCode::OK, "{json}");
    let alice_notes = json["space"]["space_id"].as_str().unwrap().to_string();
    assert!(alice_notes.starts_with("notes_"), "{alice_notes}");
    assert!(
        alice_notes.contains(&alice_id),
        "{alice_notes} vs {alice_id}"
    );
    assert_eq!(json["space"]["owner_id"], alice_id);

    let (status, json) = notes_post(&app, &bob).await;
    assert_eq!(status, StatusCode::OK, "{json}");
    let bob_notes = json["space"]["space_id"].as_str().unwrap().to_string();
    assert!(bob_notes.starts_with("notes_"), "{bob_notes}");
    assert_ne!(alice_notes, bob_notes);

    // The notes API always resolves the caller's own space.
    let (status, json) = send(&app, "GET", "/api/v1/notes/space", Some(&bob), None).await;
    assert_eq!(status, StatusCode::OK, "{json}");
    assert_eq!(json["space"]["space_id"], bob_notes);
    assert_ne!(json["space"]["space_id"], alice_notes);
    assert_eq!(json["space"]["owner_id"], bob_id);

    let page_uri = format!("/api/v1/pages/{alice_notes}/journal?lang=zh");
    let (status, json) = send(
        &app,
        "PUT",
        &page_uri,
        Some(&alice),
        Some(serde_json::json!({
            "markdown": "# Journal\n\nprivate thoughts",
            "title": "Journal",
            "lang": "zh"
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{json}");

    let (status, json) = send(&app, "GET", &page_uri, Some(&alice), None).await;
    assert_eq!(status, StatusCode::OK, "{json}");
    assert!(json["markdown"]
        .as_str()
        .unwrap()
        .contains("private thoughts"));

    // Another user cannot read the notes page (space scope + ACL).
    let (status, json) = send(&app, "GET", &page_uri, Some(&bob), None).await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{json}");

    // A tenant admin passes the space role check but the owner-only page ACL
    // still hides content, including history markdown.
    let (status, json) = send(&app, "GET", &page_uri, Some(&admin), None).await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{json}");
    let history_uri =
        format!("/api/v1/pages/{alice_notes}/journal/history?lang=zh&include_markdown=true");
    let (status, json) = send(&app, "GET", &history_uri, Some(&admin), None).await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{json}");
    let (status, json) = send(
        &app,
        "PUT",
        &page_uri,
        Some(&admin),
        Some(serde_json::json!({"markdown": "overwrite attempt", "title": "Nope", "lang": "zh"})),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{json}");

    // Admin's own notes space is separate.
    let (status, json) = notes_post(&app, &admin).await;
    assert_eq!(status, StatusCode::OK, "{json}");
    assert_ne!(json["space"]["space_id"], alice_notes);

    // Reserved prefix: normal self-service creation cannot claim notes_*.
    let (status, json) = send(
        &app,
        "POST",
        "/api/v1/spaces",
        Some(&alice),
        Some(serde_json::json!({"space_id": "notes_alice", "name": "collision"})),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{json}");

    assert_notes_space_immutable(&app, Some(&alice), &alice_notes, &bob_id).await;

    // Notes content survives failed lifecycle operations and stays writable.
    let (status, json) = send(&app, "GET", &page_uri, Some(&alice), None).await;
    assert_eq!(status, StatusCode::OK, "{json}");
    let (status, json) = send(
        &app,
        "PUT",
        &page_uri,
        Some(&alice),
        Some(serde_json::json!({
            "markdown": "# Journal\n\nupdated private thoughts",
            "title": "Journal",
            "lang": "zh"
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{json}");
    assert_eq!(json["version"], 2);

    // Ordinary space lifecycle still works end to end.
    let (status, json) = send(
        &app,
        "POST",
        "/api/v1/spaces",
        Some(&alice),
        Some(serde_json::json!({"name": "Fake notes", "settings": {"kind": "notes"}})),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{json}");
    let (status, json) = send(
        &app,
        "POST",
        "/api/v1/spaces",
        Some(&alice),
        Some(serde_json::json!({"name": "Shared space"})),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{json}");
    let normal_id = json["space"]["space_id"].as_str().unwrap().to_string();
    assert!(!normal_id.starts_with("notes_"));
    let (status, json) = send(
        &app,
        "PATCH",
        &format!("/api/v1/spaces/{normal_id}"),
        Some(&alice),
        Some(serde_json::json!({"settings": {"kind": "notes"}})),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{json}");
    let (status, json) = send(
        &app,
        "PATCH",
        &format!("/api/v1/spaces/{normal_id}"),
        Some(&alice),
        Some(serde_json::json!({"name": "Shared renamed"})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{json}");
    let (status, json) = send(
        &app,
        "POST",
        &format!("/api/v1/spaces/{normal_id}/archive"),
        Some(&alice),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{json}");
    let (status, json) = send(
        &app,
        "POST",
        &format!("/api/v1/spaces/{normal_id}/restore"),
        Some(&alice),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{json}");
    let (status, json) = send(
        &app,
        "POST",
        &format!("/api/v1/spaces/{normal_id}/transfer"),
        Some(&alice),
        Some(serde_json::json!({"owner_type": "user", "owner_id": bob_id})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{json}");
    let (status, json) = send(
        &app,
        "DELETE",
        &format!("/api/v1/spaces/{normal_id}"),
        Some(&bob),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{json}");
}

#[tokio::test]
async fn multi_user_notes_spaces_are_owner_scoped_and_hidden_from_admin() {
    let temp = tempfile::tempdir().unwrap();
    multi_user_notes_flow(sqlite_config(&temp.path().join("multi.sqlite"))).await;
}

#[tokio::test]
async fn team_notes_spaces_are_owner_scoped_and_hidden_from_admin() {
    if std::env::var("LKB_TEST_PG_URL")
        .map(|value| value.trim().is_empty())
        .unwrap_or(true)
    {
        eprintln!(
            "SKIP team notes test: LKB_TEST_PG_URL is not set; \
             refusing to pretend it passed"
        );
        return;
    }
    multi_user_notes_flow(pg_team_config()).await;
}
