//! Event-entity derived records for the optional SQL-RAG channel.
//!
//! Extraction is always optional. These helpers only derive stable IDs and
//! normalized dictionary keys; they do not decide visibility or ranking.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const EXTRACTOR_VERSION: &str = "event-v1";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExtractedEntity {
    pub name: String,
    #[serde(rename = "type")]
    pub entity_type: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExtractedEvent {
    pub event: String,
    #[serde(default)]
    pub entities: Vec<ExtractedEntity>,
}

pub fn normalize_entity_name(value: &str) -> String {
    let lower = value.trim().to_lowercase();
    let mut out = String::with_capacity(lower.len());
    let mut last_space = false;
    for ch in lower.chars() {
        if ch.is_whitespace() {
            if !out.is_empty() && !last_space {
                out.push(' ');
                last_space = true;
            }
        } else {
            out.push(ch);
            last_space = false;
        }
    }
    out.trim().to_string()
}

pub fn normalize_entity_type(value: &str) -> String {
    let normalized = value.trim().to_lowercase();
    if normalized.is_empty() {
        "unknown".to_string()
    } else {
        normalized
    }
}

pub fn derive_event_id(
    tenant_id: &str,
    space_id: &str,
    chunk_id: &str,
    seq: u32,
    extractor_version: &str,
    extraction_model: &str,
) -> String {
    let mut hasher = Sha256::new();
    for part in [
        tenant_id,
        space_id,
        chunk_id,
        extractor_version,
        extraction_model,
    ] {
        hasher.update(part.as_bytes());
        hasher.update([0u8]);
    }
    hasher.update(seq.to_string().as_bytes());
    format!("evt_{}", hex::encode(&hasher.finalize()[..16]))
}

pub fn derive_entity_id(
    tenant_id: &str,
    space_id: &str,
    normalized_name: &str,
    entity_type: &str,
) -> String {
    let mut hasher = Sha256::new();
    for part in [tenant_id, space_id, normalized_name, entity_type] {
        hasher.update(part.as_bytes());
        hasher.update([0u8]);
    }
    format!("ent_{}", hex::encode(&hasher.finalize()[..16]))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entity_name_normalization_is_stable() {
        assert_eq!(normalize_entity_name("  Doc_ID   "), "doc_id");
        assert_eq!(normalize_entity_name("Alpha\t Beta"), "alpha beta");
        assert_eq!(normalize_entity_type(" Concept "), "concept");
        assert_eq!(normalize_entity_type(""), "unknown");
    }

    #[test]
    fn ids_are_scoped_and_content_sensitive() {
        let a = derive_event_id("t", "s", "chk", 0, "event-v1", "m1");
        let b = derive_event_id("t", "s", "chk", 0, "event-v1", "m1");
        let c = derive_event_id("t", "s", "chk", 0, "event-v1", "m2");
        assert_eq!(a, b);
        assert_ne!(a, c);
        assert!(a.starts_with("evt_"));
        assert_eq!(a.len(), 36);
        assert_ne!(
            derive_entity_id("t", "s", "alpha", "concept"),
            derive_entity_id("t2", "s", "alpha", "concept")
        );
    }
}
