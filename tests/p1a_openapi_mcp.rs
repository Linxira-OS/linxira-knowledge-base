use axum::body::Body;
use axum::http::{header, Request, StatusCode};
use lkb::config::Config;
use lkb::governance::{Policy, Principal};
use lkb::ingest::{import_path, ImportOptions};
use lkb::mcp;
use lkb::providers::ProviderSet;
use lkb::store::{self};
use lkb::web::{router, AppState};
use std::sync::Arc;
use tower::ServiceExt;

fn test_config(path: &std::path::Path) -> Config {
    let mut config = Config::default();
    config.storage.sqlite_path = path.to_path_buf();
    config.storage.tenant_id = "t_local".into();
    config.storage.default_space = "s_default".into();
    config
}

async fn search_hits(app: &axum::Router, q: &str) -> usize {
    let response = app.clone().oneshot(Request::builder().uri(format!("/api/v1/search?q={q}"))
        .header(header::HOST, "localhost").body(Body::empty()).unwrap()).await.unwrap();
    let body: serde_json::Value = serde_json::from_slice(&axum::body::to_bytes(response.into_body(), 1_000_000).await.unwrap()).unwrap();
    body["hits"].as_array().map(Vec::len).unwrap_or(0)
}

fn p1_principal() -> Principal {
    Principal::local_user("alice", "t_local", vec!["s_default".into()], vec![])
}

#[tokio::test]
async fn openapi_document_lists_real_routes_without_auth() {
    let temp = tempfile::tempdir().unwrap();
    let config = test_config(&temp.path().join("openapi.sqlite"));
    let store = store::open(&config).await.unwrap();
    let app = router(AppState::new(store, Arc::new(config), Arc::new(ProviderSet::with_none())));
    let response = app
        .oneshot(Request::builder().uri("/openapi.json").header(header::HOST, "localhost").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body: serde_json::Value =
        serde_json::from_slice(&axum::body::to_bytes(response.into_body(), 2_000_000).await.unwrap()).unwrap();
    assert_eq!(body["openapi"], "3.1.0");
    for path in [
        "/api/v1/query", "/api/v1/query/stream", "/api/v1/search", "/api/v1/answer",
        "/api/v1/billing/plans", "/api/v1/billing/wallet", "/api/v1/billing/ledger",
        "/api/v1/billing/invoices", "/api/v1/billing/topup", "/api/v1/billing/webhook/{provider}",
        "/api/v1/auth/csrf", "/api/v1/auth/login", "/api/v1/auth/me", "/api/v1/auth/logout",
        "/api/v1/pages",
        "/api/v1/pages/{space}/{slug}/revisions/{revision_id}",
        "/api/v1/conversations/{conversation_id}/restore",
        "/api/v1/answer/stream",
        "/api/v1/pages/{space}/{slug}", "/api/v1/pages/{space}/{slug}/history",
        "/api/v1/pages/{space}/{slug}/restore",
        "/api/v1/pages/{space}/{slug}/summary", "/api/v1/conversations",
        "/api/v1/admin/users", "/api/v1/admin/roles", "/api/v1/admin/groups",
        "/api/v1/admin/spaces", "/api/v1/admin/usage", "/api/v1/admin/usage/aggregate",
        "/api/v1/admin/audit", "/api/v1/admin/aliases", "/api/v1/admin/providers",
        "/api/v1/admin/routes", "/api/v1/admin/quotas",
    ] {
        assert!(body["paths"][path].is_object(), "missing path {path}");
    }
    for schema in ["Answer", "Citation", "Degraded", "Space", "User", "Quota", "Usage", "Conversation", "Summary"] {
        assert!(body["components"]["schemas"][schema].is_object(), "missing schema {schema}");
    }
    assert!(body["paths"]["/openapi.json"]["get"]["security"].as_array().unwrap().is_empty());
}

#[tokio::test]
async fn mcp_lists_tools_and_calls_read_tools_with_scope() {
    let tools = mcp::tools();
    let names: Vec<&str> = tools.iter().filter_map(|tool| tool["name"].as_str()).collect();
    for expected in ["kb_search", "kb_answer", "kb_get_page", "kb_get_page_summary", "kb_get_page_history", "kb_list_spaces", "kb_write_page"] {
        assert!(names.contains(&expected), "missing MCP tool {expected}");
    }
    assert!(tools.iter().all(|tool| tool["inputSchema"]["type"] == "object"));

    let temp = tempfile::tempdir().unwrap();
    let config = test_config(&temp.path().join("mcp.sqlite"));
    let store = store::open(&config).await.unwrap();
    store::bootstrap_space(&store, &config).await.unwrap();
    let dir = temp.path().join("docs");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("01.md"), "# MCP\n\nMCP search sentence.").unwrap();
    import_path(&store, &dir, &ImportOptions {
        tenant_id: "t_local".into(), space_id: "s_default".into(),
        acl: Some(lkb::governance::Acl::allow_all()),
        allow_implicit_allow_all: true, ..Default::default()
    }, None, None).await.unwrap();
    let store = Arc::new(store);
    let config = Arc::new(config);
    let providers = Arc::new(ProviderSet::with_none());
    let principal = p1_principal();
    let search = mcp::call_tool(&store, &config, &providers, &principal, "kb_search",
        &serde_json::json!({"query": "MCP", "top_k": 5})).await.unwrap();
    assert!(search["hits"].as_array().unwrap().len() >= 1);
    let spaces = mcp::call_tool(&store, &config, &providers, &principal, "kb_list_spaces", &serde_json::json!({})).await.unwrap();
    assert_eq!(spaces["spaces"][0]["space_id"], "s_default");

    let mut reader = p1_principal();
    reader.scope = lkb::governance::TokenScope::Read;
    let denied = mcp::call_tool(&store, &config, &providers, &reader, "kb_write_page",
        &serde_json::json!({"space": "s_default", "slug": "x", "title": "x", "markdown": "# x"})).await;
    assert_eq!(denied.unwrap_err().0, "forbidden");
}

