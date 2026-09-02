//! Lossless context compression.
//!
//! The compressor reduces the token cost of an item's active representation
//! without ever destroying source-of-truth information: the original content
//! is preserved (offloaded or in the item record), and compression only
//! replaces the *active* view. The current implementation is deterministic
//! and local: exact-duplicate line removal, structural extraction, and
//! bounded truncation with an explicit truncation marker. No external model
//! is required (mirroring ContextPilot's fallback path, but always on).

use serde::{Deserialize, Serialize};

/// Options controlling a compression pass.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct CompressOptions {
    /// Target maximum tokens for the compressed output.
    pub max_tokens: usize,
    /// Whether to remove exact duplicate lines first.
    pub dedup_lines: bool,
    /// Whether to attempt structural extraction (headings, list items, code
    /// block boundaries) for markdown-ish content.
    pub structural: bool,
    /// Whether bounded truncation is allowed as a last resort.
    pub truncate: bool,
}

impl Default for CompressOptions {
    fn default() -> Self {
        Self {
            max_tokens: 512,
            dedup_lines: true,
            structural: true,
            truncate: true,
        }
    }
}

/// The result of a compression pass.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CompressedContent {
    /// The compressed active representation.
    pub content: String,
    /// Token count of the compressed content.
    pub token_count: usize,
    /// Tokens before compression.
    pub original_tokens: usize,
    /// Tokens saved (original minus compressed; never negative).
    pub tokens_saved: usize,
    /// The applied strategies, in order.
    pub strategies: Vec<String>,
}

/// The compressor abstraction.
pub struct ContextCompressor {
    counter: crate::context::tokens::ApproxTokenCounter,
}

impl ContextCompressor {
    /// Creates a compressor using the default approximate token counter.
    pub fn new() -> Self {
        Self {
            counter: crate::context::tokens::ApproxTokenCounter,
        }
    }

    /// Compresses `content` under `options`, never failing: on any problem
    /// the original content is returned unchanged (fail-safe contract).
    pub fn compress(&self, content: &str, options: &CompressOptions) -> CompressedContent {
        use crate::context::tokens::TokenCounter;

        let original_tokens = self.counter.count(content);
        let mut strategies: Vec<String> = Vec::new();
        let mut current = content.to_string();

        if options.dedup_lines {
            let deduped = dedup_lines(&current);
            if deduped != current {
                current = deduped;
                strategies.push("dedup_lines".to_string());
            }
        }

        if options.structural {
            let extracted = structural_extract(&current);
            if extracted != current {
                current = extracted;
                strategies.push("structural_extract".to_string());
            }
        }

        if options.truncate && self.counter.count(&current) > options.max_tokens {
            let truncated = truncate_to_tokens(&current, options.max_tokens);
            if truncated != current {
                current = truncated;
                strategies.push("truncate".to_string());
            }
        }

        let token_count = self.counter.count(&current);
        CompressedContent {
            token_count,
            original_tokens,
            // Never claim negative savings.
            tokens_saved: original_tokens.saturating_sub(token_count),
            content: current,
            strategies,
        }
    }

    /// Estimates the compressed token count without producing the content.
    pub fn estimate(&self, content: &str, options: &CompressOptions) -> usize {
        self.compress(content, options).token_count
    }
}

impl Default for ContextCompressor {
    fn default() -> Self {
        Self::new()
    }
}

/// Removes exact duplicate lines, keeping first occurrences in order.
fn dedup_lines(content: &str) -> String {
    let mut seen = std::collections::HashSet::new();
    let mut out: Vec<&str> = Vec::new();
    for line in content.lines() {
        // Ignore pure-whitespace differences when deduplicating.
        let key = line.trim_end();
        if seen.insert(key) {
            out.push(line);
        }
    }
    if content.ends_with('\n') {
        out.push("");
    }
    out.join("\n")
}

/// Keeps structurally significant markdown lines: headings, list items,
/// fenced-code boundaries, and the first line; drops prose walls.
fn structural_extract(content: &str) -> String {
    let lines: Vec<&str> = content.lines().collect();
    if lines.len() < 6 {
        // Too short to be worth restructuring.
        return content.to_string();
    }
    let mut out: Vec<&str> = Vec::new();
    let mut in_fence = false;
    for (index, line) in lines.iter().enumerate() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("```") {
            in_fence = !in_fence;
            out.push(line);
            continue;
        }
        if in_fence {
            out.push(line);
            continue;
        }
        let structural = trimmed.starts_with('#')
            || trimmed.starts_with("- ")
            || trimmed.starts_with("* ")
            || trimmed.starts_with(">")
            || index == 0;
        if structural {
            out.push(line);
        }
    }
    out.join("\n")
}

