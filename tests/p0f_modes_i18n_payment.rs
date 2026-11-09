use axum::body::Body;
use axum::http::{header, Request, StatusCode};
use lkb::config::{Config, DeploymentMode, FeatureOverrides, ProfileName};
use lkb::governance::{UserRole};
use lkb::i18n;
use lkb::providers::ProviderSet;
use lkb::store;
use lkb::util::{now_rfc3339, sha256_hex_str};
use lkb::web::{router, AppState};
use std::path::Path;
use std::sync::Arc;
use tower::ServiceExt;

fn test_config(path: &Path) -> Config {
    let mut config = Config::default();
    config.storage.sqlite_path = path.to_path_buf();
    config.auth.mode = lkb::config::AuthMode::None;
    config
}

#[test]
fn feature_defaults_and_overrides_follow_the_mode_matrix() {
    let solo = DeploymentMode::Solo.default_features();
    assert!(!solo.signup && !solo.billing && !solo.multi_tenant && !solo.sso);
    assert!(solo.audit && solo.rate_limit);
    let team = DeploymentMode::Team.default_features();
    assert!(!team.signup && !team.billing && !team.multi_tenant);
    assert!(team.audit && team.rate_limit);
    let internal = DeploymentMode::EnterpriseInternal.default_features();
    assert!(!internal.signup && !internal.billing);
    assert!(internal.sso && internal.multi_tenant && internal.public_api);
    let external = DeploymentMode::EnterpriseExternal.default_features();
    assert!(external.signup && external.billing && external.multi_tenant && external.public_api);

    let overrides = FeatureOverrides {
        billing: Some(true),
        sso: Some(false),
        ..FeatureOverrides::default()
    };
    let resolved = overrides.resolve(DeploymentMode::Team);
    assert!(resolved.billing);
    assert!(!resolved.sso);
    assert!(resolved.audit);
}

#[test]
fn invalid_solo_billing_and_external_without_required_features_are_rejected() {
    let mut solo = Config::default();
    solo.features.billing = Some(true);
    assert!(solo.validate().is_err());

    let mut enterprise = Config::default();
    enterprise.profile = Some(ProfileName::Enterprise);
    enterprise.storage.backend = lkb::config::Backend::Postgres;
    enterprise.storage.url_env = "LKB_TEST_P0F_DATABASE_URL".to_string();
    std::env::set_var("LKB_TEST_P0F_DATABASE_URL", "postgres://example/db");
    enterprise.deployment.mode = Some(DeploymentMode::EnterpriseExternal);
    enterprise.features.billing = Some(false);
    assert!(enterprise.validate().is_err());

    enterprise.features.billing = Some(true);
    enterprise.features.signup = Some(true);
    enterprise.features.multi_tenant = Some(true);
    enterprise.features.public_api = Some(true);
    assert!(enterprise.validate().is_ok());
}

#[tokio::test]
async fn feature_disabled_and_payment_not_configured_are_stable_codes() {
    let temp = tempfile::tempdir().unwrap();
    let mut config = test_config(&temp.path().join("feature.sqlite"));
    config.features.audit = Some(false);
    let store = store::open(&config).await.unwrap();
    store::bootstrap_space(&store, &config).await.unwrap();
    let app = router(AppState::new(
        store.clone(),
        Arc::new(config.clone()),
        Arc::new(ProviderSet::with_none()),
    ));
    let audit = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v1/admin/audit")
                .header(header::HOST, "localhost")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(audit.status(), StatusCode::FORBIDDEN);
    let json: serde_json::Value = serde_json::from_slice(
        &axum::body::to_bytes(audit.into_body(), 1_000_000).await.unwrap(),
    )
    .unwrap();
    assert_eq!(json["error"], "feature_disabled");

    let mut billing = test_config(&temp.path().join("billing.sqlite"));
    billing.features.billing = Some(true);
    let store = store::open(&billing).await.unwrap();
    store::bootstrap_space(&store, &billing).await.unwrap();
    let app = router(AppState::new(
        store,
        Arc::new(billing),
        Arc::new(ProviderSet::with_none()),
    ));
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/billing/checkout")
                .header(header::HOST, "localhost")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(r#"{"plan":"pro","amount_minor":1000}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    let json: serde_json::Value = serde_json::from_slice(
        &axum::body::to_bytes(response.into_body(), 1_000_000).await.unwrap(),
    )
    .unwrap();
    assert_eq!(json["error"], "payment_not_configured");

    let payment = lkb::config::PaymentConfig::default();
    assert!(matches!(
        lkb::payment::provider_from_config(&payment),
        Err(lkb::payment::PaymentError::NotConfigured)
    ));
    assert_eq!(lkb::payment::is_configured(&payment), false);
}

