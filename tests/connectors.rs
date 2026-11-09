use axum::body::Body;
use axum::http::{header, Request, StatusCode};
use axum::{Json, Router};
use lkb::config::Config;
use lkb::connectors;
use lkb::governance::{Acl, Policy, Principal};
use lkb::providers::ProviderSet;
use lkb::store::{self, ConnectorJobRecord, Store};
use lkb::web::{router, AppState};
use std::sync::Arc;
use tower::ServiceExt;

fn test_config(path: &std::path::Path) -> Config {
    let mut config = Config::default();
    config.storage.sqlite_path = path.to_path_buf();
    config.storage.tenant_id = "t_local".into();
    config.storage.default_space = "s_default".into();
    config.features.connectors = Some(true);
    config
}

fn policy() -> Policy {
    Policy::new(
        Principal::local_user("alice", "t_local", vec!["s_default".into()], vec![]),
        None,
    )
    .unwrap()
}

async fn create_connector(
    app: &Router,
    kind: &str,
    name: &str,
    config: serde_json::Value,
) -> String {
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/connectors")
                .header(header::HOST, "localhost")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    serde_json::json!({
                        "space_id": "s_default", "kind": kind, "name": name, "config": config
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK, "create {kind}");
    let json: serde_json::Value = serde_json::from_slice(
        &axum::body::to_bytes(response.into_body(), 1_000_000)
            .await
            .unwrap(),
    )
    .unwrap();
    json["connector"]["connector_id"]
        .as_str()
        .unwrap()
        .to_string()
}

async fn run_job(
    store: &Arc<dyn Store>,
    config: &Arc<Config>,
    connector_id: &str,
) -> ConnectorJobRecord {
    let record = store
        .connector_get("t_local", connector_id)
        .await
        .unwrap()
        .unwrap();
    let job = ConnectorJobRecord {
        job_id: format!("cjob_test_{}", lkb::util::random_hex(6)),
        connector_id: record.connector_id.clone(),
        status: "queued".into(),
        started_at: lkb::util::now_rfc3339(),
        finished_at: None,
        stats_json: "{}".into(),
        error: None,
    };
    store.connector_job_create(&job).await.unwrap();
    connectors::run_sync(
        store.clone(),
        config.clone(),
        Arc::new(ProviderSet::with_none()),
        record,
        job,
    )
    .await
}

#[tokio::test]
async fn filesystem_export_sync_deep_links_acl_and_idempotency() {
    let temp = tempfile::tempdir().unwrap();
    let config = test_config(&temp.path().join("connectors.sqlite"));
    let store = store::open(&config).await.unwrap();
    store::bootstrap_space(&store, &config).await.unwrap();
    let manifest_dir = temp.path().join("export");
    std::fs::create_dir_all(&manifest_dir).unwrap();
    std::fs::write(
        manifest_dir.join("manifest.json"),
        serde_json::json!({
            "items": [
                {
                    "external_id": "doc-1",
                    "url": "https://example.invalid/doc-1",
                    "title": "External One",
                    "revision": "rev-1",
                    "acl": {"mode": "allow_all", "users": [], "groups": [], "version": 1},
                    "content": "# External One\n\nexternalconnector token alpha."
                },
                {
                    "external_id": "ref-only",
                    "url": "https://example.invalid/ref-only",
                    "title": "Ref Only"
                }
            ]
        })
        .to_string(),
    )
    .unwrap();
    let app = router(AppState::new(
        store.clone(),
        Arc::new(config.clone()),
        Arc::new(ProviderSet::with_none()),
    ));
    let connector_id = create_connector(
        &app,
        "filesystem_export",
        "files-export",
        serde_json::json!({"dir": manifest_dir}),
    )
    .await;

    let test = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/v1/connectors/{connector_id}/test"))
                .header(header::HOST, "localhost")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let test_json: serde_json::Value = serde_json::from_slice(
        &axum::body::to_bytes(test.into_body(), 1_000_000)
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(test_json["ok"], true);
    assert_eq!(test_json["details"]["items"], 2);

    let job = run_job(&store, &Arc::new(config.clone()), &connector_id).await;
    assert_eq!(job.status, "completed");
    let stats: serde_json::Value = serde_json::from_str(&job.stats_json).unwrap();
    assert_eq!(stats["items"], 2);
    assert_eq!(stats["indexed"], 1);
    assert_eq!(stats["refs_only"], 1);
    assert_eq!(stats["failed"], 0);

    let refs = store
        .external_ref_list("t_local", None, Some("filesystem_export"), 10)
        .await
        .unwrap();
    assert_eq!(refs.len(), 2);
    let doc_ref = refs
        .iter()
        .find(|reference| reference.external_id == "doc-1")
        .unwrap();
    assert_eq!(doc_ref.url, "https://example.invalid/doc-1");
    assert_eq!(doc_ref.revision.as_deref(), Some("rev-1"));
    assert!(!doc_ref.doc_id.is_empty());
    let acl = Acl::from_json(&doc_ref.acl_json).unwrap();
    assert_eq!(acl.to_json(), Acl::allow_all().to_json());
    let ref_only = refs
        .iter()
        .find(|reference| reference.external_id == "ref-only")
        .unwrap();
    assert_eq!(ref_only.doc_id, "");
    let unknown_acl = Acl::from_json(&ref_only.acl_json).unwrap();
    assert_ne!(
        unknown_acl.to_json(),
        Acl::allow_all().to_json(),
        "missing source ACL must deny"
    );

    let hits = lkb::search::search(
        &store,
        &ProviderSet::with_none(),
        &policy(),
        "externalconnector",
        lkb::search::SearchMode::Bm25,
        10,
    )
    .await
    .unwrap();
    assert_eq!(hits.hits.len(), 1);
    assert!(hits.hits[0]
        .path
        .contains("connectors/filesystem_export/doc-1.md"));

    let refs_api = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/api/v1/external-refs?doc_id={}", doc_ref.doc_id))
                .header(header::HOST, "localhost")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(refs_api.status(), StatusCode::OK);
    let refs_json: serde_json::Value = serde_json::from_slice(
        &axum::body::to_bytes(refs_api.into_body(), 1_000_000)
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(refs_json["external_refs"].as_array().unwrap().len(), 1);

    // idempotent second sync: same document/ref counts
    let documents_before = store
        .list_documents("t_local", Some("s_default"))
        .await
        .unwrap()
        .len();
    let job2 = run_job(&store, &Arc::new(config), &connector_id).await;
    assert_eq!(job2.status, "completed");
    assert_eq!(
        store
            .list_documents("t_local", Some("s_default"))
            .await
            .unwrap()
            .len(),
        documents_before
    );
    assert_eq!(
        store
            .external_ref_list("t_local", None, Some("filesystem_export"), 10)
            .await
            .unwrap()
            .len(),
        2
    );
}

async fn fake_http_items() -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "items": [{
            "external_id": "http-1",
            "url": "https://example.invalid/http-1",
            "title": "HTTP Item",
            "acl": {"mode": "allow_all", "users": [], "groups": [], "version": 1},
            "content": "# HTTP\n\nhttpconnector beta."
        }]
    }))
}

