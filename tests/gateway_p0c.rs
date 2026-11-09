// SPDX-License-Identifier: MIT OR Apache-2.0
use axum::body::Body;
use axum::http::StatusCode;
use axum::http::{header, Request};
use axum::routing::post;
use axum::{Json, Router};
use lkb::answer::{build_answer, AnswerOptions};
use lkb::config::Config;
use lkb::gateway::{ChatMessage, Gateway, GatewayRequest};
use lkb::governance::{Policy, Principal, UserRole};
use lkb::ingest::{import_path, ImportOptions};
use lkb::providers::ProviderSet;
use lkb::store::{
    self, ModelAliasRecord, ProviderModelRecord, ProviderRecord, RouteRecord, TokenQuotaRecord,
    UsageFilter,
};
use lkb::util::{now_rfc3339, random_hex, sha256_hex_str};
use lkb::web::{router, AppState};
use std::sync::Arc;
use tokio::net::TcpListener;
use tower::ServiceExt;

async fn ok_provider() -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "choices": [{"message": {"role": "assistant", "content": "gateway-ok"}}],
        "usage": {"prompt_tokens": 7, "completion_tokens": 3}
    }))
}

async fn answer_provider() -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "choices": [{"message": {"role": "assistant", "content": "{\"answer\":\"gateway answer\",\"citations\":[1]}"}}],
        "usage": {"prompt_tokens": 11, "completion_tokens": 4}
    }))
}

async fn fail_provider() -> StatusCode {
    StatusCode::INTERNAL_SERVER_ERROR
}

async fn start_provider() -> String {
    let app = Router::new()
        .route("/ok/chat/completions", post(ok_provider))
        .route("/fail/chat/completions", post(fail_provider))
        .route("/answer/chat/completions", post(answer_provider));
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    format!("http://{addr}")
}

fn test_config(path: &std::path::Path) -> Config {
    let mut config = Config::default();
    config.storage.sqlite_path = path.to_path_buf();
    // Token quota management is a team+ capability; enable it explicitly so
    // the admin quota API tests exercise the enabled path.
    config.features.quotas = Some(true);
    config
}

fn principal(tenant: &str, user: &str) -> Principal {
    Principal::local_user_with_role(
        user,
        tenant,
        vec!["s_default".to_string()],
        Vec::new(),
        UserRole::User,
    )
}

