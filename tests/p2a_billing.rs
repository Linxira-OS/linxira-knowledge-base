use axum::body::Body;
use axum::http::{header, Request, StatusCode};
use lkb::billing::{Billing, BillingError, SubscriptionStatus};
use lkb::config::Config;
use lkb::payment::{sign_stripe_webhook, PaymentProvider, StripeProvider, WebhookRequest};
use lkb::providers::ProviderSet;
use lkb::store::{self, Store};
use lkb::web::{router, AppState};
use std::sync::Arc;
use tower::ServiceExt;

fn test_config(path: &std::path::Path) -> Config {
    let mut config = Config::default();
    config.storage.sqlite_path = path.to_path_buf();
    config.storage.tenant_id = "t_local".into();
    config.storage.default_space = "s_default".into();
    config.billing.enabled = true;
    config.billing.engine = "local".into();
    config.billing.currency = "USD".into();
    config.payment.provider = "manual".into();
    config.features.billing = Some(true);
    config
}

async fn summary_provider() -> axum::Json<serde_json::Value> {
    axum::Json(serde_json::json!({
        "choices": [{"message": {"role": "assistant", "content": "{\"answer\":\"hi\",\"citations\":[1]}"}}],
        "usage": {"prompt_tokens": 5, "completion_tokens": 5}
    }))
}

async fn start_provider() -> String {
    use axum::routing::post;
    let app = axum::Router::new().route("/chat/completions", post(summary_provider));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    format!("http://{addr}")
}

