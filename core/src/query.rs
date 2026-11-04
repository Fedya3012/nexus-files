//! Omnibar query parsing.
//!
//! One box does four jobs (see mockup 03):
//! - `> đổi tên`   → command palette
//! - `@ du an`     → GoTo a folder by name (frecency)
//! - `# công việc` → tags (Beta; parsed now so the UI can say so)
//! - `C:\Users`    → path with autocompletion
//! - anything else → search, with filters such as `ext:xlsx,pdf`,
//!   `size:>10mb`, `sửa:30d` / `modified:7d`, `kind:ảnh`.

use crate::fold::fold;
use crate::model::FileKind;

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum OmnibarMode {
    Search,
    Command,
    GoTo,
    Tag,
    Path,
}

/// A filter recognised in the query, shown as a removable chip.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct FilterChip {
    /// Display text, e.g. "ext: xlsx, pdf".
    pub label: String,
    /// The exact token in the query, so the UI can remove it.
    pub token: String,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct SearchFilters {
    pub extensions: Vec<String>,
    pub min_size: Option<u64>,
    pub max_size: Option<u64>,
    pub modified_within_ms: Option<i64>,
    pub kinds: Vec<FileKind>,
    pub folders_only: bool,
    pub files_only: bool,
}

#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct ParsedQuery {
    pub mode: OmnibarMode,
    /// Free text left after removing the prefix and filters.
    pub text: String,
    pub chips: Vec<FilterChip>,
}

pub struct Parsed {
    pub public: ParsedQuery,
    pub filters: SearchFilters,
    /// Folded words that must all appear in the name.
    pub terms: Vec<String>,
}

pub fn looks_like_path(s: &str) -> bool {
    let b = s.as_bytes();
    s.starts_with('/')
        || s.starts_with('~')
        || s.starts_with("\\\\")
        || s.starts_with('%')
        || (b.len() >= 2 && b[0].is_ascii_alphabetic() && b[1] == b':' && (b.len() == 2 || b[2] == b'\\' || b[2] == b'/'))
}

pub fn parse(input: &str) -> Parsed {
    let trimmed = input.trim_start();
    let (mode, rest) = if let Some(r) = trimmed.strip_prefix('>') {
        (OmnibarMode::Command, r)
    } else if let Some(r) = trimmed.strip_prefix('@') {
        (OmnibarMode::GoTo, r)
    } else if let Some(r) = trimmed.strip_prefix('#') {
        (OmnibarMode::Tag, r)
    } else if looks_like_path(trimmed) {
        (OmnibarMode::Path, trimmed)
    } else {
        (OmnibarMode::Search, trimmed)
    };
    let mut filters = SearchFilters::default();
    let mut chips = Vec::new();
    let mut words = Vec::new();
    if mode == OmnibarMode::Search {
        for token in rest.split_whitespace() {
            match parse_filter(token, &mut filters) {
                Some(label) => chips.push(FilterChip { label, token: token.to_string() }),
                None => words.push(token),
            }
        }
    }
    let text = if mode == OmnibarMode::Search { words.join(" ") } else { rest.trim().to_string() };
    let terms = fold(&text).split_whitespace().map(str::to_string).collect();
    Parsed { public: ParsedQuery { mode, text, chips }, filters, terms }
}

const DAY_MS: i64 = 86_400_000;

fn parse_filter(token: &str, f: &mut SearchFilters) -> Option<String> {
    let (key, value) = token.split_once(':')?;
    if value.is_empty() {
        return None;
    }
    match fold(key).as_str() {
        "ext" | "duoi" => {
            let exts: Vec<String> = value
                .split(',')
                .map(|e| e.trim().trim_start_matches('.').to_lowercase())
                .filter(|e| !e.is_empty())
                .collect();
            if exts.is_empty() {
                return None;
            }
            let label = format!("ext: {}", exts.join(", "));
            f.extensions.extend(exts);
            Some(label)
        }
        "size" | "kichthuoc" | "kt" => {
            let (op, num) = if let Some(n) = value.strip_prefix(">=").or_else(|| value.strip_prefix('>')) {
                ('>', n)
            } else if let Some(n) = value.strip_prefix("<=").or_else(|| value.strip_prefix('<')) {
                ('<', n)
            } else {
                ('>', value)
            };
            let bytes = parse_size(num)?;
            if op == '>' { f.min_size = Some(bytes) } else { f.max_size = Some(bytes) }
            Some(format!("cỡ {op} {}", num.to_uppercase()))
        }
        "modified" | "sua" | "date" | "ngay" => {
            let ms = parse_age(value)?;
            f.modified_within_ms = Some(ms);
            Some(format!("sửa: {}", human_age(ms)))
        }
        "kind" | "loai" | "type" => {
            let k = parse_kind(&fold(value))?;
            f.kinds.push(k);
            if k == FileKind::Folder {
                f.folders_only = true;
            }
            Some(format!("loại: {value}"))
        }
        _ => None,
    }
}