#[tokio::test]
async fn gateway_fails_over_and_records_usage() {
    let temp = tempfile::tempdir().unwrap();
    let config = test_config(&temp.path().join("gateway.sqlite"));
    let store = store::open(&config).await.unwrap();
    let base = start_provider().await;
    let now = now_rfc3339();
    for (id, path) in [("p_fail", "/fail"), ("p_ok", "/ok")] {
        store
            .upsert_provider(
                "t_local",
                &ProviderRecord {
                    provider_id: id.to_string(),
                    kind: "openai_compatible".to_string(),
                    base_url: format!("{base}{path}"),
                    api_key_env: None,
                    status: "active".to_string(),
                    priority: 100,
                    weight: 1,
                    limits_json: "{}".to_string(),
                    timeouts_json: "{\"total_ms\":2000}".to_string(),
                    created_at: now.clone(),
                    updated_at: now.clone(),
                },
            )
            .await
            .unwrap();
    }
    for (id, provider) in [("pm_fail", "p_fail"), ("pm_ok", "p_ok")] {
        store
            .upsert_provider_model(
                "t_local",
                &ProviderModelRecord {
                    provider_model_id: id.to_string(),
                    provider_id: provider.to_string(),
                    upstream_model: "fake-upstream".to_string(),
                    context_window: 8192,
                    pricing_json: "{\"input_per_million_micros\":1000000,\"output_per_million_micros\":2000000,\"currency\":\"USD\"}".to_string(),
                    capabilities_json: "[\"chat\"]".to_string(),
                    status: "active".to_string(),
                    created_at: now.clone(),
                    updated_at: now.clone(),
                },
            )
            .await
            .unwrap();
    }
    store
        .upsert_model_alias(
            "t_local",
            &ModelAliasRecord {
                public_id: "chat".to_string(),
                display_name: "Chat".to_string(),
                capabilities_json: "[\"chat\"]".to_string(),
                status: "active".to_string(),
                access_policy_json: "{\"allow_all\":true}".to_string(),
                price_json: "{\"input_per_million_micros\":2000000,\"output_per_million_micros\":4000000,\"currency\":\"USD\"}".to_string(),
                version: 1,
                created_at: now.clone(),
                updated_at: now.clone(),
            },
        )
        .await
        .unwrap();
    for (route_id, model, priority) in [("r_fail", "pm_fail", 1), ("r_ok", "pm_ok", 2)] {
        store
            .upsert_route(
                "t_local",
                &RouteRecord {
                    route_id: route_id.to_string(),
                    alias_public_id: "chat".to_string(),
                    provider_model_id: model.to_string(),
                    weight: 1,
                    priority,
                    health: "healthy".to_string(),
                    circuit_state: "closed".to_string(),
                    last_error_at: None,
                    circuit_open_until: None,
                    created_at: now.clone(),
                    updated_at: now.clone(),
                },
            )
            .await
            .unwrap();
    }

    let gateway = Gateway::new(store.clone());
    let request_id = format!("req_{}", random_hex(8));
    let response = gateway
        .chat(&GatewayRequest {
            request_id: request_id.clone(),
            principal: principal("t_local", "alice"),
            space_id: None,
            role: "user".to_string(),
            alias: "chat".to_string(),
            messages: vec![ChatMessage {
                role: "user".into(),
                content: "hello world".into(),
            }],
            max_output_tokens: 32,
            temperature: 0.0,
            source: "kb".into(),
            rewritten_query: None,
        })
        .await
        .unwrap();
    assert_eq!(response.model, "chat");
    assert_eq!(response.content, "gateway-ok");
    assert_eq!(response.usage.input_tokens, 7);
    assert_eq!(response.usage.output_tokens, 3);
    assert_eq!(response.usage.token_source, "provider");
    assert!(response.usage.cost_micros > 0);

    let events = store
        .list_usage_events(&UsageFilter {
            tenant_id: "t_local".into(),
            limit: 10,
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].alias, "chat");
    assert_eq!(events[0].provider_model_id, "pm_ok");
    assert_eq!(events[0].input_tokens, 7);
    assert_eq!(events[0].status, "success");

    let routes = store.list_routes("t_local", Some("chat")).await.unwrap();
    let failed = routes
        .iter()
        .find(|route| route.route_id == "r_fail")
        .unwrap();
    assert_eq!(failed.health, "open");
    assert_eq!(failed.circuit_state, "open");
}

