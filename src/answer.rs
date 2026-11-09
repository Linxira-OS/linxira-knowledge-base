use crate::contract::{
    sort_degraded, Answer, AnswerMode, Citation, Confidence, Conflict, ConflictReason,
    ConflictResolution, Degraded,
};
use crate::error::StoreError;
use crate::governance::Policy;
use crate::providers::ProviderSet;
use crate::search::{search_with_tuning, SearchMode, SearchTuning};
use crate::store::Store;
use serde::Deserialize;
use std::collections::BTreeMap;
use std::sync::Arc;
use tokio::sync::mpsc;

pub struct AnswerOptions {
    pub mode: SearchMode,
    pub limit: usize,
    pub tuning: SearchTuning,
}

impl Default for AnswerOptions {
    fn default() -> Self {
        Self {
            mode: SearchMode::Auto,
            limit: 8,
            tuning: SearchTuning::default(),
        }
    }
}

pub async fn build_answer(
    store: &Arc<dyn Store>,
    providers: &ProviderSet,
    policy: &Policy,
    query: &str,
    options: &AnswerOptions,
) -> Result<Answer, StoreError> {
    build_answer_inner(store, providers, policy, query, options, None, true).await
}

/// Variant used by the server with the configured billing engine. External
/// engines must not post local ledger entries.
pub async fn build_answer_with_ledger(
    store: &Arc<dyn Store>,
    providers: &ProviderSet,
    policy: &Policy,
    query: &str,
    options: &AnswerOptions,
    local_ledger: bool,
) -> Result<Answer, StoreError> {
    build_answer_inner(store, providers, policy, query, options, None, local_ledger).await
}

/// Streaming variant: token deltas are forwarded on `tx` as the gateway
/// produces them. The returned Answer is complete, with citations aligned.
pub async fn build_answer_stream(
    store: &Arc<dyn Store>,
    providers: &ProviderSet,
    policy: &Policy,
    query: &str,
    options: &AnswerOptions,
    tx: mpsc::Sender<String>,
) -> Result<Answer, StoreError> {
    build_answer_inner(store, providers, policy, query, options, Some(tx), true).await
}

pub async fn build_answer_stream_with_ledger(
    store: &Arc<dyn Store>,
    providers: &ProviderSet,
    policy: &Policy,
    query: &str,
    options: &AnswerOptions,
    tx: mpsc::Sender<String>,
    local_ledger: bool,
) -> Result<Answer, StoreError> {
    build_answer_inner(store, providers, policy, query, options, Some(tx), local_ledger).await
}

async fn build_answer_inner(
    store: &Arc<dyn Store>,
    providers: &ProviderSet,
    policy: &Policy,
    query: &str,
    options: &AnswerOptions,
    stream_tx: Option<mpsc::Sender<String>>,
    local_ledger: bool,
) -> Result<Answer, StoreError> {
    let response = search_with_tuning(
        store,
        providers,
        policy,
        query,
        options.mode,
        options.limit,
        &options.tuning,
    )
    .await?;
    let mut degraded = response.degraded.clone();

    if response.hits.is_empty() {
        sort_degraded(&mut degraded);
        return Ok(Answer {
            answer: None,
            citations: Vec::new(),
            confidence: Confidence::low("no_citations"),
            conflicts: Vec::new(),
            degraded,
            knowledge_as_of: None,
            mode: AnswerMode::Extractive,
            model: None,
            usage: None,
            cache_hit: false,
            conversation_id: None,
            message_id: None,
            summary_kind: None,
            summary_source: None,
        });
    }

    let citation_count = response.hits.len().min(5);
    let cited = &response.hits[..citation_count];
    let extractive_text = extractive_answer(cited);

    let mut mode = AnswerMode::Extractive;
    let mut answer_text = Some(extractive_text);
    let mut selected_indices: Option<Vec<usize>> = None;

    let gateway = crate::gateway::Gateway::new(store.clone()).with_local_ledger(local_ledger);
    let sources = cited
        .iter()
        .enumerate()
        .map(|(index, hit)| {
            format!(
                "[{}] {}:{}-{}\n{}",
                index + 1,
                hit.path,
                hit.line_start,
                hit.line_end,
                crate::util::truncate_chars(&hit.quote, 1200)
            )
        })
        .collect::<Vec<String>>()
        .join("\n\n");
    let system = "Answer only from the supplied SOURCES. Do not use outside knowledge. \
Return strict JSON: {\"answer\":\"...\",\"citations\":[1,2]}. \
Citation numbers are 1-based source ids. If the sources do not answer the question, return \
{\"answer\":\"证据不足\",\"citations\":[]}.";
    let user = format!("QUESTION:\n{query}\n\nSOURCES:\n{sources}");
    let request_id = format!("ans_{}", crate::util::random_hex(10));
    let gateway_request = crate::gateway::GatewayRequest {
        request_id,
        principal: policy.principal.clone(),
        space_id: policy.principal.allowed_space_ids.first().cloned(),
        role: "user".to_string(),
        alias: "default".to_string(),
        messages: vec![
            crate::gateway::ChatMessage { role: "system".into(), content: system.into() },
            crate::gateway::ChatMessage { role: "user".into(), content: user },
        ],
        max_output_tokens: 1024,
        temperature: 0.0,
        source: "kb".to_string(),
        rewritten_query: None,
    };
    let mut answer_usage = None;
    let mut answer_model = None;
    let gateway_result = if let Some(tx) = stream_tx {
        gateway.chat_stream(&gateway_request, tx).await
    } else {
        gateway.chat(&gateway_request).await
    };
    match gateway_result {
        Ok(result) => {
            if !result.degraded_codes.is_empty() {
                degraded.push(crate::contract::Degraded::new(
                    "stream_fallback",
                    crate::contract::DegradedCategory::Performance,
                    "stream",
                    "L1",
                    "L0",
                    crate::contract::DegradedReason::Unavailable,
                    "upstream does not support streaming; the answer was buffered",
                    "non_stream_answer",
                ));
            }
            match parse_llm_answer(&result.content, cited.len()) {
            Ok(parsed) if !parsed.citations.is_empty() => {
                mode = AnswerMode::Generative;
                selected_indices = Some(parsed.citations.clone());
                answer_text = Some(parsed.answer);
                answer_model = Some(result.model);
                answer_usage = Some(crate::contract::AnswerUsage {
                    input_tokens: result.usage.input_tokens,
                    output_tokens: result.usage.output_tokens,
                    cached_tokens: result.usage.cached_tokens,
                    token_source: result.usage.token_source,
                    cost_micros: result.usage.cost_micros,
                    currency: result.usage.currency,
                });
            }
            Ok(_) => degraded.push(Degraded::citation_alignment_failed()),
            Err(_) => degraded.push(Degraded::citation_alignment_failed()),
            }
        }
        Err(err) => {
            if err.code == "quota_exceeded" {
                return Err(StoreError::QuotaExceeded(err.message));
            }
            degraded.push(Degraded::llm_unavailable(&err.message));
        }
    }

    let cited_hits: Vec<&crate::store::SearchHit> = match &selected_indices {
        Some(indices) => indices
            .iter()
            .filter_map(|index| cited.get((*index).saturating_sub(1)))
            .collect(),
        None => cited.iter().collect(),
    };
    let citations: Vec<Citation> = cited_hits
        .iter()
        .map(|hit| Citation {
            doc_id: hit.doc_id.clone(),
            chunk_id: hit.chunk_id.clone(),
            path: hit.path.clone(),
            line_start: hit.line_start,
            line_end: hit.line_end,
            quote: crate::util::truncate_chars(&hit.quote, 1000),
            source_url: None,
        })
        .collect();

    let conflicts = detect_conflicts(&cited_hits);
    let knowledge_as_of = cited_hits
        .iter()
        .map(|hit| hit.governance.valid_from.clone())
        .min();
    let top_margin = match (response.hits.first(), response.hits.get(1)) {
        (Some(first), Some(second)) => (second.score - first.score).abs().min(1.0) * 0.15,
        (Some(_), None) => 0.05,
        _ => 0.0,
    };
    let confidence = Confidence::heuristic(
        citations.len(),
        top_margin,
        &degraded,
        conflicts.len(),
    );
    sort_degraded(&mut degraded);

    Ok(Answer {
        answer: answer_text,
        citations,
        confidence,
        conflicts,
        degraded,
        knowledge_as_of,
        mode,
        model: answer_model,
        usage: answer_usage,
        cache_hit: false,
        conversation_id: None,
        message_id: None,
        summary_kind: None,
        summary_source: None,
    })
}