#[tokio::test]
async fn http_json_sync_and_stub_not_configured() {
    let temp = tempfile::tempdir().unwrap();
    let config = test_config(&temp.path().join("http-connectors.sqlite"));
    let store = store::open(&config).await.unwrap();
    store::bootstrap_space(&store, &config).await.unwrap();
    let provider = Router::new().route("/items", axum::routing::get(fake_http_items));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, provider).await.unwrap() });
    let app = router(AppState::new(
        store.clone(),
        Arc::new(config.clone()),
        Arc::new(ProviderSet::with_none()),
    ));
    let http_id = create_connector(
        &app,
        "http_json",
        "http-items",
        serde_json::json!({"base_url": format!("http://{addr}/items")}),
    )
    .await;
    let job = run_job(&store, &Arc::new(config.clone()), &http_id).await;
    assert_eq!(job.status, "completed");
    let hits = lkb::search::search(
        &store,
        &ProviderSet::with_none(),
        &policy(),
        "httpconnector",
        lkb::search::SearchMode::Bm25,
        10,
    )
    .await
    .unwrap();
    assert_eq!(hits.hits.len(), 1);
    let refs = store
        .external_ref_list("t_local", None, Some("http_json"), 10)
        .await
        .unwrap();
    assert_eq!(refs.len(), 1);
    assert_eq!(refs[0].url, "https://example.invalid/http-1");

    let stub_id = create_connector(&app, "feishu", "feishu-stub", serde_json::json!({})).await;
    let test = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/v1/connectors/{stub_id}/test"))
                .header(header::HOST, "localhost")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let test_json: serde_json::Value = serde_json::from_slice(
        &axum::body::to_bytes(test.into_body(), 1_000_000)
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(test_json["ok"], false);
    assert_eq!(test_json["code"], "connector_not_configured");
    let stub_job = run_job(&store, &Arc::new(config.clone()), &stub_id).await;
    assert_eq!(stub_job.status, "failed");
    assert!(
        stub_job.error.clone().unwrap().contains("not configured"),
        "job={stub_job:?}"
    );
    let jobs = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/api/v1/connectors/{stub_id}/jobs"))
                .header(header::HOST, "localhost")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(jobs.status(), StatusCode::OK);

    let delete = app
        .clone()
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri(format!("/api/v1/connectors/{stub_id}"))
                .header(header::HOST, "localhost")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(delete.status(), StatusCode::OK);
    assert!(store
        .connector_get("t_local", &stub_id)
        .await
        .unwrap()
        .is_none());
}