#[tokio::test]
async fn hard_token_quota_blocks_before_provider_call() {
    let temp = tempfile::tempdir().unwrap();
    let config = test_config(&temp.path().join("quota.sqlite"));
    let store = store::open(&config).await.unwrap();
    let now = now_rfc3339();
    store
        .upsert_model_alias(
            "t_local",
            &ModelAliasRecord {
                public_id: "chat".to_string(),
                display_name: "Chat".to_string(),
                capabilities_json: "[\"chat\"]".to_string(),
                status: "active".to_string(),
                access_policy_json: "{\"allow_all\":true}".to_string(),
                price_json: "{}".to_string(),
                version: 1,
                created_at: now.clone(),
                updated_at: now.clone(),
            },
        )
        .await
        .unwrap();
    store
        .upsert_provider(
            "t_local",
            &ProviderRecord {
                provider_id: "p_quota".into(),
                kind: "openai_compatible".into(),
                base_url: "http://127.0.0.1:1".into(),
                api_key_env: None,
                status: "active".into(),
                priority: 100,
                weight: 1,
                limits_json: "{}".into(),
                timeouts_json: "{}".into(),
                created_at: now.clone(),
                updated_at: now.clone(),
            },
        )
        .await
        .unwrap();
    store
        .upsert_provider_model(
            "t_local",
            &ProviderModelRecord {
                provider_model_id: "pm_quota".into(),
                provider_id: "p_quota".into(),
                upstream_model: "fake".into(),
                context_window: 8192,
                pricing_json: "{}".into(),
                capabilities_json: "[\"chat\"]".into(),
                status: "active".into(),
                created_at: now.clone(),
                updated_at: now.clone(),
            },
        )
        .await
        .unwrap();
    store
        .upsert_route(
            "t_local",
            &RouteRecord {
                route_id: "r_quota".into(),
                alias_public_id: "chat".into(),
                provider_model_id: "pm_quota".into(),
                weight: 1,
                priority: 1,
                health: "healthy".into(),
                circuit_state: "closed".into(),
                last_error_at: None,
                circuit_open_until: None,
                created_at: now.clone(),
                updated_at: now.clone(),
            },
        )
        .await
        .unwrap();
    store
        .upsert_token_quota(&TokenQuotaRecord {
            quota_id: "tq_test".into(),
            tenant_id: "t_local".into(),
            scope_type: "user".into(),
            scope_id: "alice".into(),
            period: "month".into(),
            limit_tokens: Some(1),
            limit_requests: Some(100),
            limit_cost_micros: None,
            currency: "USD".into(),
            enforcement: "hard".into(),
            action: "block".into(),
            enabled: true,
            created_at: now.clone(),
            updated_at: now,
        })
        .await
        .unwrap();
    let error = Gateway::new(store)
        .chat(&GatewayRequest {
            request_id: format!("req_{}", random_hex(8)),
            principal: principal("t_local", "alice"),
            space_id: None,
            role: "user".into(),
            alias: "chat".into(),
            messages: vec![ChatMessage {
                role: "user".into(),
                content: "a long prompt for quota".into(),
            }],
            max_output_tokens: 100,
            temperature: 0.0,
            source: "kb".into(),
            rewritten_query: None,
        })
        .await
        .unwrap_err();
    assert_eq!(error.status, StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(error.code, "quota_exceeded");
}

#[tokio::test]
async fn alias_access_policy_and_unavailable_alias_are_safe() {
    let temp = tempfile::tempdir().unwrap();
    let config = test_config(&temp.path().join("alias.sqlite"));
    let store = store::open(&config).await.unwrap();
    let gateway = Gateway::new(store.clone());
    let error = gateway
        .chat(&GatewayRequest {
            request_id: "req_missing".into(),
            principal: Principal::local_user("alice", "t_local", vec!["s_default".into()], vec![]),
            space_id: None,
            role: "user".into(),
            alias: "missing".into(),
            messages: vec![ChatMessage {
                role: "user".into(),
                content: "hi".into(),
            }],
            max_output_tokens: 8,
            temperature: 0.0,
            source: "kb".into(),
            rewritten_query: None,
        })
        .await
        .unwrap_err();
    assert_eq!(error.code, "model_unavailable");
}

#[tokio::test]
async fn admin_model_usage_and_quota_api_is_redacted() {
    let temp = tempfile::tempdir().unwrap();
    let mut config = test_config(&temp.path().join("admin.sqlite"));
    config.storage.tenant_id = "t_local".into();
    config.storage.default_space = "s_default".into();
    let store = store::open(&config).await.unwrap();
    let now = now_rfc3339();
    store
        .upsert_provider(
            "t_local",
            &ProviderRecord {
                provider_id: "p_admin".into(),
                kind: "openai_compatible".into(),
                base_url: "http://secret-provider.invalid/v1".into(),
                api_key_env: Some("SECRET_ENV".into()),
                status: "active".into(),
                priority: 100,
                weight: 1,
                limits_json: "{}".into(),
                timeouts_json: "{}".into(),
                created_at: now.clone(),
                updated_at: now.clone(),
            },
        )
        .await
        .unwrap();
    store
        .upsert_model_alias(
            "t_local",
            &ModelAliasRecord {
                public_id: "shown-model".into(),
                display_name: "Shown".into(),
                capabilities_json: "[\"chat\"]".into(),
                status: "active".into(),
                access_policy_json: "{\"allow_all\":true}".into(),
                price_json: "{}".into(),
                version: 1,
                created_at: now.clone(),
                updated_at: now.clone(),
            },
        )
        .await
        .unwrap();
    store
        .add_user_with_role("t_local", "admin", "x", UserRole::Owner)
        .await
        .unwrap();
    let user = store.get_user("t_local", "admin").await.unwrap().unwrap();
    let raw = format!("lkb_{}", random_hex(16));
    store
        .create_token(&lkb::store::TokenRecord {
            token_hash: sha256_hex_str(&raw),
            token_id: format!("tok_{}", random_hex(6)),
            tenant_id: "t_local".into(),
            user_id: user.user_id,
            scope: lkb::governance::TokenScope::Admin,
            kind: lkb::governance::PrincipalKind::Human,
            label: "admin".into(),
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
    let request = Request::builder()
        .uri("/api/v1/admin/providers")
        .header(header::HOST, "localhost")
        .header(header::AUTHORIZATION, format!("Bearer {raw}"))
        .body(Body::empty())
        .unwrap();
    let response = app.clone().oneshot(request).await.unwrap();
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), 1_000_000)
        .await
        .unwrap();
    let text = String::from_utf8(bytes.to_vec()).unwrap();
    assert_eq!(status, StatusCode::OK, "body={text}");
    assert!(text.contains("p_admin"));
    assert!(text.contains("key_configured"));
    assert!(!text.contains("api_key_env"));
    assert!(!text.contains("SECRET_ENV"));

    let request = Request::builder()
        .method("POST")
        .uri("/api/v1/admin/quotas")
        .header(header::HOST, "localhost")
        .header(header::CONTENT_TYPE, "application/json")
        .header(header::AUTHORIZATION, format!("Bearer {raw}"))
        .body(Body::from(
            r#"{"scope_type":"user","scope_id":"alice","period":"month","limit_tokens":100}"#,
        ))
        .unwrap();
    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);
}

