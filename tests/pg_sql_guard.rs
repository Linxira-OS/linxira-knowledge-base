// SPDX-License-Identifier: MIT OR Apache-2.0
//! Static guard for hand-written PostgreSQL SQL: every direct `sqlx::query`
//! literal with immediately chained `.bind(...)` calls must have placeholder
//! numbers 1..=N where N equals the bind count. Dynamic `format!` queries are
//! skipped (they are few and audited manually); this guard targets the common
//! "added a column but forgot a bind" regression without needing PostgreSQL.

use std::collections::BTreeSet;

fn audit_literal_queries(source: &str) -> (usize, Vec<String>) {
    let bytes = source.as_bytes();
    let mut index = 0usize;
    let mut checked = 0usize;
    let mut issues = Vec::new();
    while let Some(offset) = source[index..].find("sqlx::query") {
        let start = index + offset;
        let mut quote = start + "sqlx::query".len();
        while quote < bytes.len() && bytes[quote] != b'"' {
            quote += 1;
        }
        if quote >= bytes.len() {
            break;
        }
        // Parse the Rust string literal body.
        let mut body = String::new();
        let mut i = quote + 1;
        let mut escaped = false;
        while i < bytes.len() {
            let ch = bytes[i] as char;
            if escaped {
                body.push(ch);
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == '"' {
                break;
            } else {
                body.push(ch);
            }
            i += 1;
        }
        let end_quote = i;
        // Find the closing ')' of the query(...) call.
        let mut close = end_quote + 1;
        while close < bytes.len() && bytes[close] != b')' {
            close += 1;
        }
        close += 1;
        // Count immediately chained `.bind(...)` calls.
        let mut cursor = close;
        let mut binds = 0usize;
        loop {
            while cursor < bytes.len() && (bytes[cursor] as char).is_whitespace() {
                cursor += 1;
            }
            let rest = &source[cursor..];
            if rest.starts_with(".bind") {
                binds += 1;
                cursor += ".bind".len();
                while cursor < bytes.len() && (bytes[cursor] as char).is_whitespace() {
                    cursor += 1;
                }
                if cursor < bytes.len() && bytes[cursor] == b'(' {
                    let mut depth = 0i32;
                    while cursor < bytes.len() {
                        match bytes[cursor] {
                            b'(' => depth += 1,
                            b')' => {
                                depth -= 1;
                                if depth == 0 {
                                    cursor += 1;
                                    break;
                                }
                            }
                            _ => {}
                        }
                        cursor += 1;
                    }
                }
                continue;
            }
            break;
        }
        if binds == 0 {
            // Query assigned to a variable or built dynamically: conservative skip.
            index = close.max(1);
            continue;
        }
        checked += 1;
        let mut numbers = BTreeSet::new();
        let mut j = 0usize;
        let chars: Vec<char> = body.chars().collect();
        while j < chars.len() {
            if chars[j] == '$' {
                let mut number = String::new();
                let mut k = j + 1;
                while k < chars.len() && chars[k].is_ascii_digit() {
                    number.push(chars[k]);
                    k += 1;
                }
                if let Ok(value) = number.parse::<usize>() {
                    numbers.insert(value);
                    j = k;
                    continue;
                }
            }
            j += 1;
        }
        let max = numbers.iter().next_back().copied().unwrap_or(0);
        let expected: BTreeSet<usize> = (1..=max).collect();
        if max != binds || numbers != expected {
            issues.push(format!(
                "near offset {start}: max placeholder ${max}, binds {binds}, numbers {:?}, sql {:?}",
                numbers,
                body.chars().take(80).collect::<String>()
            ));
        }
        index = close.max(1);
    }
    (checked, issues)
}

#[test]
fn postgres_sql_placeholder_bind_count_guard() {
    let source = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/src/store/postgres.rs"
    ))
    .expect("read postgres store");
    let (checked, issues) = audit_literal_queries(&source);
    assert!(
        checked >= 100,
        "expected to audit at least 100 literal queries, checked {checked}"
    );
    assert!(
        issues.is_empty(),
        "SQL placeholder/bind mismatches:\n{}",
        issues.join("\n")
    );
    assert!(
        source.contains("rewritten_query, at) VALUES ($1,"),
        "usage_events insert must keep rewritten_query in the PostgreSQL column list"
    );
}

#[test]
fn guard_detects_placeholder_bind_mismatch() {
    let bad = r#"sqlx::query("INSERT INTO t (a,b,c) VALUES ($1,$2,$3)").bind(a).bind(b).execute(&pool).await?;"#;
    let (checked, issues) = audit_literal_queries(bad);
    assert_eq!(checked, 1);
    assert!(
        !issues.is_empty(),
        "guard must flag 3 placeholders with 2 binds"
    );
    let good = r#"sqlx::query("SELECT * FROM t WHERE a=$1 AND b=$2").bind(a).bind(b).fetch_all(&pool).await?;"#;
    let (checked, issues) = audit_literal_queries(good);
    assert_eq!(checked, 1);
    assert!(issues.is_empty());
}