#[tokio::test]
async fn admin_lists_and_usage_aggregate_require_admin_scope() {
    let temp = tempfile::tempdir().unwrap();
    let config = test_config(&temp.path().join("admin.sqlite"));
    let store = store::open(&config).await.unwrap();
    store::bootstrap_space(&store, &config).await.unwrap();
    let app = router(AppState::new(store, Arc::new(config), Arc::new(ProviderSet::with_none())));
    for uri in [
        "/api/v1/admin/users", "/api/v1/admin/roles", "/api/v1/admin/groups",
        "/api/v1/admin/spaces", "/api/v1/admin/usage/aggregate",
    ] {
        let response = app.clone().oneshot(Request::builder().uri(uri).header(header::HOST, "localhost").body(Body::empty()).unwrap()).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK, "{uri}");
    }
    let patch = app.clone().oneshot(Request::builder().method("PATCH").uri("/api/v1/admin/users/local")
        .header(header::HOST, "localhost").header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(r#"{"disabled":false}"#)).unwrap()).await.unwrap();
    assert_eq!(patch.status(), StatusCode::OK);
    let audit = app.clone().oneshot(Request::builder().uri("/api/v1/admin/audit?limit=50")
        .header(header::HOST, "localhost").body(Body::empty()).unwrap()).await.unwrap();
    assert_eq!(audit.status(), StatusCode::OK);
    let audit_json: serde_json::Value = serde_json::from_slice(&axum::body::to_bytes(audit.into_body(), 2_000_000).await.unwrap()).unwrap();
    let audit_events = audit_json["events"].as_array().unwrap();
    assert!(audit_events.iter().any(|event| event["action"] == "user.admin.update" && event["target_id"] == "local"),
        "audit={audit_json}");
    let aggregate = app.oneshot(Request::builder().uri("/api/v1/admin/usage/aggregate?group_by=alias")
        .header(header::HOST, "localhost").body(Body::empty()).unwrap()).await.unwrap();
    let body: serde_json::Value = serde_json::from_slice(&axum::body::to_bytes(aggregate.into_body(), 1_000_000).await.unwrap()).unwrap();
    assert_eq!(body["group_by"], "alias");
    assert!(body["groups"].is_array());
    assert!(body["totals"].is_object());
}

#[allow(dead_code)]
fn _unused(_: &Policy) {}

#[test]
fn mcp_subprocess_smoke_over_stdio() {
    use std::io::Write;
    let temp = tempfile::tempdir().unwrap();
    let db = temp.path().join("mcp-sub.sqlite");
    let config_path = temp.path().join("lkb.toml");
    let db_path = db.display().to_string().replace('\\', "/");
    std::fs::write(&config_path, format!(
        "[storage]\nbackend = \"sqlite\"\nsqlite_path = \"{db_path}\"\ntenant_id = \"t_local\"\ndefault_space = \"s_default\"\n"
    )).unwrap();
    let mut child = std::process::Command::new(env!("CARGO_BIN_EXE_lkb"))
        .arg("--config").arg(&config_path)
        .args(["mcp", "serve"])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()
        .unwrap();
    let mut stdin = child.stdin.take().unwrap();
    for line in [
        r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05"}}"#,
        r#"{"jsonrpc":"2.0","id":2,"method":"tools/list"}"#,
        r#"{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"kb_list_spaces","arguments":{}}}"#,
    ] {
        writeln!(stdin, "{line}").unwrap();
    }
    drop(stdin);
    let output = child.wait_with_output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("\"tools\""), "stdout={stdout}");
    assert!(stdout.contains("kb_search") && stdout.contains("kb_write_page"), "stdout={stdout}");
    assert!(stdout.contains("\"spaces\""), "stdout={stdout}");
}