#[tokio::test]
async fn answer_llm_uses_gateway_alias_and_records_kb_usage() {
    let temp = tempfile::tempdir().unwrap();
    let config = test_config(&temp.path().join("answer-gateway.sqlite"));
    let store = store::open(&config).await.unwrap();
    let base = start_provider().await;
    let now = now_rfc3339();
    store
        .upsert_provider(
            "t_local",
            &ProviderRecord {
                provider_id: "p_answer".into(),
                kind: "openai_compatible".into(),
                base_url: format!("{base}/answer"),
                api_key_env: None,
                status: "active".into(),
                priority: 100,
                weight: 1,
                limits_json: "{}".into(),
                timeouts_json: "{}".into(),
                created_at: now.clone(),
                updated_at: now.clone(),
            },
        )
        .await
        .unwrap();
    store
        .upsert_provider_model(
            "t_local",
            &ProviderModelRecord {
                provider_model_id: "pm_answer".into(),
                provider_id: "p_answer".into(),
                upstream_model: "answer-model".into(),
                context_window: 8192,
                pricing_json: "{}".into(),
                capabilities_json: "[\"chat\"]".into(),
                status: "active".into(),
                created_at: now.clone(),
                updated_at: now.clone(),
            },
        )
        .await
        .unwrap();
    store
        .upsert_model_alias(
            "t_local",
            &ModelAliasRecord {
                public_id: "default".into(),
                display_name: "Default".into(),
                capabilities_json: "[\"chat\"]".into(),
                status: "active".into(),
                access_policy_json: "{\"allow_all\":true}".into(),
                price_json: "{}".into(),
                version: 1,
                created_at: now.clone(),
                updated_at: now.clone(),
            },
        )
        .await
        .unwrap();
    store
        .upsert_route(
            "t_local",
            &RouteRecord {
                route_id: "r_answer".into(),
                alias_public_id: "default".into(),
                provider_model_id: "pm_answer".into(),
                weight: 1,
                priority: 1,
                health: "healthy".into(),
                circuit_state: "closed".into(),
                last_error_at: None,
                circuit_open_until: None,
                created_at: now.clone(),
                updated_at: now,
            },
        )
        .await
        .unwrap();
    let dir = temp.path().join("docs");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("01.md"), "# Hello\n\nDocument sentence.").unwrap();
    import_path(
        &store,
        &dir,
        &ImportOptions {
            tenant_id: "t_local".into(),
            space_id: "s_default".into(),
            acl: Some(lkb::governance::Acl::allow_all()),
            allow_implicit_allow_all: true,
            ..Default::default()
        },
        None,
        None,
    )
    .await
    .unwrap();
    let policy = Policy::new(
        Principal::local_user("alice", "t_local", vec!["s_default".into()], vec![]),
        None,
    )
    .unwrap();
    let answer = build_answer(
        &store,
        &ProviderSet::with_none(),
        &policy,
        "Hello",
        &AnswerOptions::default(),
    )
    .await
    .unwrap();
    assert_eq!(answer.mode, lkb::contract::AnswerMode::Generative);
    assert_eq!(answer.model.as_deref(), Some("default"));
    assert!(answer.usage.as_ref().unwrap().input_tokens > 0);
    let events = store
        .list_usage_events(&UsageFilter {
            tenant_id: "t_local".into(),
            limit: 10,
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].source, "kb");
    assert_eq!(events[0].alias, "default");
}

