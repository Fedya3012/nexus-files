//! `NexusCore`: the stateful object the UI holds for the app's lifetime.

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use parking_lot::Mutex;
use serde::{Deserialize, Serialize};

use crate::commands::{match_commands, CommandMatch};
use crate::error::{NexusError, Result};
use crate::fold::fold;
use crate::fuzzy::Fuzzy;
use crate::listing::{normalize_path, system_time_ms};
use crate::model::{Place, PlaceKind, TextRange};
use crate::ops::{Engine, FileOperation, HistoryEntry, JobStatus};
use crate::places::{drives, known_folders};
use crate::query::{parse, OmnibarMode, ParsedQuery};
use crate::search::{CancelToken, SearchResults};
use crate::session::{SessionState, Settings};
use crate::store::JsonStore;

const MAX_VISITS: usize = 2000;
const SEARCH_BUDGET_MS: u32 = 3000;

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
struct Visit {
    count: u32,
    last_ms: i64,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct PlaceMatch {
    pub place: Place,
    pub highlights: Vec<TextRange>,
    pub score: i64,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct OmnibarResults {
    pub parsed: ParsedQuery,
    pub commands: Vec<CommandMatch>,
    pub places: Vec<PlaceMatch>,
    /// Message to show instead of results (e.g. "Tags có ở bản Beta").
    pub hint: Option<String>,
}

#[derive(uniffi::Object)]
pub struct NexusCore {
    store: JsonStore,
    engine: Arc<Engine>,
    pins: Mutex<Vec<String>>,
    visits: Mutex<HashMap<String, Visit>>,
    settings: Mutex<Settings>,
}

fn frecency(v: &Visit, now: i64) -> f64 {
    let age_h = (now - v.last_ms) as f64 / 3_600_000.0;
    let w = if age_h < 4.0 {
        4.0
    } else if age_h < 24.0 {
        2.0
    } else if age_h < 24.0 * 7.0 {
        1.0
    } else {
        0.5
    };
    v.count as f64 * w
}

fn display_name(path: &str) -> String {
    Path::new(path).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| path.to_string())
}

fn key(path: &str) -> String {
    if cfg!(windows) { path.to_lowercase() } else { path.to_string() }
}

#[uniffi::export]
impl NexusCore {
    /// Opens (or creates) the app data folder, e.g. `%LOCALAPPDATA%/NexusFiles`.
    #[uniffi::constructor]
    pub fn new(data_dir: String) -> Result<Arc<Self>> {
        let store = JsonStore::open(Path::new(&data_dir))?;
        let engine = Engine::new(store.clone());
        Ok(Arc::new(NexusCore {
            pins: Mutex::new(store.load("pins")),
            visits: Mutex::new(store.load("visits")),
            settings: Mutex::new(store.load("settings")),
            store,
            engine,
        }))
    }

    pub fn data_dir(&self) -> String {
        self.store.dir().display().to_string()
    }

    // ---- settings & session -------------------------------------------

    pub fn load_settings(&self) -> Settings {
        self.settings.lock().clone()
    }

    pub fn save_settings(&self, settings: Settings) -> Result<()> {
        self.store.save("settings", &settings)?;
        *self.settings.lock() = settings;
        Ok(())
    }

    pub fn load_session(&self) -> SessionState {
        self.store.load("session")
    }

    pub fn save_session(&self, session: SessionState) -> Result<()> {
        self.store.save("session", &session)
    }

    // ---- pinned folders ("Đã ghim") -----------------------------------

    pub fn pinned(&self) -> Vec<Place> {
        self.pins
            .lock()
            .iter()
            .map(|p| Place { name: display_name(p), path: p.clone(), kind: PlaceKind::Pinned })
            .collect()
    }

    pub fn is_pinned(&self, path: String) -> bool {
        let k = key(&path);
        self.pins.lock().iter().any(|p| key(p) == k)
    }