#[tokio::test]
async fn contract_rest_search_answer_and_page_crud() {
    use axum::http::Method;
    let temp = tempfile::tempdir().unwrap();
    let config = test_config(&temp.path().join("contract.sqlite"));
    let store = store::open(&config).await.unwrap();
    store::bootstrap_space(&store, &config).await.unwrap();
    let dir = temp.path().join("docs");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("01.md"), "# Contract\n\nContract search sentence.").unwrap();
    import_path(&store, &dir, &ImportOptions {
        tenant_id: "t_local".into(), space_id: "s_default".into(),
        acl: Some(lkb::governance::Acl::allow_all()), allow_implicit_allow_all: true, ..Default::default()
    }, None, None).await.unwrap();
    let app = router(AppState::new(store.clone(), Arc::new(config.clone()), Arc::new(ProviderSet::with_none())));

    let search = app.clone().oneshot(Request::builder().uri("/api/v1/search?q=Contract")
        .header(header::HOST, "localhost").body(Body::empty()).unwrap()).await.unwrap();
    assert_eq!(search.status(), StatusCode::OK);
    let search_json: serde_json::Value = serde_json::from_slice(&axum::body::to_bytes(search.into_body(), 1_000_000).await.unwrap()).unwrap();
    assert!(search_json["hits"].as_array().unwrap().len() >= 1);

    let answer = app.clone().oneshot(Request::builder().method("POST").uri("/api/v1/answer")
        .header(header::HOST, "localhost").header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(r#"{"query":"Contract"}"#)).unwrap()).await.unwrap();
    assert_eq!(answer.status(), StatusCode::OK);
    let answer_json: serde_json::Value = serde_json::from_slice(&axum::body::to_bytes(answer.into_body(), 1_000_000).await.unwrap()).unwrap();
    assert!(answer_json["mode"].is_string());

    let stream = app.clone().oneshot(Request::builder().uri("/api/v1/answer/stream?q=Contract")
        .header(header::HOST, "localhost").body(Body::empty()).unwrap()).await.unwrap();
    assert_eq!(stream.status(), StatusCode::OK);
    assert!(stream.headers()[header::CONTENT_TYPE].to_str().unwrap().starts_with("text/event-stream"));
    let stream_body = String::from_utf8(axum::body::to_bytes(stream.into_body(), 2_000_000).await.unwrap().to_vec()).unwrap();
    assert!(stream_body.contains("event: meta") && stream_body.contains("event: done"), "{stream_body}");

    let save = app.clone().oneshot(Request::builder().method("POST").uri("/api/v1/pages/s_default/contract-page")
        .header(header::HOST, "localhost").header(header::CONTENT_TYPE, "application/json")
        .header(header::IF_MATCH, "*")
        .body(Body::from(r##"{"title":"Contract Page","lang":"zh","markdown":"# Contract Page\n\nBody one."}"##)).unwrap()).await.unwrap();
    assert_eq!(save.status(), StatusCode::OK);
    let save_json: serde_json::Value = serde_json::from_slice(&axum::body::to_bytes(save.into_body(), 1_000_000).await.unwrap()).unwrap();
    let revision_id = save_json["revision_id"].as_str().unwrap().to_string();
    assert_eq!(save_json["version"], 1);

    let page = app.clone().oneshot(Request::builder().uri("/api/v1/pages/s_default/contract-page?lang=zh")
        .header(header::HOST, "localhost").body(Body::empty()).unwrap()).await.unwrap();
    assert_eq!(page.status(), StatusCode::OK);
    let page_json: serde_json::Value = serde_json::from_slice(&axum::body::to_bytes(page.into_body(), 1_000_000).await.unwrap()).unwrap();
    assert_eq!(page_json["title"], "Contract Page");
    assert!(page_json["markdown"].as_str().unwrap().contains("Body one"));

    assert!(search_hits(&app, "body").await >= 1, "JSON-created page must be searchable");
    let history = app.clone().oneshot(Request::builder().uri("/api/v1/pages/s_default/contract-page/history?lang=zh")
        .header(header::HOST, "localhost").body(Body::empty()).unwrap()).await.unwrap();
    let history_json: serde_json::Value = serde_json::from_slice(&axum::body::to_bytes(history.into_body(), 1_000_000).await.unwrap()).unwrap();
    assert_eq!(history_json["revisions"].as_array().unwrap().len(), 1);

    let stale = app.clone().oneshot(Request::builder().method("POST").uri("/api/v1/pages/s_default/contract-page")
        .header(header::HOST, "localhost").header(header::CONTENT_TYPE, "application/json")
        .header(header::IF_MATCH, "stale-revision")
        .body(Body::from(r##"{"lang":"zh","markdown":"# Changed"}"##)).unwrap()).await.unwrap();
    assert_eq!(stale.status(), StatusCode::CONFLICT);

    let update = app.clone().oneshot(Request::builder().method("POST").uri("/api/v1/pages/s_default/contract-page")
        .header(header::HOST, "localhost").header(header::CONTENT_TYPE, "application/json")
        .header(header::IF_MATCH, revision_id.as_str())
        .body(Body::from(r##"{"lang":"zh","markdown":"# Contract Page\n\nBody two."}"##)).unwrap()).await.unwrap();
    assert_eq!(update.status(), StatusCode::OK);
    let update_json: serde_json::Value = serde_json::from_slice(&axum::body::to_bytes(update.into_body(), 1_000_000).await.unwrap()).unwrap();
    assert_eq!(update_json["version"], 2);
    assert!(search_hits(&app, "two").await >= 1, "updated content must be searchable");
    assert_eq!(search_hits(&app, "one").await, 0, "old content must not be searchable");

    let delete = app.clone().oneshot(Request::builder().method("DELETE").uri("/api/v1/pages/s_default/contract-page?lang=zh")
        .header(header::HOST, "localhost").body(Body::empty()).unwrap()).await.unwrap();
    assert_eq!(delete.status(), StatusCode::OK);
    assert_eq!(search_hits(&app, "two").await, 0, "archived page must not be searchable");
    let missing = app.clone().oneshot(Request::builder().uri("/api/v1/pages/s_default/contract-page?lang=zh")
        .header(header::HOST, "localhost").body(Body::empty()).unwrap()).await.unwrap();
    assert_eq!(missing.status(), StatusCode::NOT_FOUND);
    let restore = app.clone().oneshot(Request::builder().method("POST").uri("/api/v1/pages/s_default/contract-page/restore?lang=zh")
        .header(header::HOST, "localhost").body(Body::empty()).unwrap()).await.unwrap();
    assert_eq!(restore.status(), StatusCode::OK);
    assert!(search_hits(&app, "two").await >= 1, "restored page must be searchable again");
    let restored = app.oneshot(Request::builder().uri("/api/v1/pages/s_default/contract-page?lang=zh")
        .header(header::HOST, "localhost").body(Body::empty()).unwrap()).await.unwrap();
    assert_eq!(restored.status(), StatusCode::OK);
    let _ = Method::POST;
}

fn set_cookie_value(response: &axum::response::Response, name: &str) -> String {
    for value in response.headers().get_all(header::SET_COOKIE).iter() {
        if let Ok(text) = value.to_str() {
            if let Some(rest) = text.strip_prefix(&format!("{name}=")) {
                return rest.split(';').next().unwrap_or("").to_string();
            }
        }
    }
    String::new()
}

#[tokio::test]
async fn pages_list_revision_body_json_auth_and_conversation_restore() {
    let temp = tempfile::tempdir().unwrap();
    let config = test_config(&temp.path().join("p1contract.sqlite"));
    let store = store::open(&config).await.unwrap();
    store::bootstrap_space(&store, &config).await.unwrap();
    let app = router(AppState::new(store.clone(), Arc::new(config.clone()), Arc::new(ProviderSet::with_none())));
    let mut auth_config = test_config(&temp.path().join("p1contract-auth.sqlite"));
    auth_config.auth.mode = lkb::config::AuthMode::Session;
    let auth_store = store::open(&auth_config).await.unwrap();
    store::bootstrap_space(&auth_store, &auth_config).await.unwrap();
    auth_store.add_user_with_role("t_local", "alice", &lkb::cli::hash_password("secret"), lkb::governance::UserRole::User).await.unwrap();
    let app_auth = router(AppState::new(auth_store, Arc::new(auth_config), Arc::new(ProviderSet::with_none())));

    // page list: create three pages, paginate, filter
    for (slug, word) in [("page-a", "alpha"), ("page-b", "beta"), ("page-c", "gamma")] {
        let response = app.clone().oneshot(Request::builder().method("POST").uri(format!("/api/v1/pages/s_default/{slug}"))
            .header(header::HOST, "localhost").header(header::CONTENT_TYPE, "application/json")
            .header(header::IF_MATCH, "*")
            .body(Body::from(format!(r##"{{"title":"{slug}","lang":"zh","markdown":"# {slug}\n\n{word} body."}}"##))).unwrap()).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK, "{slug}");
    }
    let list = app.clone().oneshot(Request::builder().uri("/api/v1/pages?space=s_default&lang=zh&limit=2")
        .header(header::HOST, "localhost").body(Body::empty()).unwrap()).await.unwrap();
    assert_eq!(list.status(), StatusCode::OK);
    let list_json: serde_json::Value = serde_json::from_slice(&axum::body::to_bytes(list.into_body(), 2_000_000).await.unwrap()).unwrap();
    assert_eq!(list_json["pages"].as_array().unwrap().len(), 2);
    let cursor = list_json["next_cursor"].as_str().expect("next_cursor").to_string();
    let next = app.clone().oneshot(Request::builder().uri(format!("/api/v1/pages?space=s_default&lang=zh&limit=2&cursor={cursor}"))
        .header(header::HOST, "localhost").body(Body::empty()).unwrap()).await.unwrap();
    let next_json: serde_json::Value = serde_json::from_slice(&axum::body::to_bytes(next.into_body(), 2_000_000).await.unwrap()).unwrap();
    assert_eq!(next_json["pages"].as_array().unwrap().len(), 1);
    assert!(list_json["pages"][0]["current_revision_id"].is_string());
    let filtered = app.clone().oneshot(Request::builder().uri("/api/v1/pages?space=s_default&q=page-b")
        .header(header::HOST, "localhost").body(Body::empty()).unwrap()).await.unwrap();
    let filtered_json: serde_json::Value = serde_json::from_slice(&axum::body::to_bytes(filtered.into_body(), 2_000_000).await.unwrap()).unwrap();
    assert_eq!(filtered_json["pages"].as_array().unwrap().len(), 1);
    let denied = app.clone().oneshot(Request::builder().uri("/api/v1/pages?space=other")
        .header(header::HOST, "localhost").body(Body::empty()).unwrap()).await.unwrap();
    assert_eq!(denied.status(), StatusCode::FORBIDDEN);

    // revision body via revision id after update
    let page = app.clone().oneshot(Request::builder().uri("/api/v1/pages/s_default/page-a?lang=zh")
        .header(header::HOST, "localhost").body(Body::empty()).unwrap()).await.unwrap();
    let page_json: serde_json::Value = serde_json::from_slice(&axum::body::to_bytes(page.into_body(), 1_000_000).await.unwrap()).unwrap();
    let v1 = page_json["current_revision_id"].as_str().unwrap().to_string();
    let update = app.clone().oneshot(Request::builder().method("POST").uri("/api/v1/pages/s_default/page-a")
        .header(header::HOST, "localhost").header(header::CONTENT_TYPE, "application/json")
        .header(header::IF_MATCH, v1.as_str())
        .body(Body::from(r##"{"lang":"zh","markdown":"# page-a\n\nalpha v2 body."}"##)).unwrap()).await.unwrap();
    assert_eq!(update.status(), StatusCode::OK);
    let revision = app.clone().oneshot(Request::builder().uri(format!("/api/v1/pages/s_default/page-a/revisions/{v1}"))
        .header(header::HOST, "localhost").body(Body::empty()).unwrap()).await.unwrap();
    assert_eq!(revision.status(), StatusCode::OK);
    let revision_json: serde_json::Value = serde_json::from_slice(&axum::body::to_bytes(revision.into_body(), 1_000_000).await.unwrap()).unwrap();
    assert!(revision_json["markdown"].as_str().unwrap().contains("alpha body"));
    assert_eq!(revision_json["version"], 1);
    let history_full = app.clone().oneshot(Request::builder().uri("/api/v1/pages/s_default/page-a/history?lang=zh&include_markdown=true")
        .header(header::HOST, "localhost").body(Body::empty()).unwrap()).await.unwrap();
    let history_json: serde_json::Value = serde_json::from_slice(&axum::body::to_bytes(history_full.into_body(), 1_000_000).await.unwrap()).unwrap();
    assert!(history_json["revisions"][0]["markdown"].is_string());
    let history_slim = app.clone().oneshot(Request::builder().uri("/api/v1/pages/s_default/page-a/history?lang=zh")
        .header(header::HOST, "localhost").body(Body::empty()).unwrap()).await.unwrap();
    let slim_json: serde_json::Value = serde_json::from_slice(&axum::body::to_bytes(history_slim.into_body(), 1_000_000).await.unwrap()).unwrap();
    assert!(slim_json["revisions"][0].get("markdown").is_none());

    // JSON auth: csrf cookie, origin, failure, success, me, logout
    let csrf = app_auth.clone().oneshot(Request::builder().uri("/api/v1/auth/csrf")
        .header(header::HOST, "localhost").body(Body::empty()).unwrap()).await.unwrap();
    assert_eq!(csrf.status(), StatusCode::OK);
    let csrf_cookie = set_cookie_value(&csrf, "lkb_csrf");
    let csrf_json: serde_json::Value = serde_json::from_slice(&axum::body::to_bytes(csrf.into_body(), 1_000_000).await.unwrap()).unwrap();
    let csrf_token = csrf_json["csrf_token"].as_str().unwrap().to_string();
    assert!(!csrf_cookie.is_empty());

    let no_csrf = app_auth.clone().oneshot(Request::builder().method("POST").uri("/api/v1/auth/login")
        .header(header::HOST, "localhost").header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(r#"{"username":"alice","password":"secret"}"#)).unwrap()).await.unwrap();
    assert_eq!(no_csrf.status(), StatusCode::FORBIDDEN);
    let bad_origin = app_auth.clone().oneshot(Request::builder().method("POST").uri("/api/v1/auth/login")
        .header(header::HOST, "localhost").header(header::ORIGIN, "http://evil.example")
        .header(header::CONTENT_TYPE, "application/json")
        .header(header::COOKIE, format!("lkb_csrf={csrf_cookie}"))
        .body(Body::from(format!(r#"{{"username":"alice","password":"secret","csrf_token":"{csrf_token}"}}"#))).unwrap()).await.unwrap();
    assert_eq!(bad_origin.status(), StatusCode::FORBIDDEN);
    let wrong = app_auth.clone().oneshot(Request::builder().method("POST").uri("/api/v1/auth/login")
        .header(header::HOST, "localhost").header(header::CONTENT_TYPE, "application/json")
        .header(header::COOKIE, format!("lkb_csrf={csrf_cookie}"))
        .body(Body::from(format!(r#"{{"username":"alice","password":"nope","csrf_token":"{csrf_token}"}}"#))).unwrap()).await.unwrap();
    assert_eq!(wrong.status(), StatusCode::UNAUTHORIZED);
    let login = app_auth.clone().oneshot(Request::builder().method("POST").uri("/api/v1/auth/login")
        .header(header::HOST, "localhost").header(header::CONTENT_TYPE, "application/json")
        .header(header::COOKIE, format!("lkb_csrf={csrf_cookie}"))
        .body(Body::from(format!(r#"{{"username":"alice","password":"secret","csrf_token":"{csrf_token}"}}"#))).unwrap()).await.unwrap();
    assert_eq!(login.status(), StatusCode::OK);
    let session_cookie = set_cookie_value(&login, "lkb_session");
    let login_json: serde_json::Value = serde_json::from_slice(&axum::body::to_bytes(login.into_body(), 1_000_000).await.unwrap()).unwrap();
    assert!(login_json["principal"]["user_id"].as_str().unwrap().starts_with("usr_"));
    assert!(login_json["principal"]["display_name"].is_string());
    assert!(!session_cookie.is_empty());
    let me = app_auth.clone().oneshot(Request::builder().uri("/api/v1/auth/me")
        .header(header::HOST, "localhost").header(header::COOKIE, format!("lkb_session={session_cookie}"))
        .body(Body::empty()).unwrap()).await.unwrap();
    assert_eq!(me.status(), StatusCode::OK);
    let me_json: serde_json::Value = serde_json::from_slice(&axum::body::to_bytes(me.into_body(), 1_000_000).await.unwrap()).unwrap();
    assert!(me_json["principal"]["user_id"].as_str().unwrap().starts_with("usr_"));
    let logout = app_auth.clone().oneshot(Request::builder().method("POST").uri("/api/v1/auth/logout")
        .header(header::HOST, "localhost")
        .header(header::COOKIE, format!("lkb_session={session_cookie}; lkb_csrf={csrf_cookie}"))
        .header("x-csrf-token", csrf_token.as_str())
        .body(Body::empty()).unwrap()).await.unwrap();
    assert_eq!(logout.status(), StatusCode::OK);
    let me_after = app_auth.clone().oneshot(Request::builder().uri("/api/v1/auth/me")
        .header(header::HOST, "localhost").header(header::COOKIE, format!("lkb_session={session_cookie}"))
        .body(Body::empty()).unwrap()).await.unwrap();
    assert_eq!(me_after.status(), StatusCode::UNAUTHORIZED);

    // conversation soft delete + restore
    let search = app.clone().oneshot(Request::builder().method("POST").uri("/api/v1/query")
        .header(header::HOST, "localhost").header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(r#"{"query":"alpha","mode":"search"}"#)).unwrap()).await.unwrap();
    let search_json: serde_json::Value = serde_json::from_slice(&axum::body::to_bytes(search.into_body(), 1_000_000).await.unwrap()).unwrap();
    let conversation_id = search_json["conversation_id"].as_str().unwrap().to_string();
    let delete = app.clone().oneshot(Request::builder().method("DELETE").uri(format!("/api/v1/conversations/{conversation_id}"))
        .header(header::HOST, "localhost").body(Body::empty()).unwrap()).await.unwrap();
    assert_eq!(delete.status(), StatusCode::OK);
    let restore = app.clone().oneshot(Request::builder().method("POST").uri(format!("/api/v1/conversations/{conversation_id}/restore"))
        .header(header::HOST, "localhost").body(Body::empty()).unwrap()).await.unwrap();
    assert_eq!(restore.status(), StatusCode::OK);
    let restore_json: serde_json::Value = serde_json::from_slice(&axum::body::to_bytes(restore.into_body(), 1_000_000).await.unwrap()).unwrap();
    assert_eq!(restore_json["restored"], true);
    let detail = app.oneshot(Request::builder().uri(format!("/api/v1/conversations/{conversation_id}"))
        .header(header::HOST, "localhost").body(Body::empty()).unwrap()).await.unwrap();
    assert_eq!(detail.status(), StatusCode::OK);
}