#[tokio::test]
async fn quota_commit_uses_actual_tokens_and_soft_limit_audits() {
    let temp = tempfile::tempdir().unwrap();
    let config = test_config(&temp.path().join("quota-actual.sqlite"));
    let store = store::open(&config).await.unwrap();
    let now = now_rfc3339();
    store
        .upsert_token_quota(&TokenQuotaRecord {
            quota_id: "tq_actual".into(),
            tenant_id: "t_local".into(),
            scope_type: "user".into(),
            scope_id: "alice".into(),
            period: "month".into(),
            limit_tokens: Some(1000),
            limit_requests: Some(100),
            limit_cost_micros: None,
            currency: "USD".into(),
            enforcement: "hard".into(),
            action: "block".into(),
            enabled: true,
            created_at: now.clone(),
            updated_at: now.clone(),
        })
        .await
        .unwrap();
    let principal = Principal::local_user("alice", "t_local", vec!["s_default".into()], vec![]);
    let request_id = "qreq_actual";
    lkb::token_quota::reserve(
        &store,
        &principal,
        None,
        "user",
        request_id,
        &lkb::token_quota::Estimate {
            input_tokens: 100,
            max_output_tokens: 100,
            cost_micros: 0,
            currency: "USD".into(),
        },
    )
    .await
    .unwrap();
    lkb::token_quota::commit(&store, "t_local", request_id, 10, 0).await;
    let counter = store
        .get_quota_counter(
            "t_local",
            "user",
            "alice",
            &chrono::Utc::now().format("%Y-%m-01").to_string(),
        )
        .await
        .unwrap()
        .unwrap();
    assert_eq!(counter.used_tokens, 10);
    assert_eq!(counter.reserved_tokens, 0);

    store
        .upsert_token_quota(&TokenQuotaRecord {
            quota_id: "tq_soft".into(),
            tenant_id: "t_local".into(),
            scope_type: "user".into(),
            scope_id: "bob".into(),
            period: "month".into(),
            limit_tokens: Some(1),
            limit_requests: None,
            limit_cost_micros: None,
            currency: "USD".into(),
            enforcement: "soft".into(),
            action: "alert".into(),
            enabled: true,
            created_at: now.clone(),
            updated_at: now.clone(),
        })
        .await
        .unwrap();
    let bob = Principal::local_user("bob", "t_local", vec!["s_default".into()], vec![]);
    lkb::token_quota::reserve(
        &store,
        &bob,
        None,
        "user",
        "qreq_soft",
        &lkb::token_quota::Estimate {
            input_tokens: 100,
            max_output_tokens: 100,
            cost_micros: 0,
            currency: "USD".into(),
        },
    )
    .await
    .unwrap();
    let audit = store
        .list_audit(&lkb::store::AuditFilter {
            tenant_id: "t_local".into(),
            action: Some("quota.soft_exceeded".into()),
            limit: 10,
            ..Default::default()
        })
        .await
        .unwrap();
    assert!(!audit.is_empty());
}

