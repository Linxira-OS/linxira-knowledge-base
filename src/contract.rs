use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DegradedCategory {
    Evidence,
    Quality,
    Security,
    Performance,
}

impl DegradedCategory {
    pub fn priority(self) -> u8 {
        match self {
            DegradedCategory::Security => 0,
            DegradedCategory::Evidence => 1,
            DegradedCategory::Quality => 2,
            DegradedCategory::Performance => 3,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DegradedReason {
    NotEnabled,
    Unavailable,
    Timeout,
    ModelMissing,
    BudgetExceeded,
    Stale,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Degraded {
    pub code: String,
    pub category: DegradedCategory,
    pub capability: String,
    pub expected_level: String,
    pub actual_level: String,
    pub reason: DegradedReason,
    pub impact: String,
    pub fallback: String,
}

impl Degraded {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        code: &str,
        category: DegradedCategory,
        capability: &str,
        expected_level: &str,
        actual_level: &str,
        reason: DegradedReason,
        impact: &str,
        fallback: &str,
    ) -> Self {
        Self {
            code: code.to_string(),
            category,
            capability: capability.to_string(),
            expected_level: expected_level.to_string(),
            actual_level: actual_level.to_string(),
            reason,
            impact: impact.to_string(),
            fallback: fallback.to_string(),
        }
    }

    pub fn keyword_unavailable() -> Self {
        Self::new(
            "keyword_unavailable",
            DegradedCategory::Evidence,
            "keyword",
            "L0",
            "L0",
            DegradedReason::Unavailable,
            "keyword evidence is unavailable; answer may be empty",
            "none",
        )
    }

    pub fn dense_not_enabled() -> Self {
        Self::new(
            "dense_not_enabled",
            DegradedCategory::Evidence,
            "dense",
            "L1",
            "L0",
            DegradedReason::NotEnabled,
            "semantic candidates are not available",
            "keyword only",
        )
    }

    pub fn embedding_unavailable(detail: &str) -> Self {
        Self::new(
            "embedding_unavailable",
            DegradedCategory::Evidence,
            "embedding",
            "L1",
            "L0",
            DegradedReason::Unavailable,
            &format!("query embedding failed: {detail}"),
            "keyword only",
        )
    }

    pub fn index_missing() -> Self {
        Self::new(
            "index_missing",
            DegradedCategory::Evidence,
            "dense",
            "L1",
            "L0",
            DegradedReason::Unavailable,
            "no chunk embeddings are indexed",
            "keyword only",
        )
    }

    pub fn decision_unavailable(detail: &str) -> Self {
        Self::new(
            "decision_unavailable",
            DegradedCategory::Quality,
            "judge",
            "L1",
            "L0",
            DegradedReason::Unavailable,
            &format!("external judge failed: {detail}"),
            "keep recall order",
        )
    }

    pub fn llm_unavailable(detail: &str) -> Self {
        Self::new(
            "llm_unavailable",
            DegradedCategory::Quality,
            "llm",
            "L2",
            "L0",
            DegradedReason::Unavailable,
            &format!("generation failed: {detail}"),
            "extractive answer",
        )
    }

    pub fn citation_alignment_failed() -> Self {
        Self::new(
            "citation_alignment_failed",
            DegradedCategory::Quality,
            "llm",
            "L2",
            "L0",
            DegradedReason::Error,
            "generated text could not be aligned to supplied evidence",
            "extractive answer",
        )
    }

    pub fn acl_second_check_dropped(count: usize) -> Self {
        Self::new(
            "acl_second_check_dropped",
            DegradedCategory::Security,
            "governance",
            "L0",
            "L0",
            DegradedReason::Error,
            &format!("{count} stale candidate(s) failed the L4 ACL re-check"),
            "candidates discarded",
        )
    }

    pub fn keyword_exact_fallback() -> Self {
        Self::new(
            "keyword_exact_fallback",
            DegradedCategory::Quality,
            "keyword",
            "L2",
            "L1",
            DegradedReason::Unavailable,
            "full-text query produced no rows; substring fallback was used",
            "substring match",
        )
    }

    pub fn event_not_enabled() -> Self {
        Self::new(
            "event_not_enabled",
            DegradedCategory::Evidence,
            "event",
            "L1",
            "L0",
            DegradedReason::NotEnabled,
            "event-entity extraction is not configured",
            "keyword only",
        )
    }

    pub fn event_extraction_unavailable(detail: &str) -> Self {
        Self::new(
            "event_extraction_unavailable",
            DegradedCategory::Evidence,
            "event",
            "L1",
            "L0",
            DegradedReason::Unavailable,
            &format!("event extraction/index is unavailable: {detail}"),
            "keyword only",
        )
    }

    pub fn event_no_seed() -> Self {
        Self::new(
            "event_no_seed",
            DegradedCategory::Evidence,
            "event",
            "L1",
            "L0",
            DegradedReason::Unavailable,
            "no seed entity matched the query",
            "keyword only",
        )
    }

    pub fn event_channel_timeout(detail: &str) -> Self {
        Self::new(
            "event_channel_timeout",
            DegradedCategory::Performance,
            "event",
            "L1",
            "L0",
            DegradedReason::Timeout,
            &format!("event-entity channel exceeded its budget: {detail}"),
            "keyword only",
        )
    }

    pub fn malformed_hit() -> Self {
        Self::new(
            "malformed_hit",
            DegradedCategory::Evidence,
            "storage",
            "L0",
            "L0",
            DegradedReason::Error,
            "a stored candidate could not be decoded",
            "candidate discarded",
        )
    }
}

pub fn sort_degraded(items: &mut [Degraded]) {
    items.sort_by(|a, b| {
        a.category
            .priority()
            .cmp(&b.category.priority())
            .then_with(|| a.code.cmp(&b.code))
    });
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConfidenceBand {
    High,
    Medium,
    Low,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Confidence {
    pub score: f64,
    pub band: ConfidenceBand,
    pub basis: Vec<String>,
}

impl Confidence {
    pub fn low(reason: &str) -> Self {
        Self {
            score: 0.0,
            band: ConfidenceBand::Low,
            basis: vec![reason.to_string()],
        }
    }

    pub fn heuristic(hit_count: usize, top_margin: f64, degraded: &[Degraded], conflicts: usize) -> Self {
        let mut basis = Vec::new();
        let mut score = if hit_count == 0 { 0.0 } else { 0.45 };
        if hit_count > 0 {
            score += 0.10 * (hit_count.min(3) as f64);
            basis.push(format!("citations={hit_count}"));
            score += top_margin.clamp(0.0, 0.15);
            basis.push("rank_margin".to_string());
        }
        if conflicts > 0 {
            score -= 0.10;
            basis.push(format!("conflicts={conflicts}"));
        }
        let security = degraded
            .iter()
            .filter(|d| d.category == DegradedCategory::Security)
            .count();
        let evidence = degraded
            .iter()
            .filter(|d| d.category == DegradedCategory::Evidence)
            .count();
        let quality = degraded
            .iter()
            .filter(|d| d.category == DegradedCategory::Quality)
            .count();
        if degraded.is_empty() {
            basis.push("no_degradation".to_string());
        } else {
            score -= 0.15 * security as f64 + 0.08 * evidence as f64 + 0.04 * quality as f64;
            basis.push(format!(
                "degraded(security={security},evidence={evidence},quality={quality})"
            ));
        }
        score = score.clamp(0.0, 1.0);
        let band = if score >= 0.70 {
            ConfidenceBand::High
        } else if score >= 0.40 {
            ConfidenceBand::Medium
        } else {
            ConfidenceBand::Low
        };
        Self { score, band, basis }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AnswerMode {
    Extractive,
    Generative,
    Hybrid,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Citation {
    pub doc_id: String,
    pub chunk_id: String,
    pub path: String,
    pub line_start: i64,
    pub line_end: i64,
    pub quote: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_url: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConflictReason {
    Version,
    DualSource,
    Claim,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConflictResolution {
    Unresolved,
    Preferred,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Conflict {
    pub conflict_id: String,
    pub topic: String,
    pub candidates: Vec<String>,
    pub reason: ConflictReason,
    pub resolution: ConflictResolution,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub preferred_doc_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnswerUsage {
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub cached_tokens: i64,
    pub token_source: String,
    pub cost_micros: i64,
    pub currency: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Answer {
    pub answer: Option<String>,
    pub citations: Vec<Citation>,
    pub confidence: Confidence,
    pub conflicts: Vec<Conflict>,
    pub degraded: Vec<Degraded>,
    /// Minimum `valid_from` among documents actually cited. `null` means
    /// "not applicable", not "fresh".
    pub knowledge_as_of: Option<String>,
    pub mode: AnswerMode,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usage: Option<AnswerUsage>,
    pub cache_hit: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub conversation_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary_kind: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary_source: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn degraded_sorts_security_first() {
        let mut items = vec![
            Degraded::dense_not_enabled(),
            Degraded::acl_second_check_dropped(1),
            Degraded::decision_unavailable("timeout"),
        ];
        sort_degraded(&mut items);
        assert_eq!(items[0].category, DegradedCategory::Security);
        assert_eq!(items[1].category, DegradedCategory::Evidence);
        assert_eq!(items[2].category, DegradedCategory::Quality);
    }

    #[test]
    fn confidence_is_bounded() {
        let conf = Confidence::heuristic(
            1,
            0.0,
            &[
                Degraded::acl_second_check_dropped(2),
                Degraded::dense_not_enabled(),
            ],
            1,
        );
        assert!(conf.score >= 0.0 && conf.score <= 1.0);
        assert_eq!(conf.band, ConfidenceBand::Low);
    }
}
