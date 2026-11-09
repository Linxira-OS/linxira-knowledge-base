use axum::body::Body;
use axum::extract::ConnectInfo;
use axum::http::{header, Request, StatusCode};
use lkb::config::{AuthMode, Config};
use lkb::governance::UserRole;
use lkb::providers::ProviderSet;
use lkb::store::{self, AuditFilter, Store};
use lkb::web::{router, AppState};
use std::net::SocketAddr;
use std::sync::Arc;
use tower::ServiceExt;

fn test_config(path: &std::path::Path) -> Config {
    let mut config = Config::default();
    config.storage.sqlite_path = path.to_path_buf();
    config.storage.tenant_id = "t_local".into();
    config.storage.default_space = "s_default".into();
    config.auth.mode = AuthMode::Session;
    config.sso.mode = "trusted_header".into();
    config.sso.trusted_proxies = vec!["127.0.0.1/32".into()];
    config.sso.auto_provision = true;
    config.sso.default_role = "user".into();
    config.sso.group_role_map.insert("admins".into(), "admin".into());
    config.sso.group_role_map.insert("owners".into(), "owner".into());
    config.sso.allowed_email_domains = vec!["example.com".into()];
    config
}

fn sso_request(uri: &str, peer: &str, user: Option<&str>, email: Option<&str>, groups: Option<&str>) -> Request<Body> {
    let mut builder = Request::builder().uri(uri).header(header::HOST, "localhost");
    if let Some(user) = user {
        builder = builder.header("X-Auth-Request-User", user);
    }
    if let Some(email) = email {
        builder = builder.header("X-Auth-Request-Email", email);
    }
    if let Some(groups) = groups {
        builder = builder.header("X-Auth-Request-Groups", groups);
    }
    let mut request = builder.body(Body::empty()).unwrap();
    let addr: SocketAddr = format!("{peer}:40000").parse().unwrap();
    request.extensions_mut().insert(ConnectInfo(addr));
    request
}

fn session_cookie(response: &axum::response::Response) -> Option<String> {
    for value in response.headers().get_all(header::SET_COOKIE).iter() {
        if let Ok(text) = value.to_str() {
            if text.starts_with("lkb_session=") {
                return Some(text.split(';').next().unwrap_or("").to_string());
            }
        }
    }
    None
}

#[tokio::test]
async fn trusted_proxy_provisions_user_maps_role_and_rotates_session() {
    let temp = tempfile::tempdir().unwrap();
    let config = test_config(&temp.path().join("sso.sqlite"));
    let store = store::open(&config).await.unwrap();
    store::bootstrap_space(&store, &config).await.unwrap();
    let app = router(AppState::new(store.clone(), Arc::new(config.clone()), Arc::new(ProviderSet::with_none())));

    let status = app.clone().oneshot(Request::builder().uri("/api/v1/auth/sso/status")
        .header(header::HOST, "localhost").body(Body::empty()).unwrap()).await.unwrap();
    assert_eq!(status.status(), StatusCode::OK);
    let status_json: serde_json::Value = serde_json::from_slice(&axum::body::to_bytes(status.into_body(), 1_000_000).await.unwrap()).unwrap();
    assert_eq!(status_json["enabled"], true);
    assert_eq!(status_json["mode"], "trusted_header");

    let response = app.clone().oneshot(sso_request("/api/v1/auth/me", "127.0.0.1", Some("alice"), Some("alice@example.com"), Some("devs,admins"))).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert!(session_cookie(&response).is_some(), "SSO login must set HttpOnly session cookie");
    let json: serde_json::Value = serde_json::from_slice(&axum::body::to_bytes(response.into_body(), 1_000_000).await.unwrap()).unwrap();
    let user_id = json["principal"]["user_id"].as_str().unwrap().to_string();
    assert!(user_id.starts_with("usr_"));
    assert_eq!(json["principal"]["tenant_role"], "admin", "group_role_map must win over default role");

    let users = store.list_users("t_local").await.unwrap();
    assert_eq!(users.iter().filter(|user| user.username == "alice").count(), 1);
    let alice = users.iter().find(|user| user.username == "alice").unwrap();
    assert_eq!(alice.source, "sso");
    assert_eq!(alice.email.as_deref(), Some("alice@example.com"));
    assert_eq!(alice.role, UserRole::Admin);

    let again = app.clone().oneshot(sso_request("/api/v1/auth/me", "127.0.0.1", Some("alice"), Some("alice@example.com"), Some("admins"))).await.unwrap();
    assert_eq!(again.status(), StatusCode::OK);
    let again_json: serde_json::Value = serde_json::from_slice(&axum::body::to_bytes(again.into_body(), 1_000_000).await.unwrap()).unwrap();
    assert_eq!(again_json["principal"]["user_id"], user_id, "repeat SSO request must not create a second user");
    assert_eq!(store.list_users("t_local").await.unwrap().iter().filter(|user| user.username == "alice").count(), 1);

    let audit = store.list_audit(&AuditFilter { tenant_id: "t_local".into(), limit: 100, ..Default::default() }).await.unwrap();
    assert!(audit.iter().any(|event| event.action == "sso.provision"));
    assert!(audit.iter().any(|event| event.action == "sso.login"));
}

