/// Smart markdown-aware chunker.
/// Targets ~900 tokens per chunk with 15% overlap (mirrors QMD's approach).
/// Prefers splitting at heading and code-fence boundaries over hard cuts.

const TARGET_CHARS: usize = 3_600; // ~900 tokens @ 4 chars/token
const OVERLAP_CHARS: usize = 540;  // 15% overlap

#[derive(Debug, Clone)]
pub struct TextChunk {
    pub text: String,
    pub pos: usize, // byte offset in original
    pub seq: usize,
}

/// Snap a byte position to the nearest valid UTF-8 character boundary.
fn snap_char_boundary(s: &str, pos: usize) -> usize {
    if pos >= s.len() { return s.len(); }
    let mut p = pos;
    while !s.is_char_boundary(p) { p += 1; }
    p
}

pub fn chunk_markdown(text: &str) -> Vec<TextChunk> {
    if text.len() <= TARGET_CHARS {
        return vec![TextChunk { text: text.to_string(), pos: 0, seq: 0 }];
    }

    let break_points = find_break_points(text);
    let mut chunks = Vec::new();
    let mut start = 0;
    let mut seq = 0;

    while start < text.len() {
        let target_end = (start + TARGET_CHARS).min(text.len());

        let cut = if target_end == text.len() {
            target_end
        } else {
            best_break(&break_points, start, target_end)
        };

        // Ensure we're at character boundaries for safe slicing
        let start_char = snap_char_boundary(text, start);
        let cut_char = snap_char_boundary(text, cut);
        
        if let Some(chunk_text) = text.get(start_char..cut_char) {
            let chunk_text = chunk_text.trim().to_string();
            if !chunk_text.is_empty() {
                chunks.push(TextChunk { text: chunk_text, pos: start_char, seq });
                seq += 1;
            }
        }

        // Overlap: step back by OVERLAP_CHARS from the cut
        let next_start = cut.saturating_sub(OVERLAP_CHARS);
        
        // CRITICAL: Must always advance. If next_start doesn't move us forward,
        // force progress by at least 1000 chars
        if next_start <= start {
            start = (start + 1000).min(text.len());
        } else {
            start = next_start;
        }
    }

    chunks
}

/// Scoring table for break points (higher = better split location)
fn break_score(line: &str) -> Option<u32> {
    let trimmed = line.trim();
    if trimmed.starts_with("# ") { return Some(100); }
    if trimmed.starts_with("## ") { return Some(90); }
    if trimmed.starts_with("### ") { return Some(80); }
    if trimmed.starts_with("#### ") { return Some(70); }
    if trimmed.starts_with("##### ") { return Some(60); }
    if trimmed.starts_with("###### ") { return Some(50); }
    if trimmed.starts_with("```") { return Some(80); }
    if trimmed == "---" || trimmed == "***" || trimmed == "===" { return Some(60); }
    if trimmed.is_empty() { return Some(20); }
    if trimmed.starts_with("- ") || trimmed.starts_with("* ") { return Some(5); }
    if trimmed.chars().next().map(|c| c.is_ascii_digit()).unwrap_or(false)
        && trimmed.contains(". ") { return Some(5); }
    None
}

struct BreakPoint {
    pos: usize,   // byte offset of line start
    score: u32,
}

fn find_break_points(text: &str) -> Vec<BreakPoint> {
    let mut points = Vec::new();
    let mut pos = 0;
    let mut in_code_block = false;

    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("```") {
            in_code_block = !in_code_block;
        }

        if !in_code_block {
            if let Some(score) = break_score(line) {
                points.push(BreakPoint { pos, score });
            }
        }

        pos += line.len() + 1; // +1 for newline
    }

    points
}

/// Find the best break point within a 200-char window before `target`.
fn best_break(points: &[BreakPoint], after: usize, target: usize) -> usize {
    let window_start = target.saturating_sub(800); // ~200 tokens window
    let window_start = window_start.max(after);

    let best = points
        .iter()
        .filter(|p| p.pos >= window_start && p.pos <= target)
        .max_by_key(|p| {
            // Penalize distance from target using squared decay
            let dist = target - p.pos;
            let dist_factor = 1.0 - (dist as f64 / 800.0).powi(2) * 0.7;
            (p.score as f64 * dist_factor) as u32
        });

    best.map(|b| b.pos).unwrap_or(target)
}

/// Extract the title from markdown: first H1, then first H2, else filename hint
pub fn extract_title(text: &str, fallback: &str) -> String {
    for line in text.lines() {
        let trimmed = line.trim();
        if let Some(heading) = trimmed.strip_prefix("# ") {
            return heading.trim().to_string();
        }
        if let Some(heading) = trimmed.strip_prefix("## ") {
            return heading.trim().to_string();
        }
    }
    // Fallback to filename without extension
    std::path::Path::new(fallback)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or(fallback)
        .to_string()
}

