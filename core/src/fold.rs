//! Accent- and case-insensitive text folding.
//!
//! Vietnamese users type "bao cao" and expect to find "Báo cáo". We fold every
//! character to a single base character (NFD, drop combining marks, map đ→d,
//! lowercase). Folding is strictly one char in, one char out, so match
//! positions in the folded string are also positions in the original string.

use unicode_normalization::char::{decompose_canonical, is_combining_mark};

/// Folds one character to its lowercase, accent-free base character.
pub fn fold_char(c: char) -> char {
    if c.is_ascii() {
        return c.to_ascii_lowercase();
    }
    match c {
        'đ' | 'Đ' => return 'd',
        'ø' | 'Ø' => return 'o',
        'ł' | 'Ł' => return 'l',
        _ => {}
    }
    let mut base = None;
    decompose_canonical(c, |d| {
        if base.is_none() && !is_combining_mark(d) {
            base = Some(d);
        }
    });
    let base = base.unwrap_or(c);
    base.to_lowercase().next().unwrap_or(base)
}

/// Folds a whole string (char count is preserved).
pub fn fold(s: &str) -> String {
    s.chars().map(fold_char).collect()
}

/// Converts char indices of `s` into UTF-16 code-unit ranges, merging
/// neighbours. C# and Kotlin strings are UTF-16, so the UI can use the ranges
/// directly to bold the matched characters.
pub fn char_indices_to_utf16_ranges(s: &str, indices: &[u32]) -> Vec<crate::model::TextRange> {
    let mut sorted: Vec<u32> = indices.to_vec();
    sorted.sort_unstable();
    sorted.dedup();
    let mut offsets = Vec::with_capacity(s.chars().count() + 1);
    let mut acc = 0u32;
    for c in s.chars() {
        offsets.push(acc);
        acc += c.len_utf16() as u32;
    }
    offsets.push(acc);
    let mut out: Vec<crate::model::TextRange> = Vec::new();
    for idx in sorted {
        let i = idx as usize;
        if i + 1 >= offsets.len() {
            continue;
        }
        let (start, end) = (offsets[i], offsets[i + 1]);
        match out.last_mut() {
            Some(last) if last.start + last.length == start => last.length += end - start,
            _ => out.push(crate::model::TextRange { start, length: end - start }),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn folds_vietnamese() {
        assert_eq!(fold("Báo cáo Quý 3"), "bao cao quy 3");
        assert_eq!(fold("Đề xuất dự án"), "de xuat du an");
        assert_eq!(fold("Hóa đơn tháng 10"), "hoa don thang 10");
        assert_eq!(fold("Ảnh gia đình"), "anh gia dinh");
    }

    #[test]
    fn folding_preserves_char_count() {
        let s = "Nguyễn Ánh – ước mơ 🎉";
        assert_eq!(s.chars().count(), fold(s).chars().count());
    }

    #[test]
    fn utf16_ranges_merge_and_handle_surrogates() {
        let r = char_indices_to_utf16_ranges("a🎉bc", &[0, 1, 3]);
        assert_eq!(r.len(), 2);
        assert_eq!((r[0].start, r[0].length), (0, 3)); // 'a' + surrogate pair
        assert_eq!((r[1].start, r[1].length), (4, 1));
    }
}
