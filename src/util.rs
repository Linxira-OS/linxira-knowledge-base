use chrono::{DateTime, SecondsFormat, Utc};
use rand::RngCore;
use sha2::{Digest, Sha256};
use std::net::IpAddr;

/// Canonical project links. Single source of truth for the CLI `--version`/
/// `--help`/error tails, `lkb config check` and the server-rendered pages.
/// Keep in sync with the `repository` field in Cargo.toml (pinned by a test).
pub const REPOSITORY_URL: &str = "https://github.com/Linxira-OS/linxira-knowledge-base";
pub const ISSUES_URL: &str = "https://github.com/Linxira-OS/linxira-knowledge-base/issues";
pub const SECURITY_URL: &str =
    "https://github.com/Linxira-OS/linxira-knowledge-base/security/advisories/new";

pub fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hex::encode(hasher.finalize())
}

pub fn sha256_hex_str(value: &str) -> String {
    sha256_hex(value.as_bytes())
}

pub fn random_hex(bytes: usize) -> String {
    let mut buf = vec![0u8; bytes];
    let mut rng = rand::rngs::OsRng;
    rng.fill_bytes(&mut buf);
    hex::encode(buf)
}

pub fn now_rfc3339() -> String {
    Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true)
}

pub fn normalize_rfc3339(value: &str) -> Result<String, String> {
    let parsed = DateTime::parse_from_rfc3339(value)
        .map_err(|err| format!("invalid RFC3339 timestamp {value:?}: {err}"))?;
    Ok(parsed
        .with_timezone(&Utc)
        .to_rfc3339_opts(SecondsFormat::Secs, true))
}

pub fn mask_database_url(url: &str) -> String {
    let mut output = if let Some(scheme_end) = url.find("://") {
        let rest = &url[scheme_end + 3..];
        if let Some(at) = rest.rfind('@') {
            if let Some(colon) = rest[..at].find(':') {
                format!(
                    "{}://{}:***@{}",
                    &url[..scheme_end],
                    &rest[..colon],
                    &rest[at + 1..]
                )
            } else {
                url.to_string()
            }
        } else {
            url.to_string()
        }
    } else {
        url.to_string()
    };

    if let Some(query_start) = output.find('?') {
        let prefix = output[..query_start].to_string();
        let query = output[query_start + 1..].to_string();
        let masked_query = query
            .split('&')
            .map(|pair| match pair.split_once('=') {
                Some((key, _)) if is_sensitive_key(key) => format!("{key}=***"),
                _ => pair.to_string(),
            })
            .collect::<Vec<String>>()
            .join("&");
        output = format!("{prefix}?{masked_query}");
    }
    output
}

fn is_sensitive_key(key: &str) -> bool {
    let key = key.to_ascii_lowercase();
    ["pass", "token", "secret", "key", "credential"]
        .iter()
        .any(|needle| key.contains(needle))
}

/// True for loopback names and addresses. Used by the startup safety check
/// (`auth = none` may only bind to loopback).
pub fn is_loopback_host(host: &str) -> bool {
    let host = host.trim().trim_start_matches('[').trim_end_matches(']');
    if host.eq_ignore_ascii_case("localhost") {
        return true;
    }
    host.parse::<IpAddr>()
        .map(|addr| addr.is_loopback())
        .unwrap_or(false)
}

/// Extract the host part of a `host:port` or `[ipv6]:port` string.
pub fn split_bind_host(bind: &str) -> &str {
    let bind = bind.trim();
    if let Some(rest) = bind.strip_prefix('[') {
        return rest.split(']').next().unwrap_or(rest);
    }
    match bind.rsplit_once(':') {
        Some((host, _port)) => host,
        None => bind,
    }
}

pub fn split_host_hostport(host_header: &str) -> &str {
    let host_header = host_header.trim();
    if let Some(rest) = host_header.strip_prefix('[') {
        return rest.split(']').next().unwrap_or(rest);
    }
    match host_header.rsplit_once(':') {
        Some((host, _port)) => host,
        None => host_header,
    }
}

pub fn constant_time_eq(left: &[u8], right: &[u8]) -> bool {
    if left.len() != right.len() {
        return false;
    }
    let mut diff = 0u8;
    for (a, b) in left.iter().zip(right.iter()) {
        diff |= a ^ b;
    }
    diff == 0
}

pub fn html_escape(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for ch in value.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(ch),
        }
    }
    out
}

pub fn url_encode_path_segment(value: &str) -> String {
    let mut out = String::new();
    for &byte in value.as_bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char)
            }
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

pub fn truncate_chars(value: &str, max_chars: usize) -> String {
    if max_chars == 0 {
        return String::new();
    }
    if value.chars().count() <= max_chars {
        return value.to_string();
    }
    let mut out: String = value.chars().take(max_chars.saturating_sub(1)).collect();
    out.push('…');
    out
}