#[test]
fn locale_resolution_and_formatters_are_stable() {
    let supported = vec!["zh-CN".to_string(), "en".to_string()];
    assert_eq!(
        i18n::resolve(Some("zh-CN, en;q=0.8"), None, None, "en", &supported),
        "zh-CN"
    );
    assert_eq!(
        i18n::resolve(Some("fr-FR, en;q=0.5"), None, None, "en", &supported),
        "en"
    );
    assert_eq!(i18n::resolve(None, None, None, "en", &supported), "en");
    assert_eq!(i18n::format_number("en", 12345.6, 1), "12,345.6");
    assert_eq!(i18n::format_currency("zh-CN", 1234, "CNY"), "¥12.34 CNY");
    assert_eq!(i18n::plural("en", 1, "one", "other"), "one");
    assert_eq!(i18n::plural("en", 2, "one", "other"), "other");
    assert_eq!(i18n::plural("zh-CN", 1, "one", "other"), "other");
}

async fn localized_unauthorized(locale: &str) -> serde_json::Value {
    let temp = tempfile::tempdir().unwrap();
    let mut config = test_config(&temp.path().join("locale.sqlite"));
    config.auth.mode = lkb::config::AuthMode::Session;
    let store = store::open(&config).await.unwrap();
    let app = router(AppState::new(
        store,
        Arc::new(config),
        Arc::new(ProviderSet::with_none()),
    ));
    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/spaces")
                .header(header::HOST, "localhost")
                .header(header::ACCEPT_LANGUAGE, locale)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(
        response.headers().get(header::CONTENT_LANGUAGE).unwrap(),
        locale
    );
    serde_json::from_slice(
        &axum::body::to_bytes(response.into_body(), 1_000_000).await.unwrap(),
    )
    .unwrap()
}

#[tokio::test]
async fn api_errors_are_localized_and_cli_locale_is_configurable() {
    let zh = localized_unauthorized("zh-CN").await;
    let en = localized_unauthorized("en").await;
    assert_eq!(zh["error"], "unauthorized");
    assert_ne!(zh["message"], en["message"]);
    assert!(zh["message"].as_str().unwrap().contains("未认证"));

    let temp = tempfile::tempdir().unwrap();
    let config_path = temp.path().join("lkb.toml");
    std::fs::write(
        &config_path,
        "profile = \"solo\"\n[i18n]\ndefault_locale = \"zh-CN\"\nsupported_locales = [\"zh-CN\", \"en\"]\n",
    )
    .unwrap();
    let output = assert_cmd::Command::cargo_bin("lkb")
        .unwrap()
        .arg("--config")
        .arg(&config_path)
        .args(["auth", "logout"])
        .env("LKB_CONFIG_FILE", temp.path().join("api.toml"))
        .output()
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("已退出登录"), "stdout: {stdout}");
}

#[tokio::test]
async fn token_role_session_still_works_with_feature_endpoint() {
    let temp = tempfile::tempdir().unwrap();
    let config = test_config(&temp.path().join("features.sqlite"));
    let store = store::open(&config).await.unwrap();
    store::bootstrap_space(&store, &config).await.unwrap();
    store
        .add_user_with_role(
            &config.storage.tenant_id,
            "owner",
            "not-a-real-argon2-hash",
            UserRole::Owner,
        )
        .await
        .unwrap();
    let user = store
        .get_user(&config.storage.tenant_id, "owner")
        .await
        .unwrap()
        .unwrap();
    let raw = format!("lkb_{}", lkb::util::random_hex(12));
    store
        .create_token(&lkb::store::TokenRecord {
            token_hash: sha256_hex_str(&raw),
            token_id: format!("tok_{}", lkb::util::random_hex(6)),
            tenant_id: config.storage.tenant_id.clone(),
            user_id: user.user_id,
            scope: lkb::governance::TokenScope::Admin,
            kind: lkb::governance::PrincipalKind::Human,
            label: "p0f".into(),
            created_at: now_rfc3339(),
            expires_at: None,
            revoked_at: None,
        })
        .await
        .unwrap();
    let app = router(AppState::new(
        store,
        Arc::new(config),
        Arc::new(ProviderSet::with_none()),
    ));
    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/features")
                .header(header::HOST, "localhost")
                .header(header::AUTHORIZATION, format!("Bearer {raw}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let json: serde_json::Value = serde_json::from_slice(
        &axum::body::to_bytes(response.into_body(), 1_000_000).await.unwrap(),
    )
    .unwrap();
    assert_eq!(json["mode"], "solo");
    assert_eq!(json["features"]["billing"], false);
    assert_eq!(json["features"]["rate_limit"], true);
}
