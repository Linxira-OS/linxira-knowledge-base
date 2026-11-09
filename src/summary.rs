// SPDX-License-Identifier: MIT OR Apache-2.0
//! Static document summaries and the async summary job worker.

use crate::config::Config;
use crate::gateway::Gateway;
use crate::store::{DocumentRecord, DocumentSummaryRecord, Store, SummaryJobRecord};
use serde_json::{json, Value};
use std::sync::Arc;

pub const EXTRACTIVE_MODEL: &str = "extractive";

pub async fn enqueue(store: &Arc<dyn Store>, document: &DocumentRecord, revision_id: &str) {
    let job = SummaryJobRecord {
        job_id: format!("sum_{}", crate::util::random_hex(12)),
        tenant_id: document.tenant_id.clone(),
        space_id: document.space_id.clone(),
        doc_id: document.doc_id.clone(),
        revision_id: revision_id.to_string(),
        status: "queued".to_string(),
        attempts: 0,
        scheduled_at: crate::util::now_rfc3339(),
        started_at: None,
        finished_at: None,
        error: None,
    };
    let _ = store.create_summary_job(&job).await;
}

fn sentence_split(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut current = String::new();
    for ch in text.chars() {
        current.push(ch);
        if matches!(ch, '。' | '！' | '？' | '.' | '!' | '?' | '\n') {
            let trimmed = current.trim();
            if !trimmed.is_empty() {
                out.push(trimmed.chars().take(300).collect());
            }
            current.clear();
        }
    }
    let trimmed = current.trim();
    if !trimmed.is_empty() {
        out.push(trimmed.chars().take(300).collect());
    }
    out
}

pub fn extractive_summary(document: &DocumentRecord, revision_id: &str) -> Value {
    let sentences = sentence_split(&document.content);
    let tldr = sentences
        .first()
        .cloned()
        .unwrap_or_else(|| crate::util::truncate_chars(&document.content, 200));
    let bullets: Vec<String> = sentences.iter().skip(1).take(4).cloned().collect();
    let words: Vec<String> = document
        .content
        .split_whitespace()
        .filter(|word| word.chars().count() > 2)
        .take(8)
        .map(|word| {
            word.trim_matches(|ch: char| !ch.is_alphanumeric())
                .to_string()
        })
        .filter(|word| !word.is_empty())
        .collect();
    json!({
        "tldr": crate::util::truncate_chars(&tldr, 240),
        "bullets": bullets,
        "keywords": words,
        "entities": [],
        "lang": document.lang,
        "source_revision": revision_id,
        "fallback": true,
    })
}

fn parse_summary(content: &str, document: &DocumentRecord, revision_id: &str) -> Option<Value> {
    let text = crate::util::strip_code_fence(content);
    let mut value: Value = serde_json::from_str(text).ok()?;
    if value
        .get("tldr")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim()
        .is_empty()
    {
        return None;
    }
    if value.get("source_revision").is_none() {
        value["source_revision"] = json!(revision_id);
    }
    if value.get("lang").is_none() {
        value["lang"] = json!(document.lang);
    }
    value["fallback"] = json!(false);
    Some(value)
}

async fn generate_summary(
    store: &Arc<dyn Store>,
    config: &Config,
    document: &DocumentRecord,
    revision_id: &str,
) -> (Value, Option<String>, Option<String>) {
    if !config.summary.enabled || !config.summary.auto {
        return (
            extractive_summary(document, revision_id),
            None,
            Some("summary_generation_failed".to_string()),
        );
    }
    let prompt = format!(
        "Summarize the document as strict JSON with keys tldr (one sentence), bullets (3-5 strings), keywords (strings), entities (objects with name/type), lang, source_revision. Document:\n{}",
        crate::util::truncate_chars(&document.content, config.summary.max_chars)
    );
    let request = crate::gateway::GatewayRequest {
        request_id: format!("sum_{}", crate::util::random_hex(10)),
        principal: crate::governance::Principal::local_user(
            "system:summary",
            &document.tenant_id,
            vec![document.space_id.clone()],
            Vec::new(),
        ),
        space_id: Some(document.space_id.clone()),
        role: "system".to_string(),
        alias: config.summary.model_alias.clone(),
        messages: vec![crate::gateway::ChatMessage {
            role: "user".into(),
            content: prompt,
        }],
        max_output_tokens: 512,
        temperature: 0.0,
        source: "summary".to_string(),
        rewritten_query: None,
    };
    match Gateway::new(store.clone())
        .with_local_ledger(config.billing.engine.eq_ignore_ascii_case("local"))
        .chat(&request)
        .await
    {
        Ok(response) => {
            if let Some(value) = parse_summary(&response.content, document, revision_id) {
                (value, response.usage_event_id, None)
            } else {
                (
                    extractive_summary(document, revision_id),
                    response.usage_event_id,
                    Some("summary_generation_failed".to_string()),
                )
            }
        }
        Err(err) => (
            extractive_summary(document, revision_id),
            None,
            Some(format!("summary_generation_failed: {}", err.code)),
        ),
    }
}