fn parse_size(s: &str) -> Option<u64> {
    let s = s.trim().to_lowercase();
    let idx = s.find(|c: char| !(c.is_ascii_digit() || c == '.')).unwrap_or(s.len());
    let (num, unit) = s.split_at(idx);
    let n: f64 = num.parse().ok()?;
    let mult: f64 = match unit.trim() {
        "" | "b" => 1.0,
        "k" | "kb" => 1024.0,
        "m" | "mb" => 1024.0 * 1024.0,
        "g" | "gb" => 1024.0 * 1024.0 * 1024.0,
        _ => return None,
    };
    Some((n * mult) as u64)
}

/// "30d", "30", "2w", "6m" (months), "1y", "24h", or "homnay"/"today".
fn parse_age(s: &str) -> Option<i64> {
    let s = fold(s);
    match s.as_str() {
        "today" | "homnay" => return Some(DAY_MS),
        "week" | "tuannay" | "tuan" => return Some(7 * DAY_MS),
        "month" | "thangnay" | "thang" => return Some(30 * DAY_MS),
        _ => {}
    }
    let idx = s.find(|c: char| !c.is_ascii_digit()).unwrap_or(s.len());
    let n: i64 = s[..idx].parse().ok()?;
    let unit_ms = match &s[idx..] {
        "" | "d" | "ngay" => DAY_MS,
        "h" | "gio" => DAY_MS / 24,
        "w" | "tuan" => 7 * DAY_MS,
        "m" | "thang" => 30 * DAY_MS,
        "y" | "nam" => 365 * DAY_MS,
        _ => return None,
    };
    Some(n * unit_ms)
}

fn human_age(ms: i64) -> String {
    if ms % DAY_MS != 0 {
        format!("{} giờ", ms / (DAY_MS / 24))
    } else {
        format!("{} ngày", ms / DAY_MS)
    }
}

fn parse_kind(v: &str) -> Option<FileKind> {
    Some(match v {
        "anh" | "image" | "img" | "photo" => FileKind::Image,
        "video" | "phim" => FileKind::Video,
        "nhac" | "audio" | "amthanh" | "music" => FileKind::Audio,
        "pdf" => FileKind::Pdf,
        "word" | "doc" | "vanban" => FileKind::Document,
        "excel" | "bang" | "sheet" => FileKind::Spreadsheet,
        "nen" | "archive" | "zip" => FileKind::Archive,
        "code" | "ma" => FileKind::Code,
        "thumuc" | "folder" | "dir" => FileKind::Folder,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mockup_query_parses_into_chips() {
        let p = parse("bao cao ext:xlsx,pdf sửa:30d");
        assert_eq!(p.public.mode, OmnibarMode::Search);
        assert_eq!(p.public.text, "bao cao");
        assert_eq!(p.terms, ["bao", "cao"]);
        assert_eq!(p.filters.extensions, ["xlsx", "pdf"]);
        assert_eq!(p.filters.modified_within_ms, Some(30 * DAY_MS));
        let labels: Vec<_> = p.public.chips.iter().map(|c| c.label.as_str()).collect();
        assert_eq!(labels, ["ext: xlsx, pdf", "sửa: 30 ngày"]);
    }

    #[test]
    fn prefixes_select_modes() {
        assert_eq!(parse("> đổi tên").public.mode, OmnibarMode::Command);
        assert_eq!(parse("> đổi tên").public.text, "đổi tên");
        assert_eq!(parse("@du an").public.mode, OmnibarMode::GoTo);
        assert_eq!(parse("#việc").public.mode, OmnibarMode::Tag);
        assert_eq!(parse("C:\\Users").public.mode, OmnibarMode::Path);
        assert_eq!(parse("/home").public.mode, OmnibarMode::Path);
        assert_eq!(parse("~/Docs").public.mode, OmnibarMode::Path);
    }

    #[test]
    fn size_and_kind_filters() {
        let p = parse("size:>10mb kind:ảnh x");
        assert_eq!(p.filters.min_size, Some(10 * 1024 * 1024));
        assert_eq!(p.filters.kinds, [FileKind::Image]);
        assert_eq!(p.terms, ["x"]);
        let p = parse("size:<500k");
        assert_eq!(p.filters.max_size, Some(500 * 1024));
    }

    #[test]
    fn unknown_filters_stay_as_text() {
        let p = parse("note:abc http://x");
        assert!(p.public.chips.is_empty());
        assert_eq!(p.terms.len(), 2);
    }
}