    pub fn pin(&self, path: String) -> Result<()> {
        let p = normalize_path(&path)?.display().to_string();
        if !Path::new(&p).is_dir() {
            return Err(NexusError::NotFound { path: p });
        }
        let mut pins = self.pins.lock();
        if !pins.iter().any(|x| key(x) == key(&p)) {
            pins.push(p);
        }
        self.store.save("pins", &*pins)
    }

    pub fn unpin(&self, path: String) -> Result<()> {
        let mut pins = self.pins.lock();
        pins.retain(|x| key(x) != key(&path));
        self.store.save("pins", &*pins)
    }

    pub fn move_pin(&self, path: String, new_index: u32) -> Result<()> {
        let mut pins = self.pins.lock();
        if let Some(i) = pins.iter().position(|x| key(x) == key(&path)) {
            let item = pins.remove(i);
            let idx = (new_index as usize).min(pins.len());
            pins.insert(idx, item);
        }
        self.store.save("pins", &*pins)
    }

    // ---- GoTo / frecency ------------------------------------------------

    /// Call when the user opens a folder; feeds GoTo ranking.
    pub fn record_visit(&self, path: String) {
        let now = system_time_ms(Ok(std::time::SystemTime::now()));
        let mut v = self.visits.lock();
        let e = v.entry(path).or_default();
        e.count = e.count.saturating_add(1);
        e.last_ms = now;
        if v.len() > MAX_VISITS {
            let mut all: Vec<(String, f64)> = v.iter().map(|(k, x)| (k.clone(), frecency(x, now))).collect();
            all.sort_by(|a, b| a.1.total_cmp(&b.1));
            for (k, _) in all.into_iter().take(v.len() - MAX_VISITS) {
                v.remove(&k);
            }
        }
        let _ = self.store.save("visits", &*v);
    }

    pub fn recent_folders(&self, limit: u32) -> Vec<Place> {
        let v = self.visits.lock();
        let mut all: Vec<(&String, &Visit)> = v.iter().collect();
        all.sort_by_key(|x| std::cmp::Reverse(x.1.last_ms));
        all.into_iter()
            .filter(|(p, _)| Path::new(p).is_dir())
            .take(limit as usize)
            .map(|(p, _)| Place { name: display_name(p), path: p.clone(), kind: PlaceKind::Recent })
            .collect()
    }

    /// Jump to a folder by typing part of its name ("@du an" → "Dự án 2026").
    pub fn goto(&self, query: String, limit: u32) -> Vec<PlaceMatch> {
        let now = system_time_ms(Ok(std::time::SystemTime::now()));
        let mut candidates: Vec<(Place, f64)> = Vec::new();
        let mut seen = std::collections::HashSet::new();
        let push = |candidates: &mut Vec<(Place, f64)>, place: Place, boost: f64, seen: &mut std::collections::HashSet<String>| {
            if seen.insert(key(&place.path)) {
                candidates.push((place, boost));
            }
        };
        for p in self.pinned() {
            push(&mut candidates, p, 5.0, &mut seen);
        }
        for p in known_folders() {
            push(&mut candidates, p, 2.0, &mut seen);
        }
        for d in drives() {
            push(&mut candidates, Place { name: d.name, path: d.path, kind: PlaceKind::Drive }, 1.0, &mut seen);
        }
        {
            let v = self.visits.lock();
            for (path, visit) in v.iter() {
                let f = frecency(visit, now);
                if let Some((_, b)) = candidates.iter_mut().find(|(c, _)| key(&c.path) == key(path)) {
                    *b += f;
                } else {
                    push(&mut candidates, Place { name: display_name(path), path: path.clone(), kind: PlaceKind::Recent }, f, &mut seen);
                }
            }
        }
        let mut fz = Fuzzy::new(&query);
        let mut out: Vec<PlaceMatch> = Vec::new();
        for (place, boost) in candidates {
            let (score, highlights) = if fz.is_empty() {
                (0i64, vec![])
            } else if let Some((s, h)) = fz.score(&place.name) {
                (s as i64 * 2, h)
            } else if let Some((s, _)) = fz.score(&place.path) {
                (s as i64 / 2, vec![])
            } else {
                continue;
            };
            out.push(PlaceMatch { place, highlights, score: score + (boost * 10.0) as i64 });
        }
        out.sort_by_key(|m| std::cmp::Reverse(m.score));
        out.retain(|m| Path::new(&m.place.path).is_dir());
        out.truncate(limit as usize);
        out
    }

