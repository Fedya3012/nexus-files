//! Live search below a folder (Omnibar scope "Thư mục hiện tại").
//!
//! The MVP walks the tree in parallel on every query, with a cancellation
//! token and a time budget, so typing stays responsive. Whole-drive instant
//! search (MFT/USN index) is a Beta feature and will plug in behind the same
//! `SearchResults` type.

use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use parking_lot::Mutex;

use crate::fold::{char_indices_to_utf16_ranges, fold};
use crate::listing::{entry_from, system_time_ms};
use crate::model::{FileKind, TextRange};
use crate::query::{parse, OmnibarMode, Parsed, SearchFilters};

/// Cancels a running search when the user types the next character.
#[derive(Debug, Default, uniffi::Object)]
pub struct CancelToken {
    flag: AtomicBool,
}

#[uniffi::export]
impl CancelToken {
    #[uniffi::constructor]
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    pub fn cancel(&self) {
        self.flag.store(true, Ordering::Relaxed);
    }

    pub fn is_cancelled(&self) -> bool {
        self.flag.load(Ordering::Relaxed)
    }
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct SearchHit {
    pub name: String,
    pub path: String,
    /// Parent folder, shown under the name in the result list.
    pub parent: String,
    pub is_dir: bool,
    pub size: u64,
    pub modified_ms: i64,
    pub kind: FileKind,
    pub type_label: String,
    pub highlights: Vec<TextRange>,
    pub score: i64,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct SearchResults {
    pub files: Vec<SearchHit>,
    pub folders: Vec<SearchHit>,
    pub total_matches: u64,
    pub scanned: u64,
    pub elapsed_ms: u32,
    /// True when the walk stopped early (time budget or cancel), so the UI can
    /// say "kết quả có thể chưa đủ".
    pub incomplete: bool,
}

struct Matcher<'a> {
    terms: &'a [String],
    filters: &'a SearchFilters,
    now_ms: i64,
}

impl Matcher<'_> {
    /// Returns (score, matched char indices) when the name contains every term.
    fn match_name(&self, name: &str) -> Option<(i64, Vec<u32>)> {
        let folded = fold(name);
        let chars: Vec<char> = folded.chars().collect();
        let mut indices = Vec::new();
        let mut score: i64 = 0;
        for term in self.terms {
            let tchars: Vec<char> = term.chars().collect();
            let mut best: Option<(usize, i64)> = None;
            let mut i = 0;
            while i + tchars.len() <= chars.len() {
                if chars[i..i + tchars.len()] == tchars[..] {
                    let at_word = i == 0 || !chars[i - 1].is_alphanumeric();
                    let s = if i == 0 { 30 } else if at_word { 20 } else { 5 };
                    if best.is_none_or(|(_, b)| s > b) {
                        best = Some((i, s));
                    }
                    if i == 0 {
                        break;
                    }
                }
                i += 1;
            }
            let (pos, s) = best?;
            score += s;
            indices.extend((pos..pos + tchars.len()).map(|x| x as u32));
        }
        let stem_len = name.rsplit_once('.').map_or(name, |(s, _)| s).chars().count();
        let covered: usize =
            self.terms.iter().map(|t| t.chars().count()).sum::<usize>() + self.terms.len().saturating_sub(1);
        if !self.terms.is_empty() && covered >= stem_len {
            score += 40; // exact name match
        }
        score -= (chars.len() as i64) / 8;
        Some((score, indices))
    }

    fn passes_filters(&self, is_dir: bool, ext: &str, kind: FileKind, size: u64, modified_ms: i64) -> bool {
        let f = self.filters;
        if f.folders_only && !is_dir {
            return false;
        }
        if !f.extensions.is_empty() && (is_dir || !f.extensions.iter().any(|e| e == ext)) {
            return false;
        }
        if !f.kinds.is_empty() && !f.kinds.contains(&kind) {
            return false;
        }
        if (f.min_size.is_some() || f.max_size.is_some()) && is_dir {
            return false;
        }
        if f.min_size.is_some_and(|m| size < m) || f.max_size.is_some_and(|m| size > m) {
            return false;
        }
        if let Some(w) = f.modified_within_ms {
            if modified_ms < self.now_ms - w {
                return false;
            }
        }
        true
    }
}

pub fn search(root: &str, query: &str, limit: u32, budget_ms: u32, show_hidden: bool, token: &CancelToken) -> SearchResults {
    let parsed = parse(query);
    run(root, &parsed, limit, budget_ms, show_hidden, token)
}