async fn wire_alias(store: &Arc<dyn Store>, base: &str) {
    use lkb::store::{ModelAliasRecord, ProviderModelRecord, ProviderRecord, RouteRecord};
    let now = lkb::util::now_rfc3339();
    store
        .upsert_provider(
            "t_local",
            &ProviderRecord {
                provider_id: "p_bill".into(),
                kind: "openai_compatible".into(),
                base_url: base.into(),
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
                provider_model_id: "pm_bill".into(),
                provider_id: "p_bill".into(),
                upstream_model: "bill-model".into(),
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
                price_json:
                    "{\"input_per_million_micros\":1000000,\"output_per_million_micros\":1000000}"
                        .into(),
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
                route_id: "r_bill".into(),
                alias_public_id: "default".into(),
                provider_model_id: "pm_bill".into(),
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
}

#[tokio::test]
async fn wallet_ledger_double_entry_idempotency_and_insufficient() {
    let temp = tempfile::tempdir().unwrap();
    let config = test_config(&temp.path().join("billing.sqlite"));
    let store = store::open(&config).await.unwrap();
    let billing = Billing::new(store.clone(), config.billing.clone());
    let topup = billing
        .topup("t_local", 10_000_000, "manual", None, "manual:seed")
        .await
        .unwrap();
    billing.settle_topup(&topup).await.unwrap();
    let wallet = billing.wallet("t_local").await.unwrap();
    assert_eq!(wallet.balance_micros, 10_000_000);

    // replaying the same topup idempotency key does not double-credit
    let again = billing
        .topup("t_local", 10_000_000, "manual", None, "manual:seed")
        .await
        .unwrap();
    assert_eq!(again.topup_id, topup.topup_id);
    billing.settle_topup(&again).await.unwrap();
    assert_eq!(
        billing.wallet("t_local").await.unwrap().balance_micros,
        10_000_000
    );

    // ledger is balanced
    let entries = store.billing_list_ledger("t_local", 100).await.unwrap();
    let debit: i64 = entries
        .iter()
        .filter(|entry| entry.direction == "debit")
        .map(|entry| entry.amount_micros)
        .sum();
    let credit: i64 = entries
        .iter()
        .filter(|entry| entry.direction == "credit")
        .map(|entry| entry.amount_micros)
        .sum();
    assert_eq!(debit, credit);
    assert_eq!(debit, 10_000_000);

    // usage debit + refund reversal
    billing
        .charge_usage("t_local", 3_000_000, 1_000, "ue_1")
        .await
        .unwrap();
    assert_eq!(
        billing.wallet("t_local").await.unwrap().balance_micros,
        7_000_000
    );
    billing
        .refund("t_local", 1_000_000, "goodwill", "refund:1")
        .await
        .unwrap();
    assert_eq!(
        billing.wallet("t_local").await.unwrap().balance_micros,
        8_000_000
    );

    // over-charge is refused and leaves the balance untouched
    let error = billing
        .charge_usage("t_local", 9_000_000, 10, "ue_2")
        .await
        .unwrap_err();
    assert!(matches!(error, BillingError::Insufficient));
    assert_eq!(
        billing.wallet("t_local").await.unwrap().balance_micros,
        8_000_000
    );
    assert!(matches!(
        billing
            .authorize_call("t_local", 9_000_000)
            .await
            .unwrap_err(),
        BillingError::Insufficient
    ));
}

#[tokio::test]
async fn concurrent_charges_never_overdraw() {
    let temp = tempfile::tempdir().unwrap();
    let config = test_config(&temp.path().join("concurrent.sqlite"));
    let store = store::open(&config).await.unwrap();
    let billing = Arc::new(Billing::new(store.clone(), config.billing.clone()));
    let topup = billing
        .topup("t_local", 1_000_000, "manual", None, "manual:conc")
        .await
        .unwrap();
    billing.settle_topup(&topup).await.unwrap();
    let mut handles = Vec::new();
    for index in 0..10 {
        let billing = billing.clone();
        handles.push(tokio::spawn(async move {
            billing
                .charge_usage("t_local", 200_000, 0, &format!("ue_{index}"))
                .await
        }));
    }
    let mut applied = 0;
    let mut refused = 0;
    for handle in handles {
        match handle.await.unwrap() {
            Ok(()) => applied += 1,
            Err(BillingError::Insufficient) => refused += 1,
            Err(err) => panic!("unexpected error: {err}"),
        }
    }
    assert_eq!(applied, 5);
    assert_eq!(refused, 5);
    assert_eq!(billing.wallet("t_local").await.unwrap().balance_micros, 0);
}

#[tokio::test]
async fn plan_state_machine_and_entitlement_snapshot() {
    let temp = tempfile::tempdir().unwrap();
    let config = test_config(&temp.path().join("plans.sqlite"));
    let store = store::open(&config).await.unwrap();
    lkb::billing::seed_default_plans(&store, "USD")
        .await
        .unwrap();
    let billing = Billing::new(store.clone(), config.billing.clone());
    let subscription = billing.subscribe("t_local", "starter", 14).await.unwrap();
    assert_eq!(subscription.status, "trial");
    let entitlement = billing.entitlement("t_local").await.unwrap();
    assert_eq!(entitlement.plan_id.as_deref(), Some("starter"));
    assert_eq!(entitlement.included_tokens, 5_000_000);
    assert_eq!(entitlement.status, "trial");

    assert!(billing
        .transition("t_local", SubscriptionStatus::Suspended)
        .await
        .is_err());
    billing
        .transition("t_local", SubscriptionStatus::Active)
        .await
        .unwrap();
    billing
        .transition("t_local", SubscriptionStatus::PastDue)
        .await
        .unwrap();
    billing
        .transition("t_local", SubscriptionStatus::Suspended)
        .await
        .unwrap();
    assert!(matches!(
        billing.authorize_call("t_local", 1).await.unwrap_err(),
        BillingError::PaymentRequired
    ));
    billing
        .transition("t_local", SubscriptionStatus::Active)
        .await
        .unwrap();
    billing
        .transition("t_local", SubscriptionStatus::Cancelled)
        .await
        .unwrap();
    let entitlement = billing.entitlement("t_local").await.unwrap();
    assert_eq!(entitlement.status, "cancelled");
}

#[tokio::test]
async fn stripe_webhook_signature_replay_and_http_topup_wallet() {
    std::env::set_var("LKB_TEST_STRIPE_SECRET", "whsec_test");
    let mut billing_config = Config::default().billing;
    billing_config.stripe_webhook_secret_env = Some("LKB_TEST_STRIPE_SECRET".into());
    billing_config.webhook_tolerance_seconds = 300;
    let provider = StripeProvider::from_env(&billing_config);
    let payload = serde_json::json!({
        "id": "evt_1",
        "type": "checkout.session.completed",
        "data": {"object": {
            "id": "cs_test_1",
            "amount_total": 5_000_000,
            "currency": "usd",
            "metadata": {"tenant_id": "t_local"}
        }}
    });
    let body = serde_json::to_vec(&payload).unwrap();
    let now = chrono::Utc::now().timestamp();
    let good = sign_stripe_webhook("whsec_test", &body, now);
    let event = provider
        .verify_webhook(&WebhookRequest {
            headers: vec![("stripe-signature".into(), good)],
            body: body.clone(),
        })
        .await
        .unwrap();
    assert_eq!(event.event_id, "evt_1");
    assert_eq!(event.tenant_id.as_deref(), Some("t_local"));
    assert_eq!(event.provider_ref.as_deref(), Some("cs_test_1"));
    let bad = sign_stripe_webhook("other", &body, now);
    assert!(provider
        .verify_webhook(&WebhookRequest {
            headers: vec![("stripe-signature".into(), bad)],
            body: body.clone()
        })
        .await
        .is_err());
    let stale = sign_stripe_webhook("whsec_test", &body, now - 10_000);
    assert!(provider
        .verify_webhook(&WebhookRequest {
            headers: vec![("stripe-signature".into(), stale)],
            body
        })
        .await
        .is_err());

    // HTTP: manual topup credits the wallet, manual webhook replay is idempotent
    let temp = tempfile::tempdir().unwrap();
    let config = test_config(&temp.path().join("http-billing.sqlite"));
    let store = store::open(&config).await.unwrap();
    store::bootstrap_space(&store, &config).await.unwrap();
    let app = router(AppState::new(
        store.clone(),
        Arc::new(config.clone()),
        Arc::new(ProviderSet::with_none()),
    ));
    let topup = app.clone().oneshot(Request::builder().method("POST").uri("/api/v1/billing/topup")
        .header(header::HOST, "localhost").header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(r#"{"amount_micros":1000000,"channel":"manual","idempotency_key":"manual:http"}"#)).unwrap()).await.unwrap();
    assert_eq!(topup.status(), StatusCode::OK);
    let webhook_body = r#"{"id":"evt_manual_1","type":"mock.paid","data":{"object":{"id":"sess_http","amount":2000000,"currency":"USD","metadata":{"tenant_id":"t_local"}}}}"#;
    let first = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/billing/webhook/manual")
                .header(header::HOST, "localhost")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(webhook_body))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(first.status(), StatusCode::OK);
    let first_json: serde_json::Value = serde_json::from_slice(
        &axum::body::to_bytes(first.into_body(), 1_000_000)
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(first_json["duplicate"], false);
    let second = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/billing/webhook/manual")
                .header(header::HOST, "localhost")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(webhook_body))
                .unwrap(),
        )
        .await
        .unwrap();
    let second_json: serde_json::Value = serde_json::from_slice(
        &axum::body::to_bytes(second.into_body(), 1_000_000)
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(second_json["duplicate"], true);
    let wallet = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v1/billing/wallet")
                .header(header::HOST, "localhost")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let wallet_json: serde_json::Value = serde_json::from_slice(
        &axum::body::to_bytes(wallet.into_body(), 1_000_000)
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(wallet_json["wallet"]["balance_micros"], 3_000_000);
    let refund_body = r#"{"id":"evt_refund_1","type":"charge.refunded","data":{"object":{"amount":1000000,"currency":"USD","metadata":{"tenant_id":"t_local"}}}}"#;
    let refund = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/billing/webhook/manual")
                .header(header::HOST, "localhost")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(refund_body))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(refund.status(), StatusCode::OK);
    let refund_json: serde_json::Value = serde_json::from_slice(
        &axum::body::to_bytes(refund.into_body(), 1_000_000)
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(refund_json["handled"], "refunded");
    let refund_replay = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/billing/webhook/manual")
                .header(header::HOST, "localhost")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(refund_body))
                .unwrap(),
        )
        .await
        .unwrap();
    let replay_json: serde_json::Value = serde_json::from_slice(
        &axum::body::to_bytes(refund_replay.into_body(), 1_000_000)
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(replay_json["duplicate"], true);
    let after_refund = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v1/billing/wallet")
                .header(header::HOST, "localhost")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let after_json: serde_json::Value = serde_json::from_slice(
        &axum::body::to_bytes(after_refund.into_body(), 1_000_000)
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(after_json["wallet"]["balance_micros"], 2_000_000);
    let ledger = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v1/billing/ledger?limit=100")
                .header(header::HOST, "localhost")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let ledger_json: serde_json::Value = serde_json::from_slice(
        &axum::body::to_bytes(ledger.into_body(), 1_000_000)
            .await
            .unwrap(),
    )
    .unwrap();
    let entries = ledger_json["entries"].as_array().unwrap();
    let debit: i64 = entries
        .iter()
        .filter(|e| e["direction"] == "debit")
        .map(|e| e["amount_micros"].as_i64().unwrap())
        .sum();
    let credit: i64 = entries
        .iter()
        .filter(|e| e["direction"] == "credit")
        .map(|e| e["amount_micros"].as_i64().unwrap())
        .sum();
    assert_eq!(debit, credit);

    // Gateway blocks calls when the wallet cannot cover the estimate (402)
    let base = start_provider().await;
    let mut gate_config = test_config(&temp.path().join("gate.sqlite"));
    gate_config.payment.provider = "manual".into();
    let gate_store = store::open(&gate_config).await.unwrap();
    store::bootstrap_space(&gate_store, &gate_config)
        .await
        .unwrap();
    wire_alias(&gate_store, &base).await;
    // an existing zero-balance wallet makes billing active for this tenant
    lkb::billing::Billing::new(gate_store.clone(), gate_config.billing.clone())
        .wallet("t_local")
        .await
        .unwrap();
    let gateway = lkb::gateway::Gateway::new(gate_store.clone());
    let request = lkb::gateway::GatewayRequest {
        request_id: "req_billing_gate".into(),
        principal: lkb::governance::Principal::local_user(
            "alice",
            "t_local",
            vec!["s_default".into()],
            vec![],
        ),
        space_id: Some("s_default".into()),
        role: "user".into(),
        alias: "default".into(),
        messages: vec![lkb::gateway::ChatMessage {
            role: "user".into(),
            content: "hello".into(),
        }],
        max_output_tokens: 8,
        temperature: 0.0,
        source: "kb".into(),
        rewritten_query: None,
    };
    let error = gateway.chat(&request).await.unwrap_err();
    assert_eq!(error.status, StatusCode::PAYMENT_REQUIRED);
    assert_eq!(error.code, "payment_required");
}

#[tokio::test]
async fn external_engine_does_not_debit_wallet() {
    let temp = tempfile::tempdir().unwrap();
    let config = test_config(&temp.path().join("external.sqlite"));
    let store = store::open(&config).await.unwrap();
    store::bootstrap_space(&store, &config).await.unwrap();
    let billing = Billing::new(store.clone(), config.billing.clone());
    let topup = billing
        .topup("t_local", 1_000_000, "manual", None, "manual:ext")
        .await
        .unwrap();
    billing.settle_topup(&topup).await.unwrap();
    let before = billing.wallet("t_local").await.unwrap().balance_micros;
    let ledger_before = store
        .billing_list_ledger("t_local", 100)
        .await
        .unwrap()
        .len();
    let base = start_provider().await;
    wire_alias(&store, &base).await;
    let gateway = lkb::gateway::Gateway::new(store.clone()).with_local_ledger(false);
    let request = lkb::gateway::GatewayRequest {
        request_id: "req_external".into(),
        principal: lkb::governance::Principal::local_user(
            "alice",
            "t_local",
            vec!["s_default".into()],
            vec![],
        ),
        space_id: Some("s_default".into()),
        role: "user".into(),
        alias: "default".into(),
        messages: vec![lkb::gateway::ChatMessage {
            role: "user".into(),
            content: "hello".into(),
        }],
        max_output_tokens: 8,
        temperature: 0.0,
        source: "kb".into(),
        rewritten_query: None,
    };
    gateway.chat(&request).await.unwrap();
    assert_eq!(
        billing.wallet("t_local").await.unwrap().balance_micros,
        before
    );
    assert_eq!(
        store
            .billing_list_ledger("t_local", 100)
            .await
            .unwrap()
            .len(),
        ledger_before
    );
}

#[tokio::test]
async fn subscription_http_and_entitlement() {
    let temp = tempfile::tempdir().unwrap();
    let config = test_config(&temp.path().join("sub-http.sqlite"));
    let store = store::open(&config).await.unwrap();
    store::bootstrap_space(&store, &config).await.unwrap();
    let app = router(AppState::new(
        store.clone(),
        Arc::new(config.clone()),
        Arc::new(ProviderSet::with_none()),
    ));
    let subscribe = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/billing/subscribe")
                .header(header::HOST, "localhost")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(r#"{"plan_id":"starter","trial":true}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(subscribe.status(), StatusCode::OK);
    let subscribe_json: serde_json::Value = serde_json::from_slice(
        &axum::body::to_bytes(subscribe.into_body(), 1_000_000)
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(subscribe_json["subscription"]["status"], "trial");
    let get = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v1/billing/subscription")
                .header(header::HOST, "localhost")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let get_json: serde_json::Value = serde_json::from_slice(
        &axum::body::to_bytes(get.into_body(), 1_000_000)
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(get_json["subscription"]["plan_id"], "starter");
    assert_eq!(get_json["entitlement"]["included_tokens"], 5_000_000);
    let activate = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PATCH")
                .uri("/api/v1/billing/subscription")
                .header(header::HOST, "localhost")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(r#"{"status":"active"}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(activate.status(), StatusCode::OK);
    let illegal = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PATCH")
                .uri("/api/v1/billing/subscription")
                .header(header::HOST, "localhost")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(r#"{"status":"cancelled"}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(illegal.status(), StatusCode::OK);
    let illegal2 = app
        .oneshot(
            Request::builder()
                .method("PATCH")
                .uri("/api/v1/billing/subscription")
                .header(header::HOST, "localhost")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(r#"{"status":"active"}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(illegal2.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn judge_respects_external_engine_mode() {
    let temp = tempfile::tempdir().unwrap();
    let config = test_config(&temp.path().join("judge-billing.sqlite"));
    let store = store::open(&config).await.unwrap();
    store::bootstrap_space(&store, &config).await.unwrap();
    let docs = temp.path().join("docs");
    std::fs::create_dir_all(&docs).unwrap();
    std::fs::write(docs.join("01.md"), "# Judge\n\njudge sentence alpha.").unwrap();
    lkb::ingest::import_path(
        &store,
        &docs,
        &lkb::ingest::ImportOptions {
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
    let base = start_provider().await;
    wire_alias(&store, &base).await;
    let billing = Billing::new(store.clone(), config.billing.clone());
    let topup = billing
        .topup("t_local", 1_000_000, "manual", None, "manual:judge")
        .await
        .unwrap();
    billing.settle_topup(&topup).await.unwrap();
    let balance_before = billing.wallet("t_local").await.unwrap().balance_micros;
    let ledger_before = store
        .billing_list_ledger("t_local", 100)
        .await
        .unwrap()
        .len();
    let policy = lkb::governance::Policy::new(
        lkb::governance::Principal::local_user(
            "alice",
            "t_local",
            vec!["s_default".into()],
            vec![],
        ),
        None,
    )
    .unwrap();
    let tuning = lkb::search::SearchTuning {
        judge_alias: "default".into(),
        billing_local_ledger: false,
        ..Default::default()
    };
    let response = lkb::search::search_with_tuning(
        &store,
        &ProviderSet::with_none(),
        &policy,
        "judge",
        lkb::search::SearchMode::Bm25Judge,
        5,
        &tuning,
    )
    .await
    .unwrap();
    assert!(!response.hits.is_empty());
    assert_eq!(
        billing.wallet("t_local").await.unwrap().balance_micros,
        balance_before
    );
    assert_eq!(
        store
            .billing_list_ledger("t_local", 100)
            .await
            .unwrap()
            .len(),
        ledger_before
    );
    let usage = store
        .list_usage_events(&lkb::store::UsageFilter {
            tenant_id: "t_local".into(),
            limit: 50,
            ..Default::default()
        })
        .await
        .unwrap();
    assert!(usage.iter().any(|event| event.source == "judge"));
}
