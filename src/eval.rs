use crate::governance::Policy;
use crate::providers::ProviderSet;
use crate::search::{search_with_tuning, SearchMode, SearchTuning};
use crate::store::Store;
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::Arc;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvalReport {
    pub dataset: String,
    pub k: usize,
    pub queries: usize,
    pub recall_at_k: f64,
    pub mrr: f64,
    pub per_query: Vec<EvalQueryResult>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvalQueryResult {
    pub query: String,
    pub recall: f64,
    pub reciprocal_rank: f64,
    pub first_hit_rank: Option<usize>,
    pub retrieved_paths: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct EvalQuestion {
    query: String,
    #[serde(default)]
    expected_paths: Vec<String>,
    #[serde(default)]
    expected_doc_ids: Vec<String>,
}

pub async fn evaluate(
    store: &Arc<dyn Store>,
    providers: &ProviderSet,
    policy: &Policy,
    dataset: &Path,
    k: usize,
    mode: SearchMode,
    tuning: &SearchTuning,
) -> Result<EvalReport, anyhow::Error> {
    let text = std::fs::read_to_string(dataset)?;
    let mut questions = Vec::new();
    for (index, line) in text.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let question: EvalQuestion = serde_json::from_str(line)
            .map_err(|err| anyhow::anyhow!("{}:{}: {err}", dataset.display(), index + 1))?;
        questions.push(question);
    }
    if questions.is_empty() {
        return Err(anyhow::anyhow!("dataset is empty: {}", dataset.display()));
    }

    let k = k.max(1);
    let mut per_query = Vec::new();
    let mut recall_sum = 0.0f64;
    let mut mrr_sum = 0.0f64;
    for question in &questions {
        let response =
            search_with_tuning(store, providers, policy, &question.query, mode, k, tuning).await?;
        let mut first_rank = None;
        let mut retrieved_paths = Vec::new();
        for (index, hit) in response.hits.iter().enumerate() {
            retrieved_paths.push(hit.path.clone());
            if first_rank.is_none() && matches_expected(hit, question) {
                first_rank = Some(index + 1);
            }
        }
        let recall = if first_rank.is_some() { 1.0 } else { 0.0 };
        let reciprocal_rank = first_rank.map(|rank| 1.0 / rank as f64).unwrap_or(0.0);
        recall_sum += recall;
        mrr_sum += reciprocal_rank;
        per_query.push(EvalQueryResult {
            query: question.query.clone(),
            recall,
            reciprocal_rank,
            first_hit_rank: first_rank,
            retrieved_paths,
        });
    }

    Ok(EvalReport {
        dataset: dataset.display().to_string(),
        k,
        queries: questions.len(),
        recall_at_k: recall_sum / questions.len() as f64,
        mrr: mrr_sum / questions.len() as f64,
        per_query,
    })
}

fn matches_expected(hit: &crate::store::SearchHit, question: &EvalQuestion) -> bool {
    let path = normalize_path(&hit.path);
    for expected in &question.expected_paths {
        let expected = normalize_path(expected);
        if path == expected
            || path.ends_with(&format!("/{expected}"))
            || expected.ends_with(&format!("/{path}"))
        {
            return true;
        }
    }
    question
        .expected_doc_ids
        .iter()
        .any(|expected| expected == &hit.doc_id)
}

fn normalize_path(value: &str) -> String {
    value
        .replace('\\', "/")
        .trim_start_matches("./")
        .trim_end_matches('/')
        .to_ascii_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn path_matching_accepts_root_prefix_differences() {
        assert_eq!(normalize_path("./Docs\\A.md"), "docs/a.md");
        let question = EvalQuestion {
            query: "q".into(),
            expected_paths: vec!["docs/a.md".into()],
            expected_doc_ids: vec![],
        };
        let hit = crate::store::SearchHit {
            chunk_id: "c".into(),
            doc_id: "d".into(),
            path: "a.md".into(),
            title: "a".into(),
            line_start: 1,
            line_end: 1,
            quote: "q".into(),
            score: 1.0,
            governance: crate::governance::GovernanceFields {
                tenant_id: "t".into(),
                space_id: "s".into(),
                acl_json: crate::governance::Acl::allow_all().to_json(),
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
        };
        assert!(matches_expected(&hit, &question));
    }
}