    // ---- Omnibar --------------------------------------------------------

    /// Instant part of the Omnibar (commands, folders, path completion).
    /// File results come from `search`, which the UI runs in the background.
    pub fn omnibar(&self, input: String, current_dir: String, limit: u32) -> OmnibarResults {
        let parsed = parse(&input);
        let text = parsed.public.text.clone();
        let mut res = OmnibarResults { parsed: parsed.public, commands: vec![], places: vec![], hint: None };
        match res.parsed.mode {
            OmnibarMode::Command => res.commands = match_commands(&text, limit as usize),
            OmnibarMode::GoTo => res.places = self.goto(text, limit),
            OmnibarMode::Tag => res.hint = Some("Tags sẽ có ở bản Beta.".into()),
            OmnibarMode::Path => res.places = path_completions(&input, &current_dir, limit),
            OmnibarMode::Search => {
                if !text.trim().is_empty() {
                    res.places = self.goto(text.clone(), 3);
                    res.commands = match_commands(&text, 2).into_iter().filter(|c| c.score > 0).collect();
                }
            }
        }
        res
    }

    /// Searches file and folder names below `root`. Run it off the UI thread
    /// and cancel the previous token on every keystroke.
    pub fn search(&self, root: String, query: String, limit: u32, token: Arc<CancelToken>) -> SearchResults {
        let hidden = self.settings.lock().show_hidden;
        crate::search::search(&root, &query, limit, SEARCH_BUDGET_MS, hidden, &token)
    }

    // ---- file operations -------------------------------------------------

    /// Starts a copy/move/trash in the background; poll `jobs()` for progress.
    pub fn enqueue(&self, op: FileOperation) -> Result<u64> {
        self.engine.enqueue(op)
    }

    /// Runs a quick operation (rename, new folder/file) and waits for it.
    pub fn execute(&self, op: FileOperation) -> Result<JobStatus> {
        self.engine.execute(op)
    }

    pub fn jobs(&self) -> Vec<JobStatus> {
        self.engine.jobs()
    }

    pub fn job(&self, id: u64) -> Option<JobStatus> {
        self.engine.job(id)
    }

    pub fn cancel_job(&self, id: u64) {
        self.engine.cancel(id)
    }

    pub fn clear_finished_jobs(&self) {
        self.engine.clear_finished()
    }

    /// Undoes the last operation (Ctrl+Z). Returns the undo job id.
    pub fn undo(&self) -> Option<u64> {
        self.engine.undo_last()
    }

    pub fn undo_title(&self) -> Option<String> {
        self.engine.undo_title()
    }

