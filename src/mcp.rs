//! Minimal MCP (Model Context Protocol) stdio server. It shares the REST
//! kernel: the same store, policy, search/answer/summary paths.

use crate::config::Config;
use crate::governance::{Policy, Principal, TokenScope};
use crate::providers::ProviderSet;
use crate::search::SearchMode;
use crate::store::Store;
use serde_json::{json, Value};
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

pub async fn serve(
    store: Arc<dyn Store>,
    config: Arc<Config>,
    token: Option<String>,
) -> anyhow::Result<()> {
    let principal = crate::cli::resolve_principal(&config, &store, token.as_deref())
        .await
        .map_err(|err| anyhow::anyhow!(err.to_string()))?;
    let providers = Arc::new(ProviderSet::from_config(&config.models));
    let stdin = tokio::io::stdin();
    let mut lines = BufReader::new(stdin).lines();
    let mut stdout = tokio::io::stdout();
    while let Some(line) = lines.next_line().await? {
        let line = line.trim().to_string();
        if line.is_empty() {
            continue;
        }
        let request: Value = match serde_json::from_str(&line) {
            Ok(request) => request,
            Err(_) => continue,
        };
        let id = request.get("id").cloned();
        if id.is_none() {
            // Notification (e.g. notifications/initialized): no response.
            continue;
        }
        let method = request.get("method").and_then(Value::as_str).unwrap_or("");
        let params = request.get("params").cloned().unwrap_or(Value::Null);
        let response = match method {
            "initialize" => json!({
                "protocolVersion": "2024-11-05",
                "capabilities": {"tools": {}},
                "serverInfo": {"name": "lkb", "version": env!("CARGO_PKG_VERSION")},
            }),
            "ping" => json!({}),
            "tools/list" => json!({"tools": tools()}),
            "tools/call" => {
                let name = params.get("name").and_then(Value::as_str).unwrap_or("");
                let arguments = params.get("arguments").cloned().unwrap_or(json!({}));
                match call_tool(&store, &config, &providers, &principal, name, &arguments).await {
                    Ok(value) => json!({
                        "content": [{"type": "text", "text": value.to_string()}],
                        "structuredContent": value,
                        "isError": false,
                    }),
                    Err((code, message)) => json!({
                        "content": [{"type": "text", "text": format!("{code}: {message}")}],
                        "isError": true,
                        "lkb_error": {"code": code, "message": message},
                    }),
                }
            }
            _ => {
                let reply = json!({"jsonrpc": "2.0", "id": id, "error": {"code": -32601, "message": "method not found"}});
                stdout.write_all(format!("{reply}\n").as_bytes()).await?;
                stdout.flush().await?;
                continue;
            }
        };
        let reply = json!({"jsonrpc": "2.0", "id": id, "result": response});
        stdout.write_all(format!("{reply}\n").as_bytes()).await?;
        stdout.flush().await?;
    }
    Ok(())
}

fn tool(name: &str, description: &str, scope: &str, properties: Value, required: Value) -> Value {
    json!({
        "name": name,
        "description": description,
        "inputSchema": {"type": "object", "properties": properties, "required": required},
        "annotations": {"x-lkb-scope": scope},
    })
}

pub fn tools() -> Vec<Value> {
    vec![
        tool("kb_search", "Search the knowledge base", "kb.read",
            json!({"query": {"type": "string"}, "space": {"type": ["string", "null"]}, "top_k": {"type": "integer"}}),
            json!(["query"])),
        tool("kb_answer", "Answer a question with citations", "kb.read",
            json!({"query": {"type": "string"}, "space": {"type": ["string", "null"]}}),
            json!(["query"])),
        tool("kb_get_page", "Read a wiki page", "kb.read",
            json!({"space": {"type": "string"}, "slug": {"type": "string"}, "lang": {"type": ["string", "null"]}}),
            json!(["space", "slug"])),
        tool("kb_get_page_summary", "Read a page summary", "kb.read",
            json!({"space": {"type": "string"}, "slug": {"type": "string"}, "lang": {"type": ["string", "null"]}}),
            json!(["space", "slug"])),
        tool("kb_get_page_history", "Read page revision history", "kb.read",
            json!({"space": {"type": "string"}, "slug": {"type": "string"}, "lang": {"type": ["string", "null"]}}),
            json!(["space", "slug"])),
        tool("kb_list_spaces", "List accessible spaces", "kb.read", json!({}), json!([])),
        tool("kb_write_page", "Save a wiki page", "kb.write",
            json!({"space": {"type": "string"}, "slug": {"type": "string"}, "title": {"type": "string"}, "markdown": {"type": "string"}, "lang": {"type": ["string", "null"]}}),
            json!(["space", "slug", "title", "markdown"])),
    ]
}