fn run(root: &str, parsed: &Parsed, limit: u32, budget_ms: u32, show_hidden: bool, token: &CancelToken) -> SearchResults {
    let started = Instant::now();
    let empty = SearchResults { files: vec![], folders: vec![], total_matches: 0, scanned: 0, elapsed_ms: 0, incomplete: false };
    let has_filters = parsed.filters != SearchFilters::default();
    if parsed.public.mode != OmnibarMode::Search || (parsed.terms.is_empty() && !has_filters) || !Path::new(root).is_dir() {
        return empty;
    }
    let matcher = Matcher {
        terms: &parsed.terms,
        filters: &parsed.filters,
        now_ms: system_time_ms(Ok(std::time::SystemTime::now())),
    };
    let deadline = started + Duration::from_millis(budget_ms.max(50) as u64);
    let hits: Mutex<Vec<SearchHit>> = Mutex::new(Vec::new());
    let scanned = AtomicU64::new(0);
    let stopped = AtomicBool::new(false);
    // Cap memory on huge trees; the best `limit` are picked from these.
    let cap = (limit as usize).max(1) * 50;

    let walker = ignore::WalkBuilder::new(root)
        .standard_filters(false)
        .hidden(!show_hidden)
        .follow_links(false)
        .threads(std::thread::available_parallelism().map_or(4, |n| n.get().min(8)))
        .build_parallel();

    walker.run(|| {
        let matcher = &matcher;
        let hits = &hits;
        let scanned = &scanned;
        let stopped = &stopped;
        Box::new(move |res| {
            use ignore::WalkState;
            if token.is_cancelled() || Instant::now() > deadline {
                stopped.store(true, Ordering::Relaxed);
                return WalkState::Quit;
            }
            let Ok(dent) = res else { return WalkState::Continue };
            if dent.depth() == 0 {
                return WalkState::Continue;
            }
            scanned.fetch_add(1, Ordering::Relaxed);
            let name = dent.file_name().to_string_lossy();
            let Some((mut score, idx)) = matcher.match_name(&name) else { return WalkState::Continue };
            let Ok(md) = dent.metadata() else { return WalkState::Continue };
            let entry = entry_from(name.into_owned(), dent.path(), &md);
            if !matcher.passes_filters(entry.is_dir, &entry.extension, entry.kind, entry.size, entry.modified_ms) {
                return WalkState::Continue;
            }
            score -= dent.depth() as i64 * 2;
            // Recently modified files float up a little.
            let age_days = (matcher.now_ms - entry.modified_ms).max(0) / 86_400_000;
            score += 10 - age_days.min(10);
            let path = dent.path();
            let hit = SearchHit {
                highlights: char_indices_to_utf16_ranges(&entry.name, &idx),
                path: path.display().to_string(),
                parent: path.parent().map(|p| p.display().to_string()).unwrap_or_default(),
                name: entry.name,
                is_dir: entry.is_dir,
                size: entry.size,
                modified_ms: entry.modified_ms,
                kind: entry.kind,
                type_label: entry.type_label,
                score,
            };
            let mut h = hits.lock();
            h.push(hit);
            if h.len() >= cap {
                stopped.store(true, Ordering::Relaxed);
                return WalkState::Quit;
            }
            WalkState::Continue
        })
    });

    let mut all = hits.into_inner();
    let total = all.len() as u64;
    all.sort_by(|a, b| b.score.cmp(&a.score).then_with(|| a.name.len().cmp(&b.name.len())));
    let (mut folders, mut files): (Vec<_>, Vec<_>) = all.into_iter().partition(|h| h.is_dir);
    folders.truncate((limit as usize / 3).max(3));
    files.truncate(limit as usize);
    SearchResults {
        files,
        folders,
        total_matches: total,
        scanned: scanned.load(Ordering::Relaxed),
        elapsed_ms: started.elapsed().as_millis() as u32,
        incomplete: stopped.load(Ordering::Relaxed),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn tree() -> tempfile::TempDir {
        let t = tempfile::tempdir().unwrap();
        let p = t.path();
        fs::create_dir_all(p.join("Kế toán/2026")).unwrap();
        fs::create_dir_all(p.join("Báo cáo khách hàng")).unwrap();
        fs::write(p.join("Báo cáo quý 3.xlsx"), b"x").unwrap();
        fs::write(p.join("Kế toán/2026/Báo cáo tài chính 2026.pdf"), vec![0u8; 2048]).unwrap();
        fs::write(p.join("Mẫu báo cáo tuần.pdf"), b"x").unwrap();
        fs::write(p.join("notes.md"), b"x").unwrap();
        t
    }

    #[test]
    fn finds_accent_insensitive_with_highlights() {
        let t = tree();
        let r = search(t.path().to_str().unwrap(), "bao cao", 20, 2000, false, &CancelToken::default());
        assert_eq!(r.files.len(), 3);
        assert_eq!(r.folders.len(), 1);
        // A name starting with the query ranks above "Mẫu báo cáo tuần".
        assert_ne!(r.files[0].name, "Mẫu báo cáo tuần.pdf");
        let mau = r.files.iter().find(|h| h.name.starts_with("Mẫu")).unwrap();
        assert_eq!(mau.highlights, vec![TextRange { start: 4, length: 3 }, TextRange { start: 8, length: 3 }]);
    }

    #[test]
    fn filters_restrict_results() {
        let t = tree();
        let root = t.path().to_str().unwrap();
        let r = search(root, "bao cao ext:pdf", 20, 2000, false, &CancelToken::default());
        assert_eq!(r.files.len(), 2);
        assert!(r.folders.is_empty());
        let r = search(root, "ext:pdf size:>1kb", 20, 2000, false, &CancelToken::default());
        assert_eq!(r.files.len(), 1);
        let r = search(root, "kind:folder", 20, 2000, false, &CancelToken::default());
        assert_eq!(r.folders.len(), 3);
    }

    #[test]
    fn cancelled_token_stops_immediately() {
        let t = tree();
        let tok = CancelToken::default();
        tok.cancel();
        let r = search(t.path().to_str().unwrap(), "bao", 20, 2000, false, &tok);
        assert!(r.files.is_empty());
        assert!(r.incomplete);
    }

    #[test]
    fn empty_query_returns_nothing() {
        let t = tree();
        let r = search(t.path().to_str().unwrap(), "  ", 20, 2000, false, &CancelToken::default());
        assert_eq!(r.scanned, 0);
    }
}
