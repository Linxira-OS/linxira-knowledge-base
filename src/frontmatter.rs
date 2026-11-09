//! Minimal frontmatter parser for seed and Markdown imports.
//!
//! Only a flat `key: value` block is supported. Values may be single- or
//! double-quoted. The parser deliberately does not accept nested YAML; the
//! seed contract only uses title/slug/lang/order/summary.

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Frontmatter {
    pub title: Option<String>,
    pub slug: Option<String>,
    pub lang: Option<String>,
    pub order: Option<i32>,
    pub summary: Option<String>,
    pub body: String,
}

pub fn parse(content: &str) -> Frontmatter {
    let content = content.strip_prefix('\u{feff}').unwrap_or(content);
    if !content.starts_with("---\n") && !content.starts_with("---\r\n") {
        return Frontmatter {
            body: content.to_string(),
            ..Frontmatter::default()
        };
    }

    let mut frontmatter = Frontmatter::default();
    let mut cursor = content.find('\n').map(|index| index + 1).unwrap_or(content.len());
    let mut body_start = None;
    while cursor < content.len() {
        let line_end = content[cursor..]
            .find('\n')
            .map(|offset| cursor + offset)
            .unwrap_or(content.len());
        let line = &content[cursor..line_end];
        if line.trim() == "---" {
            body_start = Some((line_end + 1).min(content.len()));
            break;
        }
        if let Some((key, value)) = line.split_once(':') {
            let key = key.trim().to_ascii_lowercase();
            let value = unquote(value.trim());
            if value.is_empty() {
                cursor = line_end + 1;
                continue;
            }
            match key.as_str() {
                "title" => frontmatter.title = Some(value),
                "slug" => frontmatter.slug = Some(value),
                "lang" => frontmatter.lang = Some(value.to_ascii_lowercase()),
                "order" => frontmatter.order = value.parse::<i32>().ok(),
                "summary" => frontmatter.summary = Some(value),
                _ => {}
            }
        }
        cursor = (line_end + 1).min(content.len());
    }

    match body_start {
        Some(start) => {
            frontmatter.body = content[start..]
                .trim_start_matches(|c| c == '\r' || c == '\n')
                .to_string()
        }
        None => frontmatter.body = content.to_string(),
    }
    frontmatter
}

fn unquote(value: &str) -> String {
    let bytes = value.as_bytes();
    if bytes.len() >= 2 {
        let first = bytes[0];
        let last = bytes[bytes.len() - 1];
        if (first == b'"' && last == b'"') || (first == b'\'' && last == b'\'') {
            return value[1..value.len() - 1]
                .replace("\\\"", "\"")
                .replace("\\'", "'");
        }
    }
    value.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_seed_frontmatter() {
        let parsed = parse(
            "---\ntitle: 产品概览\nslug: 01-overview\nlang: zh\norder: 1\nsummary: 一句话摘要。\n---\n\n# 正文\n",
        );
        assert_eq!(parsed.title.as_deref(), Some("产品概览"));
        assert_eq!(parsed.slug.as_deref(), Some("01-overview"));
        assert_eq!(parsed.lang.as_deref(), Some("zh"));
        assert_eq!(parsed.order, Some(1));
        assert_eq!(parsed.summary.as_deref(), Some("一句话摘要。"));
        assert_eq!(parsed.body, "# 正文\n");
    }

    #[test]
    fn plain_markdown_has_no_frontmatter() {
        let parsed = parse("# title\nbody");
        assert!(parsed.title.is_none());
        assert_eq!(parsed.body, "# title\nbody");
    }

    #[test]
    fn quoted_values_are_unquoted() {
        let parsed = parse("---\ntitle: \"A: B\"\nslug: x\nlang: en\norder: 2\nsummary: \"s: t\"\n---\nbody");
        assert_eq!(parsed.title.as_deref(), Some("A: B"));
        assert_eq!(parsed.summary.as_deref(), Some("s: t"));
    }
}