fn require_read(_principal: &Principal) -> Result<(), (String, String)> {
    // Every token scope carries at least read access.
    Ok(())
}

fn require_write(principal: &Principal) -> Result<(), (String, String)> {
    match principal.scope {
        TokenScope::Write | TokenScope::Admin => Ok(()),
        _ => Err(("forbidden".into(), "kb.write scope required".into())),
    }
}

fn string_arg(arguments: &Value, key: &str) -> Result<String, (String, String)> {
    arguments
        .get(key)
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| ("invalid_request".into(), format!("{key} is required")))
}

fn scoped_space(principal: &Principal, space: &str) -> Result<(), (String, String)> {
    if principal.allowed_space_ids.iter().any(|allowed| allowed == space) {
        Ok(())
    } else {
        Err(("forbidden".into(), format!("space {space} is outside the token scope")))
    }
}

pub async fn call_tool(
    store: &Arc<dyn Store>,
    config: &Arc<Config>,
    providers: &Arc<ProviderSet>,
    principal: &Principal,
    name: &str,
    arguments: &Value,
) -> Result<Value, (String, String)> {
    require_read(principal)?;
    let policy = Policy::new(principal.clone(), None)
        .map_err(|err| ("internal_error".to_string(), err.to_string()))?;
    match name {
        "kb_search" => {
            let query = string_arg(arguments, "query")?;
            let space = arguments.get("space").and_then(Value::as_str);
            if let Some(space) = space {
                scoped_space(principal, space)?;
            }
            let top_k = arguments.get("top_k").and_then(Value::as_u64).unwrap_or(10) as usize;
            let mut tuning = crate::search::SearchTuning::from_retrieval(&config.retrieval);
            tuning.billing_local_ledger = config.billing.engine.eq_ignore_ascii_case("local");
            let response = crate::search::search_with_tuning(store, providers, &policy, &query, SearchMode::Auto, top_k.max(1).min(100), &tuning)
                .await
                .map_err(|err| ("search_failed".to_string(), err.to_string()))?;
            Ok(serde_json::to_value(response).unwrap_or(Value::Null))
        }
        "kb_answer" => {
            let query = string_arg(arguments, "query")?;
            let options = crate::answer::AnswerOptions::default();
            let answer = crate::answer::build_answer_with_ledger(store, providers, &policy, &query, &options, config.billing.engine.eq_ignore_ascii_case("local"))
                .await
                .map_err(|err| ("answer_failed".to_string(), err.to_string()))?;
            Ok(serde_json::to_value(answer).unwrap_or(Value::Null))
        }
        "kb_get_page" => {
            let (space, slug, lang) = page_args(arguments)?;
            scoped_space(principal, &space)?;
            let view = store
                .get_page(&policy, &space, &slug, lang.as_deref())
                .await
                .map_err(|err| ("store_error".to_string(), err.to_string()))?
                .ok_or_else(|| ("not_found".to_string(), "page not found".to_string()))?;
            Ok(json!({
                "page": view.page, "revision": view.revision,
            }))
        }
        "kb_get_page_summary" => {
            let (space, slug, lang) = page_args(arguments)?;
            scoped_space(principal, &space)?;
            let view = store
                .get_page(&policy, &space, &slug, lang.as_deref())
                .await
                .map_err(|err| ("store_error".to_string(), err.to_string()))?
                .ok_or_else(|| ("not_found".to_string(), "page not found".to_string()))?;
            let logical = format!("wiki/{}/{}/{}.md", space, view.page.lang, slug);
            let canonical = crate::ingest::doc_id::canonical_relative_path(&logical)
                .map_err(|err| ("internal_error".to_string(), err))?;
            let doc_id = crate::ingest::doc_id::derive_doc_id(&canonical, &view.page.content_hash);
            match crate::summary::latest_summary(store, &principal.tenant_id, &space, &doc_id, &view.page.content_hash, config).await {
                Some(summary) => Ok(json!({"summary": summary})),
                None => Ok(json!({"summary": Value::Null, "status": "pending"})),
            }
        }
        "kb_get_page_history" => {
            let (space, slug, lang) = page_args(arguments)?;
            scoped_space(principal, &space)?;
            let revisions = store
                .list_revisions(&principal.tenant_id, &space, &slug, lang.as_deref())
                .await
                .map_err(|err| ("store_error".to_string(), err.to_string()))?;
            Ok(json!({"revisions": revisions}))
        }
        "kb_list_spaces" => {
            let spaces = store
                .list_spaces(&principal.tenant_id)
                .await
                .map_err(|err| ("store_error".to_string(), err.to_string()))?;
            let visible: Vec<_> = spaces
                .into_iter()
                .filter(|space| principal.allowed_space_ids.iter().any(|allowed| allowed == &space.space_id))
                .collect();
            Ok(json!({"spaces": visible}))
        }
        "kb_write_page" => {
            require_write(principal)?;
            let (space, slug, lang) = page_args(arguments)?;
            scoped_space(principal, &space)?;
            let title = string_arg(arguments, "title")?;
            let markdown = string_arg(arguments, "markdown")?;
            let lang = lang.unwrap_or_else(|| "zh".to_string());
            let existing = store
                .get_page(&policy, &space, &slug, Some(&lang))
                .await
                .map_err(|err| ("store_error".to_string(), err.to_string()))?;
            let now = crate::util::now_rfc3339();
            let content_hash = crate::util::sha256_hex(markdown.as_bytes());
            let version = existing.as_ref().map(|view| view.page.version + 1).unwrap_or(1);
            let revision_id = crate::ingest::doc_id::derive_revision_id(&principal.tenant_id, &space, &slug, &lang, &content_hash, version);
            let acl_json = match existing.as_ref().map(|view| view.page.acl_json.clone()) {
                Some(json) => json,
                None => store
                    .get_space(&principal.tenant_id, &space)
                    .await
                    .map_err(|err| ("store_error".to_string(), err.to_string()))?
                    .map(|space| space.default_acl_json)
                    .unwrap_or_else(|| "{\"mode\":\"allow_all\",\"users\":[],\"groups\":[],\"version\":1}".to_string()),
            };
            let acl_principals = existing
                .as_ref()
                .map(|view| view.page.acl_principals.clone())
                .unwrap_or_else(|| "|*|".to_string());
            let page = crate::store::PageRecord {
                tenant_id: principal.tenant_id.clone(),
                space_id: space.clone(),
                slug: slug.clone(),
                lang: lang.clone(),
                title: title.clone(),
                summary: markdown.chars().take(200).collect(),
                order_index: existing.as_ref().map(|view| view.page.order_index).unwrap_or(0),
                current_revision_id: revision_id.clone(),
                content_hash: content_hash.clone(),
                acl_json: acl_json.clone(),
                acl_principals: acl_principals.clone(),
                valid_from: now.clone(),
                valid_to: None,
                version,
                superseded_by: None,
                created_at: existing.as_ref().map(|view| view.page.created_at.clone()).unwrap_or_else(|| now.clone()),
                updated_at: now.clone(),
            };
            let revision = crate::store::RevisionRecord {
                tenant_id: principal.tenant_id.clone(),
                revision_id: revision_id.clone(),
                space_id: space.clone(),
                slug: slug.clone(),
                lang: lang.clone(),
                title,
                markdown,
                content_hash,
                author: principal.principal_id.clone(),
                created_at: now.clone(),
                version,
                acl_json,
                acl_principals,
                valid_from: now,
                valid_to: None,
                superseded_by: None,
                source_url: None,
                source_revision: None,
            };
            store
                .save_page(&page, &revision)
                .await
                .map_err(|err| ("store_error".to_string(), err.to_string()))?;
            Ok(json!({"space": space, "slug": slug, "lang": lang, "version": version, "revision_id": revision_id}))
        }
        other => Err(("method_not_found".to_string(), format!("unknown tool {other}"))),
    }
}

fn page_args(arguments: &Value) -> Result<(String, String, Option<String>), (String, String)> {
    let space = string_arg(arguments, "space")?;
    let slug = string_arg(arguments, "slug")?;
    let lang = arguments.get("lang").and_then(Value::as_str).map(str::to_string);
    Ok((space, slug, lang))
}
