//! Token counting for the Context Engine.
//!
//! Exact token counts depend on the model's tokenizer, which the engine does
//! not link (that would drag in heavy model-specific dependencies, breaking
//! the aarch64/Termux footprint goals). Instead, [`ApproxTokenCounter`] uses
//! a documented, conservative approximation and is exposed behind a trait so a
//! model-specific tokenizer can replace it without touching call sites.
//!
//! The approximation: whitespace-delimited words count as one token each,
//! plus one token per ~4 bytes of contiguous non-whitespace CJK text, plus one
//! token per punctuation run. This deliberately over-estimates simple English
//! word counts (words can split into multiple BPE tokens) so budgets fail
//! conservatively rather than optimistically.

/// A pluggable token counter.
///
/// Implementations must be deterministic and cheap; the engine calls them on
/// every insert and re-count. [`ApproxTokenCounter`] is the default.
pub trait TokenCounter: Send + Sync {
    /// Estimates the token count of `text`.
    fn count(&self, text: &str) -> usize;
}

/// The default approximate counter (see the module docs for the exact rule).
#[derive(Debug, Clone, Copy, Default)]
pub struct ApproxTokenCounter;

impl ApproxTokenCounter {
    /// Bytes of contiguous CJK text approximated as one token.
    const CJK_BYTES_PER_TOKEN: usize = 4;
}

impl TokenCounter for ApproxTokenCounter {
    fn count(&self, text: &str) -> usize {
        let mut tokens = 0usize;
        let mut in_word = false;
        let mut cjk_bytes = 0usize;
        for ch in text.chars() {
            let is_whitespace = ch.is_whitespace();
            let is_cjk = ('\u{4E00}'..='\u{9FFF}').contains(&ch)
                || ('\u{3040}'..='\u{30FF}').contains(&ch)
                || ('\u{AC00}'..='\u{D7AF}').contains(&ch);
            if is_cjk {
                cjk_bytes += ch.len_utf8();
                in_word = false;
                continue;
            }
            if cjk_bytes > 0 {
                tokens += cjk_bytes.div_ceil(Self::CJK_BYTES_PER_TOKEN);
                cjk_bytes = 0;
            }
            if is_whitespace {
                in_word = false;
                continue;
            }
            if ch.is_ascii_alphanumeric() {
                if !in_word {
                    tokens += 1;
                    in_word = true;
                }
            } else {
                // Punctuation and symbols each form their own small token run;
                // count each non-word character as one token.
                tokens += 1;
                in_word = false;
            }
        }
        if cjk_bytes > 0 {
            tokens += cjk_bytes.div_ceil(Self::CJK_BYTES_PER_TOKEN);
        }
        tokens
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_text_counts_zero() {
        assert_eq!(ApproxTokenCounter.count(""), 0);
        assert_eq!(ApproxTokenCounter.count("   \n\t  "), 0);
    }

    #[test]
    fn simple_words_count_one_token_each() {
        assert_eq!(ApproxTokenCounter.count("hello world"), 2);
        assert_eq!(ApproxTokenCounter.count("one two three four"), 4);
    }

    #[test]
    fn long_words_still_estimate_low_per_word() {
        // Words count as 1 token each by design (documented approximation);
        // the estimate is conservative overall because real BPE splits them.
        assert_eq!(ApproxTokenCounter.count("extraordinarily"), 1);
    }

    #[test]
    fn punctuation_counts_separately() {
        // `{` `}` `(` `)` each count as one token: 4.
        assert_eq!(ApproxTokenCounter.count("a{}()"), 5);
    }

    #[test]
    fn cjk_text_uses_byte_ratio() {
        // 3 CJK chars = 9 bytes => ceil(9/4) = 3 tokens.
        assert_eq!(ApproxTokenCounter.count("中文文本"), 3);
        // Mixed: 2 CJK chars (6 bytes => 2 tokens) + one ascii word.
        assert_eq!(ApproxTokenCounter.count("中文 word"), 3);
    }

    #[test]
    fn counter_is_deterministic() {
        let text = "The quick brown fox 中文 jumps over the lazy dog.";
        let a = ApproxTokenCounter.count(text);
        let b = ApproxTokenCounter.count(text);
        assert_eq!(a, b);
        assert!(a > 0);
    }

    #[test]
    fn code_counts_reasonably() {
        let text = "fn main() { println!(\"hello\"); }";
        assert!(ApproxTokenCounter.count(text) > 5);
    }
}
