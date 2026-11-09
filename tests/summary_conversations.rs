use axum::body::Body;
use axum::http::{header, Request, StatusCode};
use axum::routing::post;
use axum::{Json, Router};
use lkb::config::Config;
use lkb::governance::{Policy, Principal, UserRole};
use lkb::ingest::{import_path, ImportOptions};
use lkb::providers::ProviderSet;
use lkb::search::{search_with_tuning, SearchMode, SearchTuning};
use lkb::store::{
    self, ModelAliasRecord, ProviderModelRecord, ProviderRecord, RouteRecord, Store, TokenRecord,
    UsageFilter,
};
use lkb::util::{now_rfc3339, random_hex, sha256_hex_str};
use lkb::web::{router, AppState};
use std::sync::Arc;
use tokio::net::TcpListener;
use tower::ServiceExt;

async fn text_provider(Json(body): Json<serde_json::Value>) -> axum::response::Response {
    use axum::response::IntoResponse;
    let prompt = body["messages"][0]["content"].as_str().unwrap_or_default();
    let content = if prompt.contains("Rewrite the follow-up") {
        "standalone rewritten query".to_string()
    } else if prompt.contains("Score each candidate") {
        let count = prompt.matches("\n[").count().max(1);
        let scores: Vec<f64> = (0..count)
            .map(|index| if index == 0 { 0.1 } else { 0.9 })
            .collect();
        format!("{{\"scores\":{scores:?}}}")
    } else {
        "{\"answer\":\"gateway answer with enough words to split into several stream tokens\",\"citations\":[1]}".to_string()
    };
    if body["stream"].as_bool() == Some(true) {
        let chunk = |text: &str| {
            format!("data: {{\"choices\":[{{\"delta\":{{\"content\":{text:?}}}}}]}}\n\n")
        };
        let mut stream = String::new();
        for part in content.split_inclusive(' ').collect::<Vec<_>>().chunks(3) {
            stream.push_str(&chunk(&part.concat()));
        }
        stream.push_str("data: [DONE]\n\n");
        return axum::response::Response::builder()
            .header("content-type", "text/event-stream")
            .body(axum::body::Body::from(stream))
            .unwrap();
    }
    Json(serde_json::json!({
        "choices": [{"message": {"role": "assistant", "content": content}}],
        "usage": {"prompt_tokens": 2, "completion_tokens": 2}
    }))
    .into_response()
}

async fn start_text_provider() -> String {
    let app = Router::new().route("/text/chat/completions", post(text_provider));
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    format!("http://{addr}/text")
}

async fn summary_provider() -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "choices": [{"message": {"role": "assistant", "content": "{\"tldr\":\"Provider summary\",\"bullets\":[\"one\",\"two\"],\"keywords\":[\"hello\"],\"entities\":[],\"lang\":\"zh\"}"}}],
        "usage": {"prompt_tokens": 9, "completion_tokens": 5}
    }))
}

async fn start_summary_provider() -> String {
    let app = Router::new().route("/summary/chat/completions", post(summary_provider));
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    format!("http://{addr}/summary")
}

fn test_config(path: &std::path::Path) -> Config {
    let mut config = Config::default();
    config.storage.sqlite_path = path.to_path_buf();
    config.storage.tenant_id = "t_local".into();
    config.storage.default_space = "s_default".into();
    config
}