pub async fn process_pending(store: &Arc<dyn Store>, config: &Config, limit: i64) -> usize {
    if !config.summary.enabled {
        return 0;
    }
    let jobs = match store.list_summary_jobs("queued", limit.max(1)).await {
        Ok(jobs) => jobs,
        Err(_) => return 0,
    };
    let mut processed = 0usize;
    for job in jobs {
        let _ = store
            .update_summary_job(
                &job.job_id,
                "running",
                job.attempts.saturating_add(1),
                Some(&crate::util::now_rfc3339()),
                None,
                None,
            )
            .await;
        let document = store
            .list_documents(&job.tenant_id, Some(&job.space_id))
            .await
            .ok()
            .and_then(|documents| documents.into_iter().find(|doc| doc.doc_id == job.doc_id));
        let Some(document) = document else {
            let _ = store
                .update_summary_job(
                    &job.job_id,
                    "failed",
                    job.attempts + 1,
                    None,
                    Some(&crate::util::now_rfc3339()),
                    Some("document_not_found"),
                )
                .await;
            continue;
        };
        let existing = store
            .get_document_summary(
                &job.tenant_id,
                &job.space_id,
                &job.doc_id,
                &job.revision_id,
                &config.summary.model_alias,
                &config.summary.prompt_version,
            )
            .await
            .ok()
            .flatten();
        if existing
            .as_ref()
            .map(|summary| summary.status == "ready")
            .unwrap_or(false)
        {
            let _ = store
                .update_summary_job(
                    &job.job_id,
                    "ready",
                    job.attempts,
                    None,
                    Some(&crate::util::now_rfc3339()),
                    None,
                )
                .await;
            continue;
        }
        let mut attempts = job.attempts;
        loop {
            attempts += 1;
            let (summary, usage_event_id, error) =
                generate_summary(store, config, &document, &job.revision_id).await;
            let failed = error.is_some();
            let record = DocumentSummaryRecord {
                tenant_id: job.tenant_id.clone(),
                space_id: job.space_id.clone(),
                doc_id: job.doc_id.clone(),
                revision_id: job.revision_id.clone(),
                lang: document.lang.clone(),
                model: if summary
                    .get("fallback")
                    .and_then(Value::as_bool)
                    .unwrap_or(false)
                {
                    EXTRACTIVE_MODEL.to_string()
                } else {
                    config.summary.model_alias.clone()
                },
                prompt_version: config.summary.prompt_version.clone(),
                content_hash: document.content_hash.clone(),
                status: if failed {
                    "failed".to_string()
                } else {
                    "ready".to_string()
                },
                summary_json: summary.to_string(),
                usage_event_id,
                generated_at: Some(crate::util::now_rfc3339()),
                error: error.clone(),
            };
            if store.upsert_document_summary(&record).await.is_ok() {
                let status = if failed && attempts < config.summary.max_attempts {
                    "queued"
                } else if failed {
                    "failed"
                } else {
                    "ready"
                };
                let _ = store
                    .update_summary_job(
                        &job.job_id,
                        status,
                        attempts,
                        None,
                        Some(&crate::util::now_rfc3339()),
                        error.as_deref(),
                    )
                    .await;
                if status == "queued" {
                    tokio::time::sleep(std::time::Duration::from_millis(
                        config.summary.retry_backoff_ms,
                    ))
                    .await;
                    continue;
                }
                processed += 1;
            } else {
                let _ = store
                    .update_summary_job(
                        &job.job_id,
                        "failed",
                        attempts,
                        None,
                        Some(&crate::util::now_rfc3339()),
                        Some("summary_store_failed"),
                    )
                    .await;
            }
            break;
        }
    }
    processed
}

pub fn spawn_worker(store: Arc<dyn Store>, config: Arc<Config>) {
    tokio::spawn(async move {
        loop {
            let _ = process_pending(&store, &config, config.summary.batch as i64).await;
            tokio::time::sleep(std::time::Duration::from_secs(5)).await;
        }
    });
}

pub async fn latest_summary(
    store: &Arc<dyn Store>,
    tenant_id: &str,
    space_id: &str,
    doc_id: &str,
    revision_id: &str,
    config: &Config,
) -> Option<DocumentSummaryRecord> {
    if let Some(summary) = store
        .get_document_summary(
            tenant_id,
            space_id,
            doc_id,
            revision_id,
            &config.summary.model_alias,
            &config.summary.prompt_version,
        )
        .await
        .ok()
        .flatten()
    {
        return Some(summary);
    }
    store
        .get_document_summary(
            tenant_id,
            space_id,
            doc_id,
            revision_id,
            EXTRACTIVE_MODEL,
            &config.summary.prompt_version,
        )
        .await
        .ok()
        .flatten()
}
