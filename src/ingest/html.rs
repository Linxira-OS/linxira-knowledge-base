/// Conservative HTML-to-text extraction. It is deliberately not a full HTML
/// parser: it removes script/style/comment content, converts block boundaries
/// to newlines, decodes common entities, and keeps text order. Chunks and
/// citations therefore refer to extracted text lines, not original HTML lines.
pub fn extract_text(html: &str) -> String {
    let chars: Vec<char> = html.chars().collect();
    let mut out = String::new();
    let mut index = 0usize;

    while index < chars.len() {
        if chars[index] == '<' {
            if starts_with(&chars, index, &['<', '!', '-', '-']) {
                if let Some(end) = find_sequence(&chars, index + 4, &['-', '-', '>']) {
                    index = end + 3;
                    continue;
                }
            }

            if let Some(end) = find_char(&chars, index + 1, '>') {
                let raw: String = chars[index + 1..end].iter().collect();
                let trimmed = raw.trim();
                let closing = trimmed.starts_with('/');
                let name = trimmed
                    .trim_start_matches('/')
                    .split_whitespace()
                    .next()
                    .unwrap_or("")
                    .to_ascii_lowercase();
                if (name == "script" || name == "style") && !closing {
                    index = skip_element(&chars, end + 1, &name);
                    out.push('\n');
                    continue;
                }
                if is_block_tag(&name) {
                    out.push('\n');
                }
                index = end + 1;
                continue;
            }
        }
        out.push(chars[index]);
        index += 1;
    }

    normalize_text(&decode_entities(&out))
}

pub fn extract_title(html: &str) -> Option<String> {
    let chars: Vec<char> = html.chars().collect();
    let lower: Vec<char> = html.to_ascii_lowercase().chars().collect();
    let start = find_sequence(&lower, 0, &['<', 't', 'i', 't', 'l', 'e'])?;
    let open_end = find_char(&chars, start + 6, '>')?;
    let close = find_sequence(&lower, open_end + 1, &['<', '/', 't', 'i', 't', 'l', 'e'])?;
    let text: String = chars[open_end + 1..close].iter().collect();
    let cleaned = normalize_text(&decode_entities(&text));
    if cleaned.is_empty() {
        None
    } else {
        Some(cleaned)
    }
}

fn is_block_tag(name: &str) -> bool {
    matches!(
        name,
        "p" | "div"
            | "br"
            | "li"
            | "tr"
            | "td"
            | "th"
            | "h1"
            | "h2"
            | "h3"
            | "h4"
            | "h5"
            | "h6"
            | "section"
            | "article"
            | "header"
            | "footer"
            | "blockquote"
            | "pre"
            | "table"
            | "ul"
            | "ol"
            | "hr"
    )
}

fn starts_with(chars: &[char], index: usize, target: &[char]) -> bool {
    index + target.len() <= chars.len()
        && chars[index..index + target.len()] == *target
}

fn find_char(chars: &[char], start: usize, target: char) -> Option<usize> {
    (start..chars.len()).find(|index| chars[*index] == target)
}

fn find_sequence(chars: &[char], start: usize, target: &[char]) -> Option<usize> {
    if target.is_empty() || start >= chars.len() {
        return None;
    }
    (start..=chars.len().saturating_sub(target.len())).find(|index| {
        chars[*index..*index + target.len()] == *target
    })
}

fn find_sequence_ci(chars: &[char], start: usize, target: &str) -> Option<usize> {
    let target: Vec<char> = target.chars().flat_map(char::to_lowercase).collect();
    if target.is_empty() || start >= chars.len() {
        return None;
    }
    (start..=chars.len().saturating_sub(target.len())).find(|index| {
        chars[*index..*index + target.len()]
            .iter()
            .flat_map(|c| (*c).to_lowercase())
            .eq(target.iter().copied())
    })
}

fn skip_element(chars: &[char], start: usize, name: &str) -> usize {
    let closing = format!("</{name}");
    match find_sequence_ci(chars, start, &closing) {
        Some(index) => match find_char(chars, index, '>') {
            Some(end) => end + 1,
            None => chars.len(),
        },
        None => chars.len(),
    }
}

fn decode_entities(input: &str) -> String {
    let chars: Vec<char> = input.chars().collect();
    let mut out = String::new();
    let mut index = 0usize;
    while index < chars.len() {
        if chars[index] == '&' {
            if let Some(end) = find_char(&chars, index + 1, ';') {
                if end - index <= 12 {
                    let entity: String = chars[index + 1..end].iter().collect();
                    if let Some(decoded) = decode_entity(&entity) {
                        out.push(decoded);
                        index = end + 1;
                        continue;
                    }
                }
            }
        }
        out.push(chars[index]);
        index += 1;
    }
    out
}

fn decode_entity(entity: &str) -> Option<char> {
    match entity.to_ascii_lowercase().as_str() {
        "amp" => Some('&'),
        "lt" => Some('<'),
        "gt" => Some('>'),
        "quot" => Some('"'),
        "apos" | "#39" => Some('\''),
        "nbsp" => Some(' '),
        "ndash" => Some('–'),
        "mdash" => Some('—'),
        "hellip" => Some('…'),
        other => {
            if let Some(hex) = other.strip_prefix("#x") {
                u32::from_str_radix(hex, 16).ok().and_then(char::from_u32)
            } else if let Some(decimal) = other.strip_prefix('#') {
                decimal.parse::<u32>().ok().and_then(char::from_u32)
            } else {
                None
            }
        }
    }
}

fn normalize_text(input: &str) -> String {
    let normalized = input.replace("\r\n", "\n").replace('\r', "\n");
    let mut lines = Vec::new();
    let mut blank = 0usize;
    for line in normalized.lines() {
        let line = line.trim_end();
        if line.trim().is_empty() {
            blank += 1;
            if blank <= 1 && !lines.is_empty() {
                lines.push(String::new());
            }
        } else {
            blank = 0;
            lines.push(line.to_string());
        }
    }
    while lines.last().map(|line| line.is_empty()).unwrap_or(false) {
        lines.pop();
    }
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn removes_scripts_and_keeps_blocks() {
        let html = "<html><head><title>标题</title><style>x{}</style></head>\
                    <body><h1>Hello</h1><script>alert(1)</script><p>A &amp; B</p></body></html>";
        let text = extract_text(html);
        assert!(text.contains("Hello"));
        assert!(text.contains("A & B"));
        assert!(!text.contains("alert"));
        assert_eq!(extract_title(html).as_deref(), Some("标题"));
    }

    #[test]
    fn decodes_numeric_entities() {
        assert_eq!(decode_entities("&#20013;&#x6587;"), "中文");
    }
}