    pub fn history(&self, limit: u32) -> Vec<HistoryEntry> {
        self.engine.history(limit)
    }
}

/// Completes the last segment of a typed path with matching child folders.
fn path_completions(input: &str, current_dir: &str, limit: u32) -> Vec<PlaceMatch> {
    let trimmed = input.trim();
    let ends_with_sep = trimmed.ends_with(['/', '\\']);
    let Ok(full) = normalize_path(trimmed) else { return vec![] };
    let _ = current_dir;
    let (dir, prefix) = if ends_with_sep || (full.is_dir() && full.parent().is_none()) {
        (full.clone(), String::new())
    } else {
        match (full.parent(), full.file_name()) {
            (Some(p), Some(n)) => (p.to_path_buf(), fold(&n.to_string_lossy())),
            _ => (full.clone(), String::new()),
        }
    };
    let Ok(rd) = std::fs::read_dir(&dir) else { return vec![] };
    let mut out: Vec<PlaceMatch> = rd
        .flatten()
        .filter(|e| e.file_type().map(|t| t.is_dir()).unwrap_or(false))
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().into_owned();
            if name.starts_with('.') && !prefix.starts_with('.') {
                return None;
            }
            let folded = fold(&name);
            if !folded.starts_with(&prefix) {
                return None;
            }
            let n = prefix.chars().count() as u32;
            let hl = if n > 0 { crate::fold::char_indices_to_utf16_ranges(&name, &(0..n).collect::<Vec<_>>()) } else { vec![] };
            Some(PlaceMatch { place: Place { name, path: e.path().display().to_string(), kind: PlaceKind::Recent }, highlights: hl, score: 0 })
        })
        .collect();
    out.sort_by(|a, b| crate::listing::natural_cmp(&a.place.name, &b.place.name));
    out.truncate(limit as usize);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn core() -> (tempfile::TempDir, Arc<NexusCore>) {
        let t = tempfile::tempdir().unwrap();
        let c = NexusCore::new(t.path().join("data").display().to_string()).unwrap();
        (t, c)
    }

    #[test]
    fn pins_persist_and_reorder() {
        let (t, c) = core();
        let a = t.path().join("Dự án 2026");
        let b = t.path().join("Hóa đơn");
        fs::create_dir_all(&a).unwrap();
        fs::create_dir_all(&b).unwrap();
        c.pin(a.display().to_string()).unwrap();
        c.pin(b.display().to_string()).unwrap();
        c.pin(a.display().to_string()).unwrap(); // no duplicate
        c.move_pin(b.display().to_string(), 0).unwrap();
        let again = NexusCore::new(c.data_dir()).unwrap();
        let names: Vec<_> = again.pinned().into_iter().map(|p| p.name).collect();
        assert_eq!(names, ["Hóa đơn", "Dự án 2026"]);
        assert!(c.pin(t.path().join("missing").display().to_string()).is_err());
    }

    #[test]
    fn goto_prefers_frequent_folders() {
        let (t, c) = core();
        let a = t.path().join("Dự án 2026");
        let b = t.path().join("Dữ liệu cũ");
        fs::create_dir_all(&a).unwrap();
        fs::create_dir_all(&b).unwrap();
        for _ in 0..5 {
            c.record_visit(a.display().to_string());
        }
        c.record_visit(b.display().to_string());
        let r = c.goto("du an".into(), 5);
        assert_eq!(r[0].place.path, a.display().to_string());
        assert!(!r[0].highlights.is_empty());
    }

    #[test]
    fn omnibar_modes() {
        let (t, c) = core();
        fs::create_dir_all(t.path().join("Documents/Công việc")).unwrap();
        let cmds = c.omnibar("> doi ten".into(), String::new(), 5);
        assert_eq!(cmds.commands[0].command.id, "file.rename");
        let path = format!("{}/Documents/cong", t.path().display());
        let r = c.omnibar(path, String::new(), 5);
        assert_eq!(r.parsed.mode, OmnibarMode::Path);
        assert_eq!(r.places[0].place.name, "Công việc");
        assert!(c.omnibar("#x".into(), String::new(), 5).hint.is_some());
    }

    #[test]
    fn session_and_settings_round_trip() {
        use crate::session::*;
        let (_t, c) = core();
        assert!(c.load_session().tabs.is_empty());
        let pane = PaneState { path: "/tmp".into(), view_mode: ViewMode::Icons, sort: Default::default() };
        let s = SessionState { tabs: vec![TabState { left: pane.clone(), right: Some(pane), right_is_active: true }], active_tab: 0, ..Default::default() };
        c.save_session(s.clone()).unwrap();
        assert_eq!(c.load_session(), s);
        let mut st = c.load_settings();
        st.theme = ThemePreference::Dark;
        c.save_settings(st.clone()).unwrap();
        assert_eq!(NexusCore::new(c.data_dir()).unwrap().load_settings(), st);
    }
}