/// Build a safe FTS5 MATCH expression. Latin/digit tokens are quoted whole;
/// CJK runs longer than three characters are expanded into overlapping
/// trigrams so natural-language Chinese questions can match without a
/// segmentation dictionary. Terms are OR-ed so one missing term does not
/// discard the query.
pub fn fts5_query(query: &str) -> String {
    query_terms(query)
        .into_iter()
        .map(|term| format!("\"{}\"", term.replace('"', "\"\"")))
        .collect::<Vec<String>>()
        .join(" OR ")
}

fn is_cjk(ch: char) -> bool {
    matches!(
        ch as u32,
        0x3400..=0x4DBF
            | 0x4E00..=0x9FFF
            | 0xF900..=0xFAFF
            | 0x3040..=0x30FF
            | 0xAC00..=0xD7AF
    )
}

/// Split a query into substring terms usable by LIKE/instr fallbacks. CJK
/// runs become trigrams; other runs stay whole. This is deliberately simple
/// and deterministic, and it is only used for keyword recall (never for ACL or
/// identity decisions).
pub fn query_terms(query: &str) -> Vec<String> {
    let mut segments = Vec::new();
    let mut current = String::new();
    for ch in query.chars() {
        let keep =
            ch.is_alphanumeric() || matches!(ch, '_' | '-' | '.' | '/' | '@' | '+') || is_cjk(ch);
        if keep {
            current.push(ch);
        } else if !current.is_empty() {
            segments.push(std::mem::take(&mut current));
        }
    }
    if !current.is_empty() {
        segments.push(current);
    }

    let mut terms = Vec::new();
    for segment in segments {
        if segment.chars().any(is_cjk) {
            let chars: Vec<char> = segment.chars().collect();
            if chars.len() <= 3 {
                terms.push(segment);
            } else {
                for window in chars.windows(3) {
                    terms.push(window.iter().collect::<String>());
                }
            }
        } else {
            terms.push(segment);
        }
    }

    let mut seen = std::collections::HashSet::new();
    terms.retain(|term| !term.is_empty() && seen.insert(term.clone()));
    terms.truncate(48);
    terms
}

pub fn strip_code_fence(value: &str) -> &str {
    let trimmed = value.trim();
    if !trimmed.starts_with("```") {
        return trimmed;
    }
    let without_open = match trimmed.find('\n') {
        Some(idx) => &trimmed[idx + 1..],
        None => return "",
    };
    match without_open.rfind("```") {
        Some(idx) => without_open[..idx].trim(),
        None => without_open.trim(),
    }
}

pub fn parse_bool(value: &str) -> Result<bool, String> {
    match value.trim().to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "on" => Ok(true),
        "0" | "false" | "no" | "off" => Ok(false),
        other => Err(format!("expected boolean, got {other:?}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn masks_password_in_database_url() {
        let masked = mask_database_url("postgres://lkb:secret@db.example:5432/lkb");
        assert_eq!(masked, "postgres://lkb:***@db.example:5432/lkb");
        assert!(!masked.contains("secret"));
        assert_eq!(
            mask_database_url("https://api.example/v1?api_key=abc&model=x"),
            "https://api.example/v1?api_key=***&model=x"
        );
    }

    #[test]
    fn loopback_detection() {
        assert!(is_loopback_host("127.0.0.1"));
        assert!(is_loopback_host("[::1]"));
        assert!(is_loopback_host("localhost"));
        assert!(!is_loopback_host("0.0.0.0"));
        assert!(!is_loopback_host("192.168.1.10"));
    }

    #[test]
    fn repository_links_pin_the_canonical_repository() {
        assert_eq!(REPOSITORY_URL, env!("CARGO_PKG_REPOSITORY"));
        assert_eq!(ISSUES_URL, format!("{REPOSITORY_URL}/issues"));
        assert_eq!(
            SECURITY_URL,
            format!("{REPOSITORY_URL}/security/advisories/new")
        );
    }

    #[test]
    fn fts_query_is_quoted() {
        assert_eq!(
            fts5_query("hello \"world\""),
            "\"hello\" OR \"world\"".to_string()
        );
        assert_eq!(fts5_query("   "), String::new());
    }

    #[test]
    fn chinese_terms_are_trigrammed() {
        assert_eq!(
            query_terms("治理字段"),
            vec!["治理字".to_string(), "理字段".to_string()]
        );
        assert!(query_terms("doc_id 如何派生").contains(&"doc_id".to_string()));
    }

    #[test]
    fn path_segment_encoding_is_stable() {
        assert_eq!(url_encode_path_segment("a b/中"), "a%20b%2F%E4%B8%AD");
    }
}