/// Truncates to at most `max_tokens` words, appending an explicit marker.
///
/// The marker makes truncation visible and auditable rather than silent.
fn truncate_to_tokens(content: &str, max_tokens: usize) -> String {
    use crate::context::tokens::TokenCounter;
    let counter = crate::context::tokens::ApproxTokenCounter;
    if counter.count(content) <= max_tokens {
        return content.to_string();
    }
    // Word-level truncation lands close to the target without splitting
    // words; the approximate counter treats each word as one token.
    let mut kept: Vec<&str> = Vec::new();
    let mut tokens = 0usize;
    for word in content.split_whitespace() {
        // Reserve one token for the truncation marker itself.
        if tokens + 1 >= max_tokens {
            break;
        }
        kept.push(word);
        tokens += 1;
    }
    format!(
        "{}\n[truncated: {} of {} estimated tokens retained]",
        kept.join(" "),
        tokens,
        counter.count(content)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn options(max_tokens: usize) -> CompressOptions {
        CompressOptions {
            max_tokens,
            ..CompressOptions::default()
        }
    }

    #[test]
    fn duplicate_lines_are_removed() {
        let content = "line one\nline two\nline one\nline two\nline three\n";
        let compressor = ContextCompressor::new();
        let result = compressor.compress(content, &options(1000));
        assert!(result.strategies.contains(&"dedup_lines".to_string()));
        assert!(!result.content.contains("line one\nline one"));
        // Three unique lines survive: "line one", "line two", "line three".
        assert_eq!(result.token_count, 6);
        assert!(result.tokens_saved > 0);
    }

    #[test]
    fn short_content_is_untouched() {
        let content = "short content";
        let compressor = ContextCompressor::new();
        let result = compressor.compress(content, &options(1000));
        assert_eq!(result.content, content);
        assert!(result.strategies.is_empty());
        assert_eq!(result.tokens_saved, 0);
    }

    #[test]
    fn truncation_marks_explicitly_and_respects_budget() {
        let content = "word ".repeat(1000);
        let compressor = ContextCompressor::new();
        let result = compressor.compress(&content, &options(50));
        assert!(result.content.contains("[truncated:"));
        assert!(result.token_count <= 50 + 20); // marker tokens included
        assert!(result.tokens_saved > 0);
    }

    #[test]
    fn structural_extraction_keeps_headings_and_lists() {
        let mut content = String::from("# Title\n\n- item one\n- item two\n");
        for i in 0..20 {
            content.push_str(&format!(
                "prose line number {i} that is fairly long and wordy\n"
            ));
        }
        content.push_str("## Section\n");
        let compressor = ContextCompressor::new();
        let result = compressor.compress(&content, &options(1000));
        assert!(result.content.contains("# Title"));
        assert!(result.content.contains("- item one"));
        assert!(result.content.contains("## Section"));
        // Prose is dropped by structural extraction.
        assert!(!result.content.contains("prose line number 5"));
    }

    #[test]
    fn fences_are_preserved_intact() {
        let content = "# Title\n```rust\nfn main() {}\nfn main() {}\n```\n";
        let compressor = ContextCompressor::new();
        let result = compressor.compress(content, &options(1000));
        assert!(result.content.contains("fn main() {}"));
    }

    #[test]
    fn compression_never_fails() {
        let compressor = ContextCompressor::new();
        // Empty, unicode, and pathological inputs all return something.
        assert!(compressor.compress("", &options(10)).token_count == 0);
        let weird = "\u{1F600}".repeat(500);
        let result = compressor.compress(&weird, &options(10));
        assert!(!result.content.is_empty() || result.token_count == 0);
    }

    #[test]
    fn estimate_matches_compress() {
        let compressor = ContextCompressor::new();
        let content = "a b c d e f\n".repeat(20);
        assert_eq!(
            compressor.estimate(&content, &options(10)),
            compressor.compress(&content, &options(10)).token_count
        );
    }

    #[test]
    fn truncation_without_truncate_option_is_noop() {
        // Distinct words so dedup_lines finds nothing to remove.
        let content: String = (0..500)
            .map(|i| format!("word{} ", i))
            .collect::<Vec<_>>()
            .join("");
        let opts = CompressOptions {
            max_tokens: 10,
            truncate: false,
            ..CompressOptions::default()
        };
        let result = ContextCompressor::new().compress(&content, &opts);
        // Nothing removed and nothing appended: a true no-op.
        assert_eq!(result.content, content);
        assert!(result.strategies.is_empty());
    }
}