fn extractive_answer(hits: &[crate::store::SearchHit]) -> String {
    hits.iter()
        .enumerate()
        .map(|(index, hit)| {
            format!(
                "[{}] {}:{}-{}\n{}",
                index + 1,
                hit.path,
                hit.line_start,
                hit.line_end,
                crate::util::truncate_chars(hit.quote.trim(), 800)
            )
        })
        .collect::<Vec<String>>()
        .join("\n\n")
}

#[derive(Debug, Deserialize)]
struct LlmAnswer {
    answer: String,
    #[serde(default)]
    citations: Vec<usize>,
}

fn parse_llm_answer(raw: &str, source_count: usize) -> Result<LlmAnswer, String> {
    let text = crate::util::strip_code_fence(raw);
    let mut parsed: LlmAnswer =
        serde_json::from_str(text).map_err(|err| format!("invalid LLM JSON: {err}"))?;
    parsed.answer = parsed.answer.trim().to_string();
    if parsed.answer.is_empty() {
        return Err("empty answer".to_string());
    }
    let mut seen = Vec::new();
    for index in &parsed.citations {
        if *index == 0 || *index > source_count {
            return Err(format!("citation index {index} out of range"));
        }
    }
    parsed.citations.retain(|index| {
        if seen.contains(index) {
            false
        } else {
            seen.push(*index);
            true
        }
    });
    Ok(parsed)
}

fn detect_conflicts(hits: &[&crate::store::SearchHit]) -> Vec<Conflict> {
    let mut by_title: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for hit in hits {
        if hit.title.trim().is_empty() {
            continue;
        }
        by_title
            .entry(hit.title.trim().to_string())
            .or_default()
            .push(hit.doc_id.clone());
    }
    let mut conflicts = Vec::new();
    for (index, (topic, mut docs)) in by_title.into_iter().enumerate() {
        docs.sort();
        docs.dedup();
        if docs.len() > 1 {
            conflicts.push(Conflict {
                conflict_id: format!("cf_{}", index + 1),
                topic,
                candidates: docs,
                reason: ConflictReason::Version,
                resolution: ConflictResolution::Unresolved,
                preferred_doc_id: None,
            });
        }
    }
    conflicts
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn llm_answer_requires_valid_citations() {
        let parsed = parse_llm_answer(
            "```json\n{\"answer\":\"ok\",\"citations\":[1,2,2]}\n```",
            3,
        )
        .unwrap();
        assert_eq!(parsed.citations, vec![1, 2]);
        assert!(parse_llm_answer("{\"answer\":\"ok\",\"citations\":[4]}", 3).is_err());
        assert!(parse_llm_answer("not json", 3).is_err());
    }
}