#[tokio::test]
async fn untrusted_peer_missing_user_domain_and_disabled_are_denied() {
    let temp = tempfile::tempdir().unwrap();
    let config = test_config(&temp.path().join("sso-deny.sqlite"));
    let store = store::open(&config).await.unwrap();
    store::bootstrap_space(&store, &config).await.unwrap();
    let app = router(AppState::new(store.clone(), Arc::new(config.clone()), Arc::new(ProviderSet::with_none())));

    let untrusted = app.clone().oneshot(sso_request("/api/v1/auth/me", "10.1.2.3", Some("mallory"), Some("mallory@example.com"), None)).await.unwrap();
    assert_eq!(untrusted.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(store.list_users("t_local").await.unwrap().iter().filter(|user| user.username == "mallory").count(), 0);

    let missing = app.clone().oneshot(sso_request("/api/v1/auth/me", "127.0.0.1", None, Some("nobody@example.com"), None)).await.unwrap();
    assert_eq!(missing.status(), StatusCode::UNAUTHORIZED);

    let domain = app.clone().oneshot(sso_request("/api/v1/auth/me", "127.0.0.1", Some("eve"), Some("eve@evil.example"), None)).await.unwrap();
    assert_eq!(domain.status(), StatusCode::FORBIDDEN);

    let first = app.clone().oneshot(sso_request("/api/v1/auth/me", "127.0.0.1", Some("bob"), Some("bob@example.com"), None)).await.unwrap();
    assert_eq!(first.status(), StatusCode::OK);
    let bob = store.get_user("t_local", "bob").await.unwrap().unwrap();
    store.set_user_disabled("t_local", &bob.user_id, true).await.unwrap();
    let disabled = app.clone().oneshot(sso_request("/api/v1/auth/me", "127.0.0.1", Some("bob"), Some("bob@example.com"), None)).await.unwrap();
    assert_eq!(disabled.status(), StatusCode::UNAUTHORIZED);

    let audit = store.list_audit(&AuditFilter { tenant_id: "t_local".into(), limit: 100, ..Default::default() }).await.unwrap();
    assert!(audit.iter().filter(|event| event.action == "sso.denied").count() >= 3);
}

#[tokio::test]
async fn spoofed_internal_marker_and_auto_provision_off_are_rejected() {
    let temp = tempfile::tempdir().unwrap();
    let mut config = test_config(&temp.path().join("sso-off.sqlite"));
    config.sso.auto_provision = false;
    let store = store::open(&config).await.unwrap();
    store::bootstrap_space(&store, &config).await.unwrap();
    let app = router(AppState::new(store.clone(), Arc::new(config), Arc::new(ProviderSet::with_none())));
    let unknown = app.clone().oneshot(sso_request("/api/v1/auth/me", "127.0.0.1", Some("new-user"), Some("new@example.com"), None)).await.unwrap();
    assert_eq!(unknown.status(), StatusCode::UNAUTHORIZED);
    assert!(store.list_users("t_local").await.unwrap().iter().all(|user| user.username != "new-user"));

    let mut spoof = Request::builder().uri("/api/v1/auth/me").header(header::HOST, "localhost")
        .header("x-lkb-sso-authenticated", "usr_victim").body(Body::empty()).unwrap();
    spoof.extensions_mut().insert(ConnectInfo("127.0.0.1:40001".parse::<SocketAddr>().unwrap()));
    let spoof_response = app.oneshot(spoof).await.unwrap();
    assert_eq!(spoof_response.status(), StatusCode::UNAUTHORIZED);
}

#[test]
fn sso_configuration_validation_is_strict() {
    let temp = tempfile::tempdir().unwrap();
    let mut config = test_config(&temp.path().join("validate.sqlite"));
    config.validate().expect("baseline SSO config must be valid");

    let mut invalid_mode = config.clone();
    invalid_mode.sso.mode = "trusted header".into();
    assert!(invalid_mode.validate().unwrap_err().to_string().contains("sso.mode"));

    let mut invalid_proxy = config.clone();
    invalid_proxy.sso.trusted_proxies = vec!["not-an-ip".into()];
    assert!(invalid_proxy.validate().unwrap_err().to_string().contains("trusted proxy"));

    let mut invalid_prefix = config.clone();
    invalid_prefix.sso.trusted_proxies = vec!["10.0.0.0/99".into()];
    assert!(invalid_prefix.validate().is_err());

    let mut invalid_role = config.clone();
    invalid_role.sso.default_role = "root".into();
    assert!(invalid_role.validate().unwrap_err().to_string().contains("sso.default_role"));

    let mut invalid_map = config.clone();
    invalid_map.sso.group_role_map.insert("team".into(), "superuser".into());
    assert!(invalid_map.validate().is_err());

    let mut invalid_domain = config.clone();
    invalid_domain.sso.allowed_email_domains = vec!["bad domain".into()];
    assert!(invalid_domain.validate().unwrap_err().to_string().contains("allowed_email_domains"));

    let mut invalid_ttl = config.clone();
    invalid_ttl.sso.session_ttl_hours = Some(0);
    assert!(invalid_ttl.validate().unwrap_err().to_string().contains("session_ttl_hours"));

    let mut empty_header = config.clone();
    empty_header.sso.user_header = "  ".into();
    assert!(empty_header.validate().unwrap_err().to_string().contains("user_header"));

    let mut empty_proxies = config.clone();
    empty_proxies.sso.trusted_proxies.clear();
    empty_proxies.validate().expect("empty proxies is valid but fail-closed");
    let warnings = empty_proxies.sso_warnings();
    assert_eq!(warnings.len(), 1);
    assert!(warnings[0].contains("deny all peers"));
}

#[tokio::test]
async fn empty_trusted_proxies_fail_closed_at_runtime() {
    let temp = tempfile::tempdir().unwrap();
    let mut config = test_config(&temp.path().join("sso-empty-proxies.sqlite"));
    config.sso.trusted_proxies.clear();
    let store = store::open(&config).await.unwrap();
    store::bootstrap_space(&store, &config).await.unwrap();
    let app = router(AppState::new(store.clone(), Arc::new(config), Arc::new(ProviderSet::with_none())));
    let response = app.oneshot(sso_request("/api/v1/auth/me", "127.0.0.1", Some("alice"), Some("alice@example.com"), None)).await.unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    assert!(store.list_users("t_local").await.unwrap().iter().all(|user| user.username != "alice"));
}