async fn wire_alias(store: &Arc<dyn Store>, base: &str) {
    let now = now_rfc3339();
    store
        .upsert_provider(
            "t_local",
            &ProviderRecord {
                provider_id: "p_summary".into(),
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
                provider_model_id: "pm_summary".into(),
                provider_id: "p_summary".into(),
                upstream_model: "summary-model".into(),
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
                route_id: "r_summary".into(),
                alias_public_id: "default".into(),
                provider_model_id: "pm_summary".into(),
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

async fn import_one(store: &Arc<dyn Store>, dir: &std::path::Path, name: &str, text: &str) {
    std::fs::create_dir_all(dir).unwrap();
    std::fs::write(dir.join(name), text).unwrap();
    import_path(
        store,
        dir,
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
}

#[tokio::test]
async fn summary_job_uses_gateway_and_writes_summary_usage() {
    let temp = tempfile::tempdir().unwrap();
    let config = test_config(&temp.path().join("summary.sqlite"));
    let store = store::open(&config).await.unwrap();
    let base = start_summary_provider().await;
    wire_alias(&store, &base).await;
    import_one(
        &store,
        &temp.path().join("docs"),
        "01.md",
        "# Hello\n\nFirst sentence. Second sentence.",
    )
    .await;
    let processed = lkb::summary::process_pending(&store, &config, 10).await;
    assert!(processed >= 1);
    let jobs = store.list_summary_jobs("ready", 10).await.unwrap();
    assert_eq!(jobs.len(), 1);
    let summaries = store
        .list_documents("t_local", Some("s_default"))
        .await
        .unwrap();
    assert_eq!(summaries.len(), 1);
    let summary = store
        .get_document_summary(
            "t_local",
            "s_default",
            &summaries[0].doc_id,
            &summaries[0].content_hash,
            "default",
            "summary-v1",
        )
        .await
        .unwrap()
        .expect("summary");
    assert_eq!(summary.status, "ready");
    assert!(summary.summary_json.contains("Provider summary"));
    let usage = store
        .list_usage_events(&UsageFilter {
            tenant_id: "t_local".into(),
            limit: 10,
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(usage.len(), 1);
    assert_eq!(usage[0].source, "summary");
    assert_eq!(usage[0].alias, "default");
}

#[tokio::test]
async fn summary_fallback_is_extractive_and_job_is_idempotent() {
    let temp = tempfile::tempdir().unwrap();
    let config = test_config(&temp.path().join("fallback.sqlite"));
    let store = store::open(&config).await.unwrap();
    let dir = temp.path().join("docs");
    import_one(
        &store,
        &dir,
        "01.md",
        "# Intro\n\nFirst sentence. Second sentence. Third sentence.",
    )
    .await;
    let before = store.list_summary_jobs("queued", 10).await.unwrap();
    assert_eq!(before.len(), 1);
    let processed = lkb::summary::process_pending(&store, &config, 10).await;
    assert_eq!(processed, 1);
    let summaries = store
        .list_documents("t_local", Some("s_default"))
        .await
        .unwrap();
    let summary = store
        .get_document_summary(
            "t_local",
            "s_default",
            &summaries[0].doc_id,
            &summaries[0].content_hash,
            "extractive",
            "summary-v1",
        )
        .await
        .unwrap()
        .expect("fallback summary");
    assert_eq!(summary.model, "extractive");
    assert!(summary.summary_json.contains("\"fallback\":true"));
    // enqueueing the same revision again must not create a second job
    lkb::summary::enqueue(&store, &summaries[0], &summaries[0].content_hash).await;
    assert_eq!(
        store.list_summary_jobs("queued", 10).await.unwrap().len(),
        0
    );
}

#[tokio::test]
async fn query_modes_share_conversation_and_history_is_owner_scoped() {
    let temp = tempfile::tempdir().unwrap();
    let config = test_config(&temp.path().join("conv.sqlite"));
    let store = store::open(&config).await.unwrap();
    store::bootstrap_space(&store, &config).await.unwrap();
    import_one(
        &store,
        &temp.path().join("docs"),
        "01.md",
        "# Hello\n\nDocument sentence for search.",
    )
    .await;
    let app = router(AppState::new(
        store.clone(),
        Arc::new(config.clone()),
        Arc::new(ProviderSet::with_none()),
    ));
    let search = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/query")
                .header(header::HOST, "localhost")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(r#"{"query":"Hello","mode":"search"}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(search.status(), StatusCode::OK);
    let search_json: serde_json::Value = serde_json::from_slice(
        &axum::body::to_bytes(search.into_body(), 1_000_000)
            .await
            .unwrap(),
    )
    .unwrap();
    let conversation_id = search_json["conversation_id"].as_str().unwrap().to_string();
    assert!(search_json["answer"].is_null());

    let both = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/query")
                .header(header::HOST, "localhost")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(format!(
                    r#"{{"query":"Hello","mode":"both","conversation_id":"{conversation_id}"}}"#
                )))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(both.status(), StatusCode::OK);
    let both_json: serde_json::Value = serde_json::from_slice(
        &axum::body::to_bytes(both.into_body(), 1_000_000)
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(both_json["conversation_id"], conversation_id);
    assert!(both_json["answer"]["answer"].is_string());

    let detail = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/api/v1/conversations/{conversation_id}"))
                .header(header::HOST, "localhost")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(detail.status(), StatusCode::OK);
    let detail_json: serde_json::Value = serde_json::from_slice(
        &axum::body::to_bytes(detail.into_body(), 1_000_000)
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(detail_json["messages"].as_array().unwrap().len(), 4);
    let delete = app
        .clone()
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri(format!("/api/v1/conversations/{conversation_id}"))
                .header(header::HOST, "localhost")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(delete.status(), StatusCode::OK);

    store
        .add_user_with_role("t_local", "other", "x", UserRole::User)
        .await
        .unwrap();
    let other = store.get_user("t_local", "other").await.unwrap().unwrap();
    let raw = format!("lkb_{}", random_hex(12));
    store
        .create_token(&TokenRecord {
            token_hash: sha256_hex_str(&raw),
            token_id: format!("tok_{}", random_hex(6)),
            tenant_id: "t_local".into(),
            user_id: other.user_id,
            scope: lkb::governance::TokenScope::Read,
            kind: lkb::governance::PrincipalKind::Human,
            label: "other".into(),
            created_at: now_rfc3339(),
            expires_at: None,
            revoked_at: None,
        })
        .await
        .unwrap();
    let denied = app
        .oneshot(
            Request::builder()
                .uri(format!("/api/v1/conversations/{conversation_id}"))
                .header(header::HOST, "localhost")
                .header(header::AUTHORIZATION, format!("Bearer {raw}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(denied.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn summary_retries_are_bounded_and_final_failure_is_recorded() {
    let temp = tempfile::tempdir().unwrap();
    let mut config = test_config(&temp.path().join("retry.sqlite"));
    config.summary.max_attempts = 2;
    config.summary.retry_backoff_ms = 1;
    let store = store::open(&config).await.unwrap();
    import_one(
        &store,
        &temp.path().join("docs"),
        "01.md",
        "# Retry\n\nOnly sentence here.",
    )
    .await;
    let processed = lkb::summary::process_pending(&store, &config, 10).await;
    assert_eq!(processed, 1);
    let jobs = store.list_summary_jobs("failed", 10).await.unwrap();
    assert_eq!(jobs.len(), 1);
    assert_eq!(jobs[0].attempts, 2);
    assert!(jobs[0].error.is_some());
    assert_eq!(
        store.list_summary_jobs("queued", 10).await.unwrap().len(),
        0
    );
    let documents = store
        .list_documents("t_local", Some("s_default"))
        .await
        .unwrap();
    let summary = store
        .get_document_summary(
            "t_local",
            "s_default",
            &documents[0].doc_id,
            &documents[0].content_hash,
            "extractive",
            "summary-v1",
        )
        .await
        .unwrap()
        .expect("extractive fallback must be preserved");
    assert_eq!(summary.status, "failed");
    assert!(summary.summary_json.contains("\"fallback\":true"));
}

#[tokio::test]
async fn query_rewrites_with_conversation_history_via_gateway() {
    let temp = tempfile::tempdir().unwrap();
    let config = test_config(&temp.path().join("rewrite.sqlite"));
    let store = store::open(&config).await.unwrap();
    store::bootstrap_space(&store, &config).await.unwrap();
    let base = start_text_provider().await;
    wire_alias(&store, &base).await;
    import_one(
        &store,
        &temp.path().join("docs"),
        "01.md",
        "# Hello\n\nDocument sentence for search.",
    )
    .await;
    let app = router(AppState::new(
        store.clone(),
        Arc::new(config.clone()),
        Arc::new(ProviderSet::with_none()),
    ));
    let search = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/query")
                .header(header::HOST, "localhost")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(r#"{"query":"Hello","mode":"search"}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    let search_json: serde_json::Value = serde_json::from_slice(
        &axum::body::to_bytes(search.into_body(), 1_000_000)
            .await
            .unwrap(),
    )
    .unwrap();
    let conversation_id = search_json["conversation_id"].as_str().unwrap().to_string();
    let both = app.clone().oneshot(Request::builder().method("POST").uri("/api/v1/query")
        .header(header::HOST, "localhost").header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(format!(r#"{{"query":"follow up question","mode":"both","conversation_id":"{conversation_id}"}}"#))).unwrap()).await.unwrap();
    assert_eq!(both.status(), StatusCode::OK);
    let both_json: serde_json::Value = serde_json::from_slice(
        &axum::body::to_bytes(both.into_body(), 1_000_000)
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(both_json["rewritten_query"], "standalone rewritten query");
    assert!(
        both_json["conversation_context_hash"]
            .as_str()
            .unwrap()
            .len()
            > 10
    );
    assert!(both_json["degraded"].as_array().unwrap().is_empty());
    let detail = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/api/v1/conversations/{conversation_id}"))
                .header(header::HOST, "localhost")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let detail_json: serde_json::Value = serde_json::from_slice(
        &axum::body::to_bytes(detail.into_body(), 1_000_000)
            .await
            .unwrap(),
    )
    .unwrap();
    let messages = detail_json["messages"].as_array().unwrap();
    assert!(messages
        .iter()
        .any(|message| message["rewritten_query"] == "standalone rewritten query"));
    let usage = store
        .list_usage_events(&UsageFilter {
            tenant_id: "t_local".into(),
            limit: 20,
            ..Default::default()
        })
        .await
        .unwrap();
    assert!(usage.iter().any(|event| event.source == "kb"
        && event.rewritten_query.as_deref() == Some("standalone rewritten query")));
}

#[tokio::test]
async fn query_stream_emits_sse_events_in_order() {
    let temp = tempfile::tempdir().unwrap();
    let config = test_config(&temp.path().join("stream.sqlite"));
    let store = store::open(&config).await.unwrap();
    store::bootstrap_space(&store, &config).await.unwrap();
    let base = start_text_provider().await;
    wire_alias(&store, &base).await;
    import_one(
        &store,
        &temp.path().join("docs"),
        "01.md",
        "# Hello\n\nDocument sentence for search.",
    )
    .await;
    let app = router(AppState::new(
        store.clone(),
        Arc::new(config.clone()),
        Arc::new(ProviderSet::with_none()),
    ));
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/query/stream")
                .header(header::HOST, "localhost")
                .header(header::CONTENT_TYPE, "application/json")
                .header(header::ACCEPT, "text/event-stream")
                .body(Body::from(r#"{"query":"Hello","mode":"both"}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert!(response.headers()[header::CONTENT_TYPE]
        .to_str()
        .unwrap()
        .starts_with("text/event-stream"));
    let body = String::from_utf8(
        axum::body::to_bytes(response.into_body(), 1_000_000)
            .await
            .unwrap()
            .to_vec(),
    )
    .unwrap();
    let positions = [
        "event: meta",
        "event: results",
        "event: token",
        "event: usage",
        "event: done",
    ]
    .map(|event| {
        body.find(event)
            .unwrap_or_else(|| panic!("missing {event} in {body}"))
    });
    assert!(positions.windows(2).all(|pair| pair[0] < pair[1]));
    assert!(
        body.matches("event: token").count() >= 2,
        "expected multiple token events: {body}"
    );
}

#[tokio::test]
async fn conversations_support_cursor_pagination_and_external_session() {
    let temp = tempfile::tempdir().unwrap();
    let config = test_config(&temp.path().join("pages.sqlite"));
    let store = store::open(&config).await.unwrap();
    store::bootstrap_space(&store, &config).await.unwrap();
    import_one(
        &store,
        &temp.path().join("docs"),
        "01.md",
        "# Hello\n\nDocument sentence for search.",
    )
    .await;
    let app = router(AppState::new(
        store.clone(),
        Arc::new(config.clone()),
        Arc::new(ProviderSet::with_none()),
    ));
    let create = |app: axum::Router| async move {
        app.oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/query")
                .header(header::HOST, "localhost")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(r#"{"query":"Hello","mode":"search"}"#))
                .unwrap(),
        )
        .await
        .unwrap()
    };
    let first = create(app.clone()).await;
    let first_json: serde_json::Value = serde_json::from_slice(
        &axum::body::to_bytes(first.into_body(), 1_000_000)
            .await
            .unwrap(),
    )
    .unwrap();
    let first_id = first_json["conversation_id"].as_str().unwrap().to_string();
    tokio::time::sleep(std::time::Duration::from_millis(1_100)).await;
    let second = create(app.clone()).await;
    let second_json: serde_json::Value = serde_json::from_slice(
        &axum::body::to_bytes(second.into_body(), 1_000_000)
            .await
            .unwrap(),
    )
    .unwrap();
    let second_id = second_json["conversation_id"].as_str().unwrap().to_string();
    let page = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v1/conversations?limit=1")
                .header(header::HOST, "localhost")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let page_json: serde_json::Value = serde_json::from_slice(
        &axum::body::to_bytes(page.into_body(), 1_000_000)
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(page_json["conversations"].as_array().unwrap().len(), 1);
    assert_eq!(page_json["conversations"][0]["conversation_id"], second_id);
    let cursor = page_json["next_cursor"]
        .as_str()
        .expect("next_cursor")
        .to_string();
    let next = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/api/v1/conversations?limit=1&cursor={cursor}"))
                .header(header::HOST, "localhost")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let next_json: serde_json::Value = serde_json::from_slice(
        &axum::body::to_bytes(next.into_body(), 1_000_000)
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(next_json["conversations"][0]["conversation_id"], first_id);
    let bind = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/v1/conversations/{first_id}/external-session"))
                .header(header::HOST, "localhost")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(r#"{"external_session_id":"zeta-session-1"}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(bind.status(), StatusCode::OK);
    let lookup = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/conversations?external_session_id=zeta-session-1")
                .header(header::HOST, "localhost")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let lookup_json: serde_json::Value = serde_json::from_slice(
        &axum::body::to_bytes(lookup.into_body(), 1_000_000)
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(lookup_json["conversations"].as_array().unwrap().len(), 1);
    assert_eq!(lookup_json["conversations"][0]["conversation_id"], first_id);
}

#[tokio::test]
async fn judge_uses_gateway_alias_and_records_usage() {
    let temp = tempfile::tempdir().unwrap();
    let config = test_config(&temp.path().join("judge.sqlite"));
    let store = store::open(&config).await.unwrap();
    store::bootstrap_space(&store, &config).await.unwrap();
    let base = start_text_provider().await;
    wire_alias(&store, &base).await;
    import_one(
        &store,
        &temp.path().join("docs"),
        "01.md",
        "# Alpha\n\nAlpha sentence one.",
    )
    .await;
    import_one(
        &store,
        &temp.path().join("docs"),
        "02.md",
        "# Beta\n\nBeta sentence two.",
    )
    .await;
    let policy = Policy::new(
        Principal::local_user("alice", "t_local", vec!["s_default".into()], vec![]),
        None,
    )
    .unwrap();
    let response = search_with_tuning(
        &store,
        &ProviderSet::with_none(),
        &policy,
        "sentence",
        SearchMode::Bm25Judge,
        10,
        &SearchTuning::default(),
    )
    .await
    .unwrap();
    assert!(response.hits.len() >= 2);
    assert!(response.hits.iter().any(|hit| hit.rerank_score.is_some()));
    let usage = store
        .list_usage_events(&UsageFilter {
            tenant_id: "t_local".into(),
            limit: 20,
            ..Default::default()
        })
        .await
        .unwrap();
    assert!(usage
        .iter()
        .any(|event| event.source == "judge" && event.alias == "default"));
}

#[tokio::test]
async fn stream_disconnect_cancels_upstream_and_records_cancelled_usage() {
    let temp = tempfile::tempdir().unwrap();
    let config = test_config(&temp.path().join("disconnect.sqlite"));
    let store = store::open(&config).await.unwrap();
    let base = start_text_provider().await;
    wire_alias(&store, &base).await;
    let (tx, rx) = tokio::sync::mpsc::channel::<String>(1);
    drop(rx);
    let request = lkb::gateway::GatewayRequest {
        request_id: format!("req_{}", random_hex(8)),
        principal: Principal::local_user("alice", "t_local", vec!["s_default".into()], vec![]),
        space_id: Some("s_default".into()),
        role: "user".into(),
        alias: "default".into(),
        messages: vec![lkb::gateway::ChatMessage {
            role: "user".into(),
            content: "stream please".into(),
        }],
        max_output_tokens: 32,
        temperature: 0.0,
        source: "kb".into(),
        rewritten_query: None,
    };
    let error = lkb::gateway::Gateway::new(store.clone())
        .chat_stream(&request, tx)
        .await
        .unwrap_err();
    assert_eq!(error.code, "client_disconnected");
    let usage = store
        .list_usage_events(&UsageFilter {
            tenant_id: "t_local".into(),
            limit: 10,
            ..Default::default()
        })
        .await
        .unwrap();
    assert!(
        usage.iter().any(|event| event.status == "cancelled"),
        "usage={usage:?}"
    );
}

#[tokio::test]
async fn non_stream_upstream_fallback_is_degraded() {
    let temp = tempfile::tempdir().unwrap();
    let config = test_config(&temp.path().join("fallback-stream.sqlite"));
    let store = store::open(&config).await.unwrap();
    store::bootstrap_space(&store, &config).await.unwrap();
    let base = start_summary_provider().await;
    wire_alias(&store, &base).await;
    import_one(
        &store,
        &temp.path().join("docs"),
        "01.md",
        "# Hello\n\nFirst sentence. Second sentence.",
    )
    .await;
    let policy = Policy::new(p1_policy_principal(), None).unwrap();
    let (tx, mut rx) = tokio::sync::mpsc::channel::<String>(16);
    let answer = lkb::answer::build_answer_stream(
        &store,
        &ProviderSet::with_none(),
        &policy,
        "Hello",
        &lkb::answer::AnswerOptions::default(),
        tx,
    )
    .await
    .unwrap();
    while rx.recv().await.is_some() {}
    assert!(
        answer
            .degraded
            .iter()
            .any(|item| item.code == "stream_fallback"),
        "degraded={:?}",
        answer.degraded
    );
}

fn p1_policy_principal() -> Principal {
    Principal::local_user("alice", "t_local", vec!["s_default".into()], vec![])
}
