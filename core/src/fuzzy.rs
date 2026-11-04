//! Fuzzy matching for short candidate lists (commands, GoTo folders),
//! on accent-folded text so "doi ten" finds "Đổi tên".

use nucleo_matcher::pattern::{AtomKind, CaseMatching, Normalization, Pattern};
use nucleo_matcher::{Config, Matcher, Utf32Str};

use crate::fold::{char_indices_to_utf16_ranges, fold};
use crate::model::TextRange;

pub struct Fuzzy {
    matcher: Matcher,
    pattern: Pattern,
    buf: Vec<char>,
}

impl Fuzzy {
    pub fn new(query: &str) -> Self {
        let mut config = Config::DEFAULT;
        config.prefer_prefix = true;
        Fuzzy {
            matcher: Matcher::new(config),
            pattern: Pattern::new(&fold(query), CaseMatching::Ignore, Normalization::Never, AtomKind::Fuzzy),
            buf: Vec::new(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.pattern.atoms.is_empty()
    }

    /// Score and highlight ranges (UTF-16) of `text`, or `None` if no match.
    pub fn score(&mut self, text: &str) -> Option<(u32, Vec<TextRange>)> {
        let folded = fold(text);
        let mut indices = Vec::new();
        let score = self.pattern.indices(Utf32Str::new(&folded, &mut self.buf), &mut self.matcher, &mut indices)?;
        Some((score, char_indices_to_utf16_ranges(text, &indices)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_without_accents() {
        let mut f = Fuzzy::new("doi ten");
        let (_, ranges) = f.score("Đổi tên").unwrap();
        assert_eq!(ranges.len(), 2);
        assert!(f.score("Xóa").is_none());
    }

    #[test]
    fn prefix_beats_scattered() {
        let mut f = Fuzzy::new("du an");
        let a = f.score("Dự án 2026").unwrap().0;
        let b = f.score("Dữ liệu quan trọng năm").map(|x| x.0).unwrap_or(0);
        assert!(a > b);
    }
}