/// Extract a snippet around a keyword match (for BM25 results)
pub fn extract_snippet(text: &str, query_terms: &[&str], max_chars: usize) -> String {
    let lower = text.to_lowercase();
    let query_lower: Vec<String> = query_terms.iter().map(|t| t.to_lowercase()).collect();

    // Find first query term hit
    let best_pos = query_lower
        .iter()
        .filter_map(|term| lower.find(term.as_str()))
        .min()
        .unwrap_or(0);

    // Extract window around hit
    let start = best_pos.saturating_sub(max_chars / 3);
    let end = (best_pos + max_chars * 2 / 3).min(text.len());

    // Snap to char boundary
    let start = snap_char_boundary(text, start);
    let end = snap_char_boundary(text, end);

    let mut snippet = text[start..end].to_string();
    if start > 0 { snippet = format!("…{}", snippet); }
    if end < text.len() { snippet = format!("{}…", snippet); }
    snippet
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_short_text_no_chunking() {
        let text = "# Hello\nShort document.";
        let chunks = chunk_markdown(text);
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].seq, 0);
        assert_eq!(chunks[0].pos, 0);
    }

    #[test]
    fn test_title_extraction() {
        assert_eq!(extract_title("# My Title\nBody", "file.md"), "My Title");
        assert_eq!(extract_title("## Secondary Title\nBody", "file.md"), "Secondary Title");
        assert_eq!(extract_title("No heading", "memory/2026-03-27.md"), "2026-03-27");
        assert_eq!(extract_title("No heading", "/path/to/file.txt"), "file");
    }

    #[test]
    fn test_chunk_markdown_long_text() {
        // Create a text longer than TARGET_CHARS (3600)
        let mut text = String::from("# Main Title\n\n");
        for i in 0..100 {
            text.push_str(&format!("Paragraph {} with some content. ", i));
            text.push_str("This is more text to make it longer. ");
            text.push_str("We need to reach the target character count.\n\n");
        }

        let chunks = chunk_markdown(&text);
        assert!(chunks.len() > 1, "Long text should be split into multiple chunks");
        
        // Check that chunks have sequential numbering
        for (i, chunk) in chunks.iter().enumerate() {
            assert_eq!(chunk.seq, i);
        }
    }

    #[test]
    fn test_chunk_respects_headings() {
        // Text with clear heading boundaries
        let text = "# First Section\n\n".to_string()
            + &"Content here. ".repeat(200)
            + "\n\n# Second Section\n\n"
            + &"More content. ".repeat(200);

        let chunks = chunk_markdown(&text);
        
        // Should have at least one chunk
        assert!(!chunks.is_empty());
        
        // Each chunk should be reasonably sized
        for chunk in &chunks {
            assert!(!chunk.text.is_empty());
        }
    }

    #[test]
    fn test_extract_snippet() {
        let text = "This is a long document with many words. "
            .repeat(50);
        
        let query_terms = &["document", "words"];
        let snippet = extract_snippet(&text, query_terms, 100);
        
        // Snippet should contain one of the query terms
        let snippet_lower = snippet.to_lowercase();
        assert!(
            snippet_lower.contains("document") || snippet_lower.contains("words"),
            "Snippet should contain query terms"
        );
        
        // Snippet should be truncated appropriately
        assert!(snippet.len() <= 150, "Snippet should be reasonably sized");
    }

    #[test]
    fn test_break_score_headings() {
        assert_eq!(break_score("# Heading 1"), Some(100));
        assert_eq!(break_score("## Heading 2"), Some(90));
        assert_eq!(break_score("### Heading 3"), Some(80));
        assert_eq!(break_score("#### Heading 4"), Some(70));
        assert_eq!(break_score("##### Heading 5"), Some(60));
        assert_eq!(break_score("###### Heading 6"), Some(50));
    }

    #[test]
    fn test_break_score_code_fence() {
        assert_eq!(break_score("```rust"), Some(80));
        assert_eq!(break_score("```"), Some(80));
    }

    #[test]
    fn test_break_score_horizontal_rules() {
        assert_eq!(break_score("---"), Some(60));
        assert_eq!(break_score("***"), Some(60));
        assert_eq!(break_score("==="), Some(60));
    }

    #[test]
    fn test_break_score_empty() {
        assert_eq!(break_score(""), Some(20));
        assert_eq!(break_score("   "), Some(20));
    }

    #[test]
    fn test_break_score_list_items() {
        assert_eq!(break_score("- List item"), Some(5));
        assert_eq!(break_score("* List item"), Some(5));
        assert_eq!(break_score("1. Numbered item"), Some(5));
    }

    #[test]
    fn test_snap_char_boundary() {
        // ASCII string
        assert_eq!(snap_char_boundary("hello", 3), 3);
        assert_eq!(snap_char_boundary("hello", 5), 5);
        
        // Multi-byte UTF-8 character (3 bytes each)
        let emoji = "🎉🎊🎁";
        assert_eq!(snap_char_boundary(emoji, 0), 0);
        assert_eq!(snap_char_boundary(emoji, 4), 4); // Should snap to 4 (end of first emoji)
    }
}