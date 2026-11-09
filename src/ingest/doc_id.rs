use crate::util::sha256_hex;
use sha2::{Digest, Sha256};

pub const CANONICALIZATION_VERSION: &str = "p1";
pub const CHUNKER_VERSION: &str = "c1";
pub const EXTRACTOR_VERSION: &str = "e1";

/// Canonical relative path used for `doc_id`.
///
/// Frozen rules for `p1`: forward slashes, no leading `./`, no `.` or empty
/// components, `..` rejected, ASCII lower-cased. Unicode NFC normalization is
/// not applied because the workspace intentionally has no Unicode
/// normalization dependency; this is recorded as a known boundary.
pub fn canonical_relative_path(path: &str) -> Result<String, String> {
    let replaced = path.trim().replace('\\', "/");
    if replaced.starts_with('/') {
        return Err(format!("path must be relative: {path:?}"));
    }
    if replaced.len() >= 2 {
        let bytes = replaced.as_bytes();
        if bytes[1] == b':' && bytes[0].is_ascii_alphabetic() {
            return Err(format!("path must be relative: {path:?}"));
        }
    }
    let mut components = Vec::new();
    for component in replaced.split('/') {
        if component.is_empty() || component == "." {
            continue;
        }
        if component == ".." {
            return Err(format!("path must not contain '..': {path:?}"));
        }
        components.push(component.to_ascii_lowercase());
    }
    if components.is_empty() {
        return Err(format!("path is empty after normalization: {path:?}"));
    }
    Ok(components.join("/"))
}

pub fn content_hash(bytes: &[u8]) -> String {
    sha256_hex(bytes)
}

/// The acceptance rule for this repository:
/// `doc_id = hex(sha256(canonical_relative_path + "\0" + content_hash))[..16 bytes]`
/// (32 lowercase hex characters). It is stable across rebuilds. A content
/// change produces a new `doc_id`, and the previous record points to it via
/// `superseded_by`.
pub fn derive_doc_id(canonical_path: &str, content_hash: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(canonical_path.as_bytes());
    hasher.update([0u8]);
    hasher.update(content_hash.as_bytes());
    hex::encode(&hasher.finalize()[..16])
}

pub fn derive_chunk_id(tenant_id: &str, doc_id: &str, seq: u32, content_hash: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(tenant_id.as_bytes());
    hasher.update([0u8]);
    hasher.update(doc_id.as_bytes());
    hasher.update([0u8]);
    hasher.update(seq.to_string().as_bytes());
    hasher.update([0u8]);
    hasher.update(CHUNKER_VERSION.as_bytes());
    hasher.update([0u8]);
    hasher.update(content_hash.as_bytes());
    format!("chk_{}", hex::encode(&hasher.finalize()[..16]))
}

pub fn derive_revision_id(
    tenant_id: &str,
    space_id: &str,
    slug: &str,
    lang: &str,
    content_hash: &str,
    version: u32,
) -> String {
    let mut hasher = Sha256::new();
    for part in [tenant_id, space_id, slug, lang, content_hash] {
        hasher.update(part.as_bytes());
        hasher.update([0u8]);
    }
    hasher.update(version.to_string().as_bytes());
    format!("rev_{}", hex::encode(&hasher.finalize()[..16]))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn path_normalization_is_frozen() {
        assert_eq!(
            canonical_relative_path("./Docs\\Guide.MD").unwrap(),
            "docs/guide.md"
        );
        assert!(canonical_relative_path("../escape.md").is_err());
        assert!(canonical_relative_path("C:\\absolute.md").is_err());
        assert!(canonical_relative_path("/absolute.md").is_err());
    }

    #[test]
    fn doc_id_is_stable_and_content_sensitive() {
        let path = "docs/guide.md";
        let hash_a = content_hash(b"alpha");
        let hash_b = content_hash(b"beta");
        let first = derive_doc_id(path, &hash_a);
        let second = derive_doc_id(path, &hash_a);
        let changed = derive_doc_id(path, &hash_b);
        assert_eq!(first, second);
        assert_ne!(first, changed);
        assert_eq!(first.len(), 32);
        assert!(first.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn chunk_ids_are_tenant_scoped() {
        let hash = content_hash(b"alpha");
        assert_ne!(
            derive_chunk_id("t1", "doc", 0, &hash),
            derive_chunk_id("t2", "doc", 0, &hash)
        );
    }
}
