use crate::config::RetrievalConfig;
use crate::contract::{sort_degraded, Degraded};
use crate::error::StoreError;
use crate::governance::Policy;
use crate::providers::ProviderSet;
use crate::store::{EventSearchParams, SearchHit, Store};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SearchMode {
    Bm25,
    Bm25Judge,
    Event,
    Hybrid,
    Auto,
}

impl SearchMode {
    pub fn parse(value: &str) -> Result<Self, String> {
        match value.trim().to_ascii_lowercase().as_str() {
            "bm25" | "keyword" => Ok(SearchMode::Bm25),
            "bm25_judge" => Ok(SearchMode::Bm25Judge),
            "event" | "event_entity" => Ok(SearchMode::Event),
            "hybrid" => Ok(SearchMode::Hybrid),
            "auto" => Ok(SearchMode::Auto),
            other => Err(format!(
                "unknown search mode {other:?}; expected bm25, bm25_judge, event, hybrid or auto"
            )),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            SearchMode::Bm25 => "bm25",
            SearchMode::Bm25Judge => "bm25_judge",
            SearchMode::Event => "event",
            SearchMode::Hybrid => "hybrid",
            SearchMode::Auto => "auto",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchTuning {
    /// Recall-stage language filter; also part of the result cache key.
    pub lang: Option<String>,
    pub judge_alias: String,
    /// False for external billing engines: the judge call must not post local
    /// ledger entries.
    pub billing_local_ledger: bool,
    pub event_depth: u32,
    pub event_max_seeds: usize,
    pub event_max_events: usize,
    pub event_timeout_ms: u64,
    pub event_text_weight: f64,
    pub rrf_bm25_weight: f64,
    pub rrf_dense_weight: f64,
    pub rrf_event_weight: f64,
    /// Rerank the fused hybrid list with the judge when one is configured.
    pub judge_after_hybrid: bool,
}

impl Default for SearchTuning {
    fn default() -> Self {
        Self::from_retrieval(&RetrievalConfig::default())
    }
}

impl SearchTuning {
    pub fn from_retrieval(config: &RetrievalConfig) -> Self {
        Self {
            lang: None,
            judge_alias: config.judge_alias.clone(),
            billing_local_ledger: true,
            event_depth: config.event_depth.clamp(1, 2),
            event_max_seeds: config.event_max_seeds.max(1),
            event_max_events: config.event_max_events.max(1),
            event_timeout_ms: config.event_timeout_ms.max(1),
            event_text_weight: config.event_text_weight.max(0.0),
            rrf_bm25_weight: config.rrf_bm25_weight.max(0.0),
            rrf_dense_weight: config.rrf_dense_weight.max(0.0),
            rrf_event_weight: config.rrf_event_weight.max(0.0),
            judge_after_hybrid: config.judge_after_hybrid,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResponse {
    pub query: String,
    pub mode_requested: SearchMode,
    pub mode_used: SearchMode,
    pub lang: Option<String>,
    pub keyword_engine: String,
    pub as_of: String,
    pub hits: Vec<SearchHit>,
    pub degraded: Vec<Degraded>,
}

pub async fn search(
    store: &Arc<dyn Store>,
    providers: &ProviderSet,
    policy: &Policy,
    query: &str,
    mode: SearchMode,
    limit: usize,
) -> Result<SearchResponse, StoreError> {
    search_with_tuning(
        store,
        providers,
        policy,
        query,
        mode,
        limit,
        &SearchTuning::default(),
    )
    .await
}

pub async fn search_with_tuning(
    store: &Arc<dyn Store>,
    providers: &ProviderSet,
    policy: &Policy,
    query: &str,
    mode: SearchMode,
    limit: usize,
    tuning: &SearchTuning,
) -> Result<SearchResponse, StoreError> {
    let query = query.trim();
    if query.is_empty() {
        return Err(StoreError::Invalid("query must not be empty".into()));
    }
    let limit = limit.clamp(1, 500);
    let policy_owned = policy.clone().with_lang(tuning.lang.clone());
    let policy = &policy_owned;
    let mut degraded = Vec::new();
    let mode_used = resolve_mode(store, providers, policy, mode, &mut degraded).await?;

    let mut hits = match mode_used {
        SearchMode::Bm25 => keyword_candidates(store, policy, query, limit, &mut degraded).await?,
        SearchMode::Bm25Judge => {
            let mut keyword =
                keyword_candidates(store, policy, query, limit, &mut degraded).await?;
            apply_judge(
                store,
                policy,
                tuning,
                providers,
                query,
                &mut keyword,
                &mut degraded,
            )
            .await;
            keyword
        }
        SearchMode::Event => {
            let event_hits =
                event_candidates(store, providers, policy, query, tuning, true, &mut degraded)
                    .await?;
            match event_hits {
                Some(hits) if !hits.is_empty() => hits,
                _ => keyword_candidates(store, policy, query, limit, &mut degraded).await?,
            }
        }
        SearchMode::Hybrid => {
            let mut keyword =
                keyword_candidates(store, policy, query, limit, &mut degraded).await?;
            let mut rankings: Vec<(f64, Vec<SearchHit>)> = Vec::new();
            if !keyword.is_empty() {
                rankings.push((
                    tuning.rrf_bm25_weight.max(0.0),
                    std::mem::take(&mut keyword),
                ));
            }

            match build_dense_hits(store, providers, policy, query, limit).await {
                Ok((dense_hits, dense_degraded)) => {
                    degraded.extend(dense_degraded);
                    if !dense_hits.is_empty() {
                        rankings.push((tuning.rrf_dense_weight.max(0.0), dense_hits));
                    }
                }
                Err(DenseBuildError::Provider(err)) => {
                    degraded.push(Degraded::embedding_unavailable(&err));
                }
                Err(DenseBuildError::Store(err)) => return Err(err),
            }

            let event_hits = event_candidates(
                store,
                providers,
                policy,
                query,
                tuning,
                mode == SearchMode::Hybrid,
                &mut degraded,
            )
            .await?;
            if let Some(event_hits) = event_hits {
                if !event_hits.is_empty() {
                    rankings.push((tuning.rrf_event_weight.max(0.0), event_hits));
                }
            }
            let mut fused = if rankings.is_empty() {
                Vec::new()
            } else {
                rrf_fuse(rankings, 60.0)
            };
            // Hybrid does not require a judge: absence is silent. A configured
            // judge that fails reports decision_unavailable inside apply_judge.
            if should_judge_hybrid(tuning, providers) {
                apply_judge(
                    store,
                    policy,
                    tuning,
                    providers,
                    query,
                    &mut fused,
                    &mut degraded,
                )
                .await;
            }
            fused
        }
        SearchMode::Auto => unreachable!("auto is resolved before execution"),
    };

    let before = hits.len();
    hits.retain(|hit| policy.can_read(&hit.governance));
    if hits.len() != before {
        let dropped = before - hits.len();
        degraded.push(Degraded::acl_second_check_dropped(dropped));
        crate::audit::record(
            store,
            &policy.principal.tenant_id,
            None,
            &policy.principal.principal_id,
            "user",
            "acl.filtered",
            "search",
            "acl",
            "denied",
            200,
            None,
            None,
            None,
            serde_json::json!({"dropped": dropped, "query_hash": crate::util::sha256_hex(query.as_bytes()), "query_length": query.chars().count()}),
        )
        .await;
    }
    hits.truncate(limit);
    sort_degraded(&mut degraded);

    Ok(SearchResponse {
        query: query.to_string(),
        mode_requested: mode,
        mode_used,
        lang: tuning.lang.clone(),
        keyword_engine: match store.backend() {
            "sqlite" => "sqlite-fts5-bm25",
            "postgres" => "postgres-tsvector-ts_rank_cd",
            other => other,
        }
        .to_string(),
        as_of: policy.as_of.clone(),
        hits,
        degraded,
    })
}

/// Signals that decide `auto` on SQLite. A probe is `None` when its provider is
/// not configured or the decision no longer needs it.
struct AutoSignals {
    embedding_configured: bool,
    embeddings: Option<Result<bool, String>>,
    extractor_configured: bool,
    events: Option<Result<bool, String>>,
}

/// Deterministic `auto` decision for SQLite plus the degradations it implies.
fn auto_mode_from_signals(signals: AutoSignals) -> (SearchMode, Vec<Degraded>) {
    let mut degraded = Vec::new();
    let mut hybrid = false;
    let mut vectors_missing = false;
    if signals.embedding_configured {
        match signals.embeddings {
            Some(Ok(true)) => hybrid = true,
            Some(Ok(false)) => vectors_missing = true,
            Some(Err(detail)) => degraded.push(Degraded::embedding_unavailable(&detail)),
            None => {}
        }
    }
    if !hybrid && signals.extractor_configured {
        match signals.events {
            Some(Ok(true)) => hybrid = true,
            Some(Ok(false)) => {}
            Some(Err(detail)) => degraded.push(Degraded::event_extraction_unavailable(&detail)),
            None => {}
        }
    }
    // When hybrid runs, its dense channel reports the empty vector index itself;
    // only the plain-BM25 outcome needs the degradation recorded here.
    if !hybrid && vectors_missing {
        degraded.push(Degraded::index_missing());
    }
    let mode = if hybrid {
        SearchMode::Hybrid
    } else {
        SearchMode::Bm25
    };
    (mode, degraded)
}

async fn resolve_mode(
    store: &Arc<dyn Store>,
    providers: &ProviderSet,
    policy: &Policy,
    requested: SearchMode,
    degraded: &mut Vec<Degraded>,
) -> Result<SearchMode, StoreError> {
    match requested {
        SearchMode::Auto => {
            if store.backend() == "postgres" {
                return Ok(SearchMode::Hybrid);
            }
            let embeddings = if providers.embedding.is_some() {
                Some(
                    store
                        .has_embeddings(policy)
                        .await
                        .map_err(|err| err.to_string()),
                )
            } else {
                None
            };
            let events = if !matches!(embeddings, Some(Ok(true))) && providers.extractor.is_some() {
                Some(
                    store
                        .has_events(policy)
                        .await
                        .map_err(|err| err.to_string()),
                )
            } else {
                None
            };
            let (mode, auto_degraded) = auto_mode_from_signals(AutoSignals {
                embedding_configured: providers.embedding.is_some(),
                embeddings,
                extractor_configured: providers.extractor.is_some(),
                events,
            });
            degraded.extend(auto_degraded);
            Ok(mode)
        }
        other => Ok(other),
    }
}

fn should_judge_hybrid(tuning: &SearchTuning, providers: &ProviderSet) -> bool {
    tuning.judge_after_hybrid && providers.judge.is_some()
}

async fn keyword_candidates(
    store: &Arc<dyn Store>,
    policy: &Policy,
    query: &str,
    limit: usize,
    degraded: &mut Vec<Degraded>,
) -> Result<Vec<SearchHit>, StoreError> {
    let result = store.search_keyword(policy, query, limit as i64 * 3).await;
    let mut hits = match result {
        Ok(hits) => hits,
        Err(StoreError::FtsUnavailable) => {
            degraded.push(Degraded::keyword_unavailable());
            Vec::new()
        }
        Err(err) => return Err(err),
    };
    if hits.iter().any(|hit| hit.exact_fallback) {
        degraded.push(Degraded::keyword_exact_fallback());
    }
    hits.truncate(limit);
    Ok(hits)
}

async fn event_candidates(
    store: &Arc<dyn Store>,
    providers: &ProviderSet,
    policy: &Policy,
    query: &str,
    tuning: &SearchTuning,
    announce_missing: bool,
    degraded: &mut Vec<Degraded>,
) -> Result<Option<Vec<SearchHit>>, StoreError> {
    let Some(_extractor) = &providers.extractor else {
        if providers.extractor_requested {
            degraded.push(Degraded::event_extraction_unavailable(
                providers
                    .extractor_error
                    .as_deref()
                    .unwrap_or("extractor is not configured"),
            ));
        } else if announce_missing {
            degraded.push(Degraded::event_not_enabled());
        }
        return Ok(None);
    };

    match store.has_events(policy).await {
        Ok(true) => {}
        Ok(false) => {
            degraded.push(Degraded::event_extraction_unavailable(
                "no extracted events are indexed",
            ));
            return Ok(None);
        }
        Err(err) => {
            degraded.push(Degraded::event_extraction_unavailable(&err.to_string()));
            return Ok(None);
        }
    }

    let timeout = Duration::from_millis(tuning.event_timeout_ms.max(1));
    let future = store.search_events(
        policy,
        EventSearchParams {
            query,
            depth: tuning.event_depth,
            max_seeds: tuning.event_max_seeds as i64,
            max_events: tuning.event_max_events as i64,
            text_weight: tuning.event_text_weight,
            limit: (tuning.event_max_events as i64).max(1),
            timeout_ms: tuning.event_timeout_ms,
        },
    );
    match tokio::time::timeout(timeout, future).await {
        Ok(Ok(hits)) if hits.is_empty() => {
            degraded.push(Degraded::event_no_seed());
            Ok(None)
        }
        Ok(Ok(hits)) => Ok(Some(hits)),
        Ok(Err(StoreError::Timeout(detail))) => {
            degraded.push(Degraded::event_channel_timeout(&detail));
            Ok(None)
        }
        Ok(Err(err)) => {
            degraded.push(Degraded::event_extraction_unavailable(&err.to_string()));
            Ok(None)
        }
        Err(_) => {
            degraded.push(Degraded::event_channel_timeout(&format!(
                "{}ms",
                tuning.event_timeout_ms
            )));
            Ok(None)
        }
    }
}

async fn apply_judge(
    store: &Arc<dyn Store>,
    policy: &Policy,
    tuning: &SearchTuning,
    providers: &ProviderSet,
    query: &str,
    hits: &mut Vec<SearchHit>,
    degraded: &mut Vec<Degraded>,
) {
    if hits.is_empty() {
        return;
    }
    let top = hits.len().min(30);
    let candidates: Vec<String> = hits[..top]
        .iter()
        .map(|hit| crate::util::truncate_chars(&hit.quote, 600))
        .collect();
    let prompt = format!(
        "Score each candidate's relevance to the query from 0.0 to 1.0. Return strict JSON {{\"scores\":[...]}} with exactly {} scores in input order.\nQUERY:\n{}\nCANDIDATES:\n{}",
        top,
        query,
        candidates.iter().enumerate().map(|(index, candidate)| format!("[{}] {}", index + 1, candidate)).collect::<Vec<_>>().join("\n")
    );
    let gateway =
        crate::gateway::Gateway::new(store.clone()).with_local_ledger(tuning.billing_local_ledger);
    let request = crate::gateway::GatewayRequest {
        request_id: format!("judge_{}", crate::util::random_hex(10)),
        principal: policy.principal.clone(),
        space_id: policy.principal.allowed_space_ids.first().cloned(),
        role: "user".to_string(),
        alias: tuning.judge_alias.clone(),
        messages: vec![crate::gateway::ChatMessage {
            role: "user".into(),
            content: prompt,
        }],
        max_output_tokens: 256,
        temperature: 0.0,
        source: "judge".to_string(),
        rewritten_query: None,
    };
    if let Ok(response) = gateway.chat(&request).await {
        let text = crate::util::strip_code_fence(&response.content);
        if let Ok(value) = serde_json::from_str::<serde_json::Value>(text) {
            if let Some(scores) = value.get("scores").and_then(serde_json::Value::as_array) {
                let values: Vec<f64> = scores
                    .iter()
                    .filter_map(serde_json::Value::as_f64)
                    .collect();
                if values.len() == top {
                    apply_judge_scores(hits, &values);
                    return;
                }
            }
        }
        degraded.push(Degraded::decision_unavailable(
            "judge returned invalid scores",
        ));
        return;
    }
    // Preserve the old adapter path as a fallback, but report the gateway failure.
    if let Some(judge) = &providers.judge {
        match judge.rerank(query, &candidates).await {
            Ok(scores) if scores.len() == top => {
                let values: Vec<f64> = scores.into_iter().map(f64::from).collect();
                apply_judge_scores(hits, &values);
            }
            Ok(_) => degraded.push(Degraded::decision_unavailable(
                "judge returned wrong score count",
            )),
            Err(err) => degraded.push(Degraded::decision_unavailable(&err.to_string())),
        }
    } else {
        degraded.push(Degraded::decision_unavailable(
            providers
                .judge_error
                .as_deref()
                .unwrap_or("judge is not configured"),
        ));
    }
}

/// Reorder the judge window (Top 30) only: membership and the tail keep their
/// positions. A score count that does not match the window is a no-op.
fn apply_judge_scores(hits: &mut Vec<SearchHit>, values: &[f64]) {
    let top = hits.len().min(30);
    if values.len() != top {
        return;
    }
    for (hit, score) in hits.iter_mut().take(top).zip(values.iter().copied()) {
        hit.rerank_score = Some(score);
    }
    let mut head: Vec<SearchHit> = hits.drain(..top).collect();
    head.sort_by(|a, b| {
        b.rerank_score
            .partial_cmp(&a.rerank_score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.chunk_id.cmp(&b.chunk_id))
    });
    head.append(hits);
    *hits = head;
}

enum DenseBuildError {
    Provider(String),
    Store(StoreError),
}

async fn build_dense_hits(
    store: &Arc<dyn Store>,
    providers: &ProviderSet,
    policy: &Policy,
    query: &str,
    limit: usize,
) -> Result<(Vec<SearchHit>, Vec<Degraded>), DenseBuildError> {
    let mut degraded = Vec::new();
    let embedding = match &providers.embedding {
        Some(provider) => provider,
        None => {
            if providers.embedding_requested {
                degraded.push(Degraded::embedding_unavailable(
                    providers
                        .embedding_error
                        .as_deref()
                        .unwrap_or("embedding provider is not configured"),
                ));
            } else {
                degraded.push(Degraded::dense_not_enabled());
            }
            return Ok((Vec::new(), degraded));
        }
    };

    let query_vector = embedding
        .embed(&[query.to_string()])
        .await
        .map_err(|err| DenseBuildError::Provider(err.to_string()))?
        .into_iter()
        .next()
        .ok_or_else(|| {
            DenseBuildError::Provider("embedding provider returned no vector".to_string())
        })?;

    let candidates = store
        .fetch_embedding_candidates(policy, (limit as i64).saturating_mul(20).max(100))
        .await
        .map_err(DenseBuildError::Store)?;
    if candidates.is_empty() {
        degraded.push(Degraded::index_missing());
        return Ok((Vec::new(), degraded));
    }

    let mut scored: Vec<(f64, SearchHit)> = candidates
        .into_iter()
        .map(|candidate| {
            let score = cosine(&query_vector, &candidate.embedding);
            (score, candidate.into_hit(score))
        })
        .collect();
    scored.sort_by(|a, b| {
        b.0.partial_cmp(&a.0)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.1.chunk_id.cmp(&b.1.chunk_id))
    });
    scored.truncate(limit);
    Ok((scored.into_iter().map(|(_, hit)| hit).collect(), degraded))
}

pub fn cosine(a: &[f32], b: &[f32]) -> f64 {
    if a.is_empty() || b.is_empty() || a.len() != b.len() {
        return 0.0;
    }
    let mut dot = 0.0f64;
    let mut na = 0.0f64;
    let mut nb = 0.0f64;
    for (x, y) in a.iter().zip(b.iter()) {
        dot += (*x as f64) * (*y as f64);
        na += (*x as f64) * (*x as f64);
        nb += (*y as f64) * (*y as f64);
    }
    if na == 0.0 || nb == 0.0 {
        return 0.0;
    }
    dot / (na.sqrt() * nb.sqrt())
}

/// Reciprocal Rank Fusion. `rankings` is `(weight, ordered hits)`. Only rank
/// positions are compared, never raw scores from different channels.
pub fn rrf_fuse(rankings: Vec<(f64, Vec<SearchHit>)>, k: f64) -> Vec<SearchHit> {
    let k = if k <= 0.0 { 60.0 } else { k };
    let mut merged: HashMap<String, (SearchHit, f64)> = HashMap::new();
    for (weight, hits) in rankings {
        for (index, hit) in hits.into_iter().enumerate() {
            let contribution = weight / (k + (index + 1) as f64);
            merged
                .entry(hit.chunk_id.clone())
                .and_modify(|(_, score)| *score += contribution)
                .or_insert((hit, contribution));
        }
    }
    let mut fused: Vec<(SearchHit, f64)> = merged.into_values().collect();
    fused.sort_by(|(a, a_score), (b, b_score)| {
        b_score
            .partial_cmp(a_score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.chunk_id.cmp(&b.chunk_id))
    });
    fused
        .into_iter()
        .map(|(mut hit, score)| {
            hit.score = score;
            hit.rerank_score = None;
            hit
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::governance::{Acl, GovernanceFields};
    use crate::providers::{JudgeProvider, ProviderError, ProviderSet};
    use async_trait::async_trait;

    struct FakeJudge;

    #[async_trait]
    impl JudgeProvider for FakeJudge {
        async fn rerank(
            &self,
            _query: &str,
            candidates: &[String],
        ) -> Result<Vec<f32>, ProviderError> {
            Ok(vec![0.5; candidates.len()])
        }

        fn id(&self) -> String {
            "fake-judge".to_string()
        }
    }

    fn hit(id: &str) -> SearchHit {
        SearchHit {
            chunk_id: id.to_string(),
            doc_id: format!("doc_{id}"),
            path: format!("{id}.md"),
            title: id.to_string(),
            line_start: 1,
            line_end: 2,
            quote: id.to_string(),
            score: 0.0,
            governance: GovernanceFields {
                tenant_id: "t".into(),
                space_id: "s".into(),
                acl_json: Acl::allow_all().to_json(),
                acl_principals: "|*|".into(),
                lang: "zh".into(),
                valid_from: "2026-01-01T00:00:00Z".into(),
                valid_to: None,
                superseded_by: None,
                version: 1,
            },
            event_id: None,
            exact_fallback: false,
            rerank_score: None,
        }
    }

    #[test]
    fn rrf_prefers_consensus() {
        let keyword = vec![hit("a"), hit("b"), hit("c")];
        let dense = vec![hit("b"), hit("a"), hit("d")];
        let fused = rrf_fuse(vec![(1.0, keyword), (1.0, dense)], 60.0);
        assert_eq!(fused[0].chunk_id, "a");
        assert_eq!(fused[1].chunk_id, "b");
    }

    #[test]
    fn cosine_handles_dimension_mismatch() {
        assert_eq!(cosine(&[1.0, 0.0], &[1.0]), 0.0);
        assert!((cosine(&[1.0, 0.0], &[1.0, 0.0]) - 1.0).abs() < 1e-9);
    }

    #[test]
    fn modes_parse_aliases() {
        assert_eq!(
            SearchMode::parse("event_entity").unwrap(),
            SearchMode::Event
        );
        assert_eq!(
            SearchMode::parse("bm25_judge").unwrap(),
            SearchMode::Bm25Judge
        );
        assert!(SearchMode::parse("unknown").is_err());
    }

    // `resolve_mode` needs a live Store; these tests pin its SQLite decision
    // table through `auto_mode_from_signals`. PostgreSQL short-circuits to
    // Hybrid before any probe and is covered by `pg_sql_guard`/integration runs.

    #[test]
    fn auto_embedding_with_vectors_uses_hybrid() {
        let (mode, degraded) = auto_mode_from_signals(AutoSignals {
            embedding_configured: true,
            embeddings: Some(Ok(true)),
            extractor_configured: false,
            events: None,
        });
        assert_eq!(mode, SearchMode::Hybrid);
        assert!(degraded.is_empty());
    }

    #[test]
    fn auto_embedding_without_vectors_degrades_to_bm25() {
        let (mode, degraded) = auto_mode_from_signals(AutoSignals {
            embedding_configured: true,
            embeddings: Some(Ok(false)),
            extractor_configured: false,
            events: None,
        });
        assert_eq!(mode, SearchMode::Bm25);
        assert_eq!(degraded.len(), 1);
        assert_eq!(degraded[0].code, "index_missing");
    }

    #[test]
    fn auto_extractor_events_uses_hybrid_without_embeddings() {
        let (mode, degraded) = auto_mode_from_signals(AutoSignals {
            embedding_configured: false,
            embeddings: None,
            extractor_configured: true,
            events: Some(Ok(true)),
        });
        assert_eq!(mode, SearchMode::Hybrid);
        assert!(degraded.is_empty());
    }

    #[test]
    fn auto_without_capabilities_stays_bm25() {
        let (mode, degraded) = auto_mode_from_signals(AutoSignals {
            embedding_configured: false,
            embeddings: None,
            extractor_configured: false,
            events: None,
        });
        assert_eq!(mode, SearchMode::Bm25);
        assert!(degraded.is_empty());
    }

    #[test]
    fn judge_after_hybrid_defaults_on_and_passes_through() {
        assert!(SearchTuning::default().judge_after_hybrid);
        let mut retrieval = RetrievalConfig::default();
        assert!(retrieval.judge_after_hybrid);
        retrieval.judge_after_hybrid = false;
        assert!(!SearchTuning::from_retrieval(&retrieval).judge_after_hybrid);
    }

    #[test]
    fn hybrid_judge_runs_only_when_configured() {
        let mut providers = ProviderSet::with_none();
        assert!(!should_judge_hybrid(&SearchTuning::default(), &providers));
        providers.judge = Some(Arc::new(FakeJudge));
        assert!(should_judge_hybrid(&SearchTuning::default(), &providers));
        let tuning = SearchTuning {
            judge_after_hybrid: false,
            ..Default::default()
        };
        assert!(!should_judge_hybrid(&tuning, &providers));
    }

    #[test]
    fn hybrid_judge_runs_after_rrf() {
        let ordered: Vec<SearchHit> = (0..35).map(|index| hit(&format!("h{index:02}"))).collect();
        let mut fused = rrf_fuse(vec![(1.0, ordered)], 60.0);
        let before: Vec<String> = fused.iter().map(|hit| hit.chunk_id.clone()).collect();
        let tail_before: Vec<String> = fused[30..].iter().map(|hit| hit.chunk_id.clone()).collect();
        let values: Vec<f64> = (0..30).map(|index| index as f64).collect();
        apply_judge_scores(&mut fused, &values);
        assert_eq!(fused.len(), before.len());
        let after: Vec<String> = fused.iter().map(|hit| hit.chunk_id.clone()).collect();
        let mut sorted_before = before.clone();
        let mut sorted_after = after.clone();
        sorted_before.sort();
        sorted_after.sort();
        assert_eq!(
            sorted_after, sorted_before,
            "judge must not change membership"
        );
        assert_eq!(fused[0].chunk_id, "h29");
        assert_eq!(fused[29].chunk_id, "h00");
        assert_eq!(
            fused[30..]
                .iter()
                .map(|hit| hit.chunk_id.clone())
                .collect::<Vec<_>>(),
            tail_before,
            "judge window is Top 30"
        );
        assert!(fused[..30].iter().all(|hit| hit.rerank_score.is_some()));
        assert!(fused[30..].iter().all(|hit| hit.rerank_score.is_none()));
    }

    #[test]
    fn hybrid_without_judge_keeps_order() {
        let providers = ProviderSet::with_none();
        let tuning = SearchTuning::default();
        let fused = rrf_fuse(
            vec![
                (1.0, vec![hit("a"), hit("b"), hit("c")]),
                (1.0, vec![hit("b")]),
            ],
            60.0,
        );
        let before: Vec<String> = fused.iter().map(|hit| hit.chunk_id.clone()).collect();
        if should_judge_hybrid(&tuning, &providers) {
            unreachable!("no judge is configured");
        }
        assert_eq!(
            fused
                .iter()
                .map(|hit| hit.chunk_id.clone())
                .collect::<Vec<_>>(),
            before
        );
        assert!(fused.iter().all(|hit| hit.rerank_score.is_none()));
    }
}