#[tokio::test]
async fn capability_mismatch_skips_route() {
    let temp = tempfile::tempdir().unwrap();
    let config = test_config(&temp.path().join("capability.sqlite"));
    let store = store::open(&config).await.unwrap();
    let now = now_rfc3339();
    store
        .upsert_provider(
            "t_local",
            &ProviderRecord {
                provider_id: "p_vision".into(),
                kind: "openai_compatible".into(),
                base_url: "http://127.0.0.1:1/vision".into(),
                api_key_env: None,
                status: "active".into(),
                priority: 1,
                weight: 1,
                limits_json: "{}".into(),
                timeouts_json: "{}".into(),
                created_at: now.clone(),
                updated_at: now.clone(),
            },
        )
        .await
        .unwrap();
    store
        .upsert_provider_model(
            "t_local",
            &ProviderModelRecord {
                provider_model_id: "pm_vision".into(),
                provider_id: "p_vision".into(),
                upstream_model: "vision-model".into(),
                context_window: 8192,
                pricing_json: "{}".into(),
                capabilities_json: "[\"vision\"]".into(),
                status: "active".into(),
                created_at: now.clone(),
                updated_at: now.clone(),
            },
        )
        .await
        .unwrap();
    store
        .upsert_model_alias(
            "t_local",
            &ModelAliasRecord {
                public_id: "chat".into(),
                display_name: "Chat".into(),
                capabilities_json: "[\"chat\"]".into(),
                status: "active".into(),
                access_policy_json: "{\"allow_all\":true}".into(),
                price_json: "{}".into(),
                version: 1,
                created_at: now.clone(),
                updated_at: now.clone(),
            },
        )
        .await
        .unwrap();
    store
        .upsert_route(
            "t_local",
            &RouteRecord {
                route_id: "r_vision".into(),
                alias_public_id: "chat".into(),
                provider_model_id: "pm_vision".into(),
                weight: 1,
                priority: 1,
                health: "healthy".into(),
                circuit_state: "closed".into(),
                last_error_at: None,
                circuit_open_until: None,
                created_at: now.clone(),
                updated_at: now,
            },
        )
        .await
        .unwrap();
    let error = Gateway::new(store)
        .chat(&GatewayRequest {
            request_id: "cap_req".into(),
            principal: Principal::local_user("alice", "t_local", vec!["s_default".into()], vec![]),
            space_id: None,
            role: "user".into(),
            alias: "chat".into(),
            messages: vec![ChatMessage {
                role: "user".into(),
                content: "hello".into(),
            }],
            max_output_tokens: 8,
            temperature: 0.0,
            source: "kb".into(),
            rewritten_query: None,
        })
        .await
        .unwrap_err();
    assert_eq!(error.code, "model_unavailable");
}
