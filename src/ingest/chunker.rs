// SPDX-License-Identifier: AGPL-3.0-only
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChunkDraft {
    pub text: String,
    pub line_start: i64,
    pub line_end: i64,
}

pub const DEFAULT_CHUNK_CHARS: usize = 1000;

/// Deterministic line-oriented chunker. Citation line ranges refer to the
/// source text passed in. Long individual lines are split into character
/// windows that keep the same source line number.
pub fn chunk_text(text: &str, max_chars: usize) -> Vec<ChunkDraft> {
    let max_chars = max_chars.max(1);
    let lines: Vec<&str> = text.lines().collect();
    let mut drafts = Vec::new();
    let mut current = String::new();
    let mut current_start = 1i64;
    let mut current_end = 1i64;

    for (index, line) in lines.iter().enumerate() {
        let line_no = (index + 1) as i64;
        let line_chars = line.chars().count();

        if line_chars > max_chars {
            if !current.trim().is_empty() {
                drafts.push(ChunkDraft {
                    text: current.trim_end().to_string(),
                    line_start: current_start,
                    line_end: current_end,
                });
                current.clear();
            }
            let chars: Vec<char> = line.chars().collect();
            for window in chars.chunks(max_chars) {
                let segment: String = window.iter().collect();
                if segment.trim().is_empty() {
                    continue;
                }
                drafts.push(ChunkDraft {
                    text: segment.trim_end().to_string(),
                    line_start: line_no,
                    line_end: line_no,
                });
            }
            current_start = line_no + 1;
            current_end = line_no + 1;
            continue;
        }

        let projected = if current.is_empty() {
            line_chars
        } else {
            current.chars().count() + 1 + line_chars
        };
        if !current.is_empty() && projected > max_chars {
            if !current.trim().is_empty() {
                drafts.push(ChunkDraft {
                    text: current.trim_end().to_string(),
                    line_start: current_start,
                    line_end: current_end,
                });
            }
            current.clear();
        }
        if current.is_empty() {
            current_start = line_no;
        }
        current.push_str(line);
        current.push('\n');
        current_end = line_no;
    }

    if !current.trim().is_empty() {
        drafts.push(ChunkDraft {
            text: current.trim_end().to_string(),
            line_start: current_start,
            line_end: current_end,
        });
    }
    drafts
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chunks_have_exact_line_ranges() {
        let text = "a\nb\nc\nd\ne\n";
        let chunks = chunk_text(text, 3);
        assert!(!chunks.is_empty());
        for chunk in &chunks {
            assert!(chunk.line_start >= 1);
            assert!(chunk.line_end >= chunk.line_start);
        }
        assert_eq!(chunks.first().unwrap().line_start, 1);
        assert_eq!(chunks.last().unwrap().line_end, 5);
    }

    #[test]
    fn long_line_is_split_but_keeps_line_number() {
        let text = format!("{}\nnext", "x".repeat(25));
        let chunks = chunk_text(&text, 10);
        assert_eq!(chunks.iter().filter(|c| c.line_start == 1).count(), 3);
        assert!(chunks.iter().any(|c| c.line_start == 2 && c.line_end == 2));
    }

    #[test]
    fn empty_text_has_no_chunks() {
        assert!(chunk_text(" \n\n ", 10).is_empty());
    }
}
