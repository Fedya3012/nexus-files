//! File operation engine: background jobs with progress, cancellation and a
//! persistent undo journal.
//!
//! Every job records the inverse of each step it completes *as it goes*, so
//! even a job that fails or is cancelled halfway can be undone exactly up to
//! where it stopped.

pub mod fsops;
pub mod types;

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;

use parking_lot::Mutex;

use crate::error::{IoContext, NexusError, Result};
use crate::listing::{normalize_path, system_time_ms};
use crate::store::JsonStore;
use fsops::{Progress, Totals};
pub use types::*;

const JOURNAL_FILE: &str = "journal";
const JOURNAL_MAX_ENTRIES: usize = 500;
const JOURNAL_MAX_AGE_MS: i64 = 30 * 86_400_000;
const FINISHED_JOBS_KEPT: usize = 30;

fn now_ms() -> i64 {
    system_time_ms(Ok(std::time::SystemTime::now()))
}

struct JobRecord {
    status: JobStatus,
    cancel: Arc<AtomicBool>,
}

#[derive(Clone)]
enum JobKind {
    Op(FileOperation),
    Undo { entry_id: u64, title: String },
}

pub struct Engine {
    store: JsonStore,
    jobs: Mutex<Vec<JobRecord>>,
    journal: Mutex<Vec<JournalEntry>>,
    next_id: AtomicU64,
}

/// Live progress of one job, shared between the worker and `jobs()`.
struct JobProgress<'a> {
    engine: &'a Engine,
    id: u64,
    cancel: &'a AtomicBool,
}

impl Progress for JobProgress<'_> {
    fn add_bytes(&self, n: u64) {
        self.engine.update(self.id, |s| s.bytes_done += n);
    }
    fn cancelled(&self) -> bool {
        self.cancel.load(Ordering::Relaxed)
    }
}

/// Collects the undo steps and results of a running job.
#[derive(Default)]
struct Outcome {
    undo: Vec<UndoAction>,
    results: Vec<String>,
    errors: Vec<ItemError>,
    skipped: u32,
}

impl Outcome {
    fn fail(&mut self, path: &Path, e: NexusError) {
        self.errors.push(ItemError { path: path.display().to_string(), message: e.to_string() });
    }
}

impl Engine {
    pub fn new(store: JsonStore) -> Arc<Self> {
        let mut journal: Vec<JournalEntry> = store.load(JOURNAL_FILE);
        let cutoff = now_ms() - JOURNAL_MAX_AGE_MS;
        journal.retain(|e| e.time_ms >= cutoff);
        let next = journal.iter().map(|e| e.id).max().unwrap_or(0) + 1;
        Arc::new(Engine { store, jobs: Mutex::new(Vec::new()), journal: Mutex::new(journal), next_id: AtomicU64::new(next) })
    }

    fn new_id(&self) -> u64 {
        self.next_id.fetch_add(1, Ordering::Relaxed)
    }

    fn update(&self, id: u64, f: impl FnOnce(&mut JobStatus)) {
        if let Some(j) = self.jobs.lock().iter_mut().find(|j| j.status.id == id) {
            f(&mut j.status);
        }
    }

    /// Starts a job on a background thread and returns its id at once.
    pub fn enqueue(self: &Arc<Self>, op: FileOperation) -> Result<u64> {
        validate(&op)?;
        Ok(self.start(JobKind::Op(op)))
    }

    fn start(self: &Arc<Self>, kind: JobKind) -> u64 {
        let id = self.new_id();
        let cancel = Arc::new(AtomicBool::new(false));
        let status = JobStatus {
            id,
            title: title_of(&kind),
            state: JobState::Queued,
            files_done: 0,
            files_total: 0,
            bytes_done: 0,
            bytes_total: 0,
            current_item: String::new(),
            errors: vec![],
            skipped: 0,
            result_paths: vec![],
            undoable: false,
        };
        self.jobs.lock().push(JobRecord { status, cancel: cancel.clone() });
        let me = self.clone();
        std::thread::Builder::new()
            .name(format!("nexus-job-{id}"))
            .spawn(move || me.run(id, kind, &cancel))
            .expect("spawn job thread");
        id
    }

    /// Runs a quick operation (rename, new folder) on the calling thread and
    /// returns its final status, so the UI can show an error immediately.
    pub fn execute(&self, op: FileOperation) -> Result<JobStatus> {
        validate(&op)?;
        let id = self.new_id();
        let kind = JobKind::Op(op);
        let cancel = AtomicBool::new(false);
        self.jobs.lock().push(JobRecord {
            status: JobStatus {
                id,
                title: title_of(&kind),
                state: JobState::Running,
                files_done: 0,
                files_total: 1,
                bytes_done: 0,
                bytes_total: 0,
                current_item: String::new(),
                errors: vec![],
                skipped: 0,
                result_paths: vec![],
                undoable: false,
            },
            cancel: Arc::new(AtomicBool::new(false)),
        });
        self.run(id, kind, &cancel);
        let status = self.job(id).expect("job exists");
        if status.state == JobState::Failed {
            if let Some(e) = status.errors.first() {
                return Err(NexusError::Io { path: e.path.clone(), message: e.message.clone() });
            }
        }
        Ok(status)
    }

    fn run(&self, id: u64, kind: JobKind, cancel: &AtomicBool) {
        self.update(id, |s| s.state = JobState::Running);
        let progress = JobProgress { engine: self, id, cancel };
        let mut out = Outcome::default();
        match &kind {
            JobKind::Op(op) => self.run_op(id, op, &progress, &mut out),
            JobKind::Undo { entry_id, .. } => self.run_undo(id, *entry_id, &progress, &mut out),
        }
        let cancelled = cancel.load(Ordering::Relaxed);
        let undoable = matches!(kind, JobKind::Op(_)) && !out.undo.is_empty();
        if undoable {
            self.record(JournalEntry { id, time_ms: now_ms(), title: title_of(&kind), actions: std::mem::take(&mut out.undo), undone: false });
        }
        let nothing_done = out.results.is_empty() && !undoable;
        self.update(id, |s| {
            s.state = if cancelled {
                JobState::Cancelled
            } else if out.errors.is_empty() && out.skipped == 0 {
                JobState::Completed
            } else if nothing_done && !out.errors.is_empty() {
                JobState::Failed
            } else {
                JobState::CompletedWithErrors
            };
            s.errors = std::mem::take(&mut out.errors);
            s.skipped = out.skipped;
            s.result_paths = std::mem::take(&mut out.results);
            s.undoable = undoable;
            s.current_item.clear();
        });
        self.prune_finished();
    }

    fn run_op(&self, id: u64, op: &FileOperation, p: &JobProgress, out: &mut Outcome) {
        match op {
            FileOperation::Copy { sources, destination, conflict } => self.transfer(id, sources, destination, *conflict, false, p, out),
            FileOperation::Move { sources, destination, conflict } => self.transfer(id, sources, destination, *conflict, true, p, out),
            FileOperation::Rename { path, new_name } => {
                let src = PathBuf::from(path);
                let Some(parent) = src.parent() else {
                    out.fail(&src, NexusError::invalid("không thể đổi tên thư mục gốc"));
                    return;
                };
                let dest = parent.join(new_name);
                // Case-only rename ("anh.jpg" → "Anh.jpg") is allowed on Windows.
                let case_only = dest.to_string_lossy().to_lowercase() == src.to_string_lossy().to_lowercase();
                if dest == src {
                    return;
                }
                if fsops::exists(&dest) && !case_only {
                    out.fail(&dest, NexusError::AlreadyExists { path: dest.display().to_string() });
                    return;
                }
                match std::fs::rename(&src, &dest).at(&src) {
                    Ok(()) => {
                        out.undo.push(UndoAction::MoveBack { current: dest.display().to_string(), original: path.clone() });
                        out.results.push(dest.display().to_string());
                        self.update(id, |s| s.files_done = 1);
                    }
                    Err(e) => out.fail(&src, e),
                }
            }
            FileOperation::Trash { paths } => {
                self.update(id, |s| s.files_total = paths.len() as u64);
                for path in paths {
                    if p.cancelled() {
                        break;
                    }
                    let src = PathBuf::from(path);
                    self.update(id, |s| s.current_item = path.clone());
                    let at = now_ms();
                    match fsops::trash_one(&src) {
                        Ok(()) => out.undo.push(UndoAction::RestoreTrashed { original: path.clone(), trashed_at_ms: at }),
                        Err(e) => out.fail(&src, e),
                    }
                    self.update(id, |s| s.files_done += 1);
                }
            }
            FileOperation::DeletePermanently { paths } => {
                self.update(id, |s| s.files_total = paths.len() as u64);
                for path in paths {
                    if p.cancelled() {
                        break;
                    }
                    let src = PathBuf::from(path);
                    self.update(id, |s| s.current_item = path.clone());
                    let r = match std::fs::symlink_metadata(&src) {
                        Ok(md) if md.is_dir() => std::fs::remove_dir_all(&src).at(&src),
                        Ok(_) => std::fs::remove_file(&src).at(&src),
                        Err(e) => Err(NexusError::io(&src, e)),
                    };
                    match r {
                        Ok(()) => out.results.push(path.clone()),
                        Err(e) => out.fail(&src, e),
                    }
                    self.update(id, |s| s.files_done += 1);
                }
            }
            FileOperation::NewFolder { parent, name } | FileOperation::NewFile { parent, name } => {
                let is_folder = matches!(op, FileOperation::NewFolder { .. });
                let dir = PathBuf::from(parent);
                let mut dest = dir.join(name);
                if fsops::exists(&dest) {
                    dest = fsops::unique_name(&dir, name, is_folder);
                }
                let r = if is_folder {
                    std::fs::create_dir(&dest).at(&dest)
                } else {
                    std::fs::OpenOptions::new().write(true).create_new(true).open(&dest).map(|_| ()).at(&dest)
                };
                match r {
                    Ok(()) => {
                        out.undo.push(UndoAction::RemoveCreated { path: dest.display().to_string() });
                        out.results.push(dest.display().to_string());
                    }
                    Err(e) => out.fail(&dest, e),
                }
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn transfer(&self, id: u64, sources: &[String], destination: &str, conflict: ConflictPolicy, is_move: bool, p: &JobProgress, out: &mut Outcome) {
        let dest_dir = PathBuf::from(destination);
        if !dest_dir.is_dir() {
            out.fail(&dest_dir, NexusError::NotFound { path: destination.to_string() });
            return;
        }
        let mut totals = Totals::default();
        for s in sources {
            fsops::measure(Path::new(s), &mut totals);
        }
        self.update(id, |s| {
            s.files_total = sources.len() as u64;
            s.bytes_total = totals.bytes;
        });
        for s in sources {
            if p.cancelled() {
                break;
            }
            let src = PathBuf::from(s);
            self.update(id, |st| st.current_item = s.clone());
            let r = self.transfer_one(&src, &dest_dir, conflict, is_move, p, out);
            if let Err(e) = r {
                if !matches!(e, NexusError::Cancelled) {
                    out.fail(&src, e);
                }
            }
            self.update(id, |st| st.files_done += 1);
        }
    }

    fn transfer_one(&self, src: &Path, dest_dir: &Path, conflict: ConflictPolicy, is_move: bool, p: &JobProgress, out: &mut Outcome) -> Result<()> {
        let md = std::fs::symlink_metadata(src).at(src)?;
        let name = src.file_name().ok_or_else(|| NexusError::invalid("không thể copy thư mục gốc"))?;
        let name_str = name.to_string_lossy();
        if md.is_dir() && fsops::is_within(dest_dir, src) {
            return Err(NexusError::invalid(format!("không thể chép \"{name_str}\" vào chính nó")));
        }
        let mut dest = dest_dir.join(name);
        let same_place = src.parent().is_some_and(|parent| fsops::is_within(parent, dest_dir) && fsops::is_within(dest_dir, parent));
        if is_move && same_place {
            out.skipped += 1;
            return Ok(());
        }
        if fsops::exists(&dest) {
            match conflict {
                ConflictPolicy::Skip => {
                    out.skipped += 1;
                    return Ok(());
                }
                ConflictPolicy::KeepBoth => dest = fsops::unique_name(dest_dir, &name_str, md.is_dir()),
                ConflictPolicy::Replace if same_place => dest = fsops::unique_name(dest_dir, &name_str, md.is_dir()),
                ConflictPolicy::Replace => {
                    let at = now_ms();
                    fsops::trash_one(&dest)?;
                    out.undo.push(UndoAction::RestoreTrashed { original: dest.display().to_string(), trashed_at_ms: at });
                }
            }
        }
        if is_move {
            fsops::move_item(src, &dest, p)?;
            out.undo.push(UndoAction::MoveBack { current: dest.display().to_string(), original: src.display().to_string() });
        } else {
            fsops::copy_item(src, &dest, p)?;
            out.undo.push(UndoAction::RemoveCreated { path: dest.display().to_string() });
        }
        out.results.push(dest.display().to_string());
        Ok(())
    }

    fn run_undo(&self, id: u64, entry_id: u64, p: &JobProgress, out: &mut Outcome) {
        let actions = {
            let j = self.journal.lock();
            match j.iter().find(|e| e.id == entry_id) {
                Some(e) => e.actions.clone(),
                None => return,
            }
        };
        self.update(id, |s| s.files_total = actions.len() as u64);
        let snapshot = if actions.iter().any(|a| matches!(a, UndoAction::RestoreTrashed { .. })) {
            fsops::TrashSnapshot::take()
        } else {
            fsops::TrashSnapshot::default()
        };
        for a in actions.iter().rev() {
            if p.cancelled() {
                break;
            }
            let r = match a {
                UndoAction::RemoveCreated { path } => {
                    let pth = Path::new(path);
                    if fsops::exists(pth) { fsops::trash_one(pth) } else { Ok(()) }
                }
                UndoAction::MoveBack { current, original } => {
                    let (cur, orig) = (Path::new(current), Path::new(original));
                    let case_only = current.to_lowercase() == original.to_lowercase();
                    if case_only {
                        std::fs::rename(cur, orig).at(cur)
                    } else {
                        fsops::move_item(cur, orig, p)
                    }
                    .map(|_| out.results.push(original.clone()))
                }
                UndoAction::RestoreTrashed { original, trashed_at_ms } => {
                    fsops::restore_trashed(Path::new(original), *trashed_at_ms, &snapshot).map(|_| out.results.push(original.clone()))
                }
            };
            if let Err(e) = r {
                let path = match a {
                    UndoAction::RemoveCreated { path } => path,
                    UndoAction::MoveBack { current, .. } => current,
                    UndoAction::RestoreTrashed { original, .. } => original,
                };
                out.fail(Path::new(path), e);
            }
            self.update(id, |s| s.files_done += 1);
        }
        if let Some(e) = self.journal.lock().iter_mut().find(|e| e.id == entry_id) {
            e.undone = true;
        }
        self.persist_journal();
    }

    fn record(&self, entry: JournalEntry) {
        {
            let mut j = self.journal.lock();
            j.push(entry);
            let excess = j.len().saturating_sub(JOURNAL_MAX_ENTRIES);
            j.drain(..excess);
        }
        self.persist_journal();
    }

    fn persist_journal(&self) {
        let snapshot = self.journal.lock().clone();
        let _ = self.store.save(JOURNAL_FILE, &snapshot);
    }

    fn prune_finished(&self) {
        let mut jobs = self.jobs.lock();
        let finished = jobs.iter().filter(|j| is_finished(j.status.state)).count();
        let mut to_drop = finished.saturating_sub(FINISHED_JOBS_KEPT);
        jobs.retain(|j| {
            if to_drop > 0 && is_finished(j.status.state) {
                to_drop -= 1;
                false
            } else {
                true
            }
        });
    }

    pub fn jobs(&self) -> Vec<JobStatus> {
        self.jobs.lock().iter().map(|j| j.status.clone()).collect()
    }

    pub fn job(&self, id: u64) -> Option<JobStatus> {
        self.jobs.lock().iter().find(|j| j.status.id == id).map(|j| j.status.clone())
    }

    pub fn cancel(&self, id: u64) {
        if let Some(j) = self.jobs.lock().iter().find(|j| j.status.id == id) {
            j.cancel.store(true, Ordering::Relaxed);
        }
    }

    pub fn clear_finished(&self) {
        self.jobs.lock().retain(|j| !is_finished(j.status.state));
    }

    /// Title of what Ctrl+Z would undo, e.g. "Copy 3 mục sang D:/Ảnh".
    pub fn undo_title(&self) -> Option<String> {
        self.journal.lock().iter().rev().find(|e| !e.undone).map(|e| e.title.clone())
    }

    /// Undoes the most recent operation that has not been undone yet.
    /// Returns the id of the undo job, or `None` when there is nothing to undo.
    pub fn undo_last(self: &Arc<Self>) -> Option<u64> {
        let (entry_id, title) = {
            let mut j = self.journal.lock();
            let e = j.iter_mut().rev().find(|e| !e.undone)?;
            // Mark immediately so a second Ctrl+Z goes to the previous entry.
            e.undone = true;
            (e.id, e.title.clone())
        };
        Some(self.start(JobKind::Undo { entry_id, title }))
    }

    pub fn history(&self, limit: u32) -> Vec<HistoryEntry> {
        self.journal
            .lock()
            .iter()
            .rev()
            .take(limit as usize)
            .map(|e| HistoryEntry { id: e.id, title: e.title.clone(), time_ms: e.time_ms, undone: e.undone })
            .collect()
    }
}

fn is_finished(s: JobState) -> bool {
    !matches!(s, JobState::Queued | JobState::Running)
}

fn validate(op: &FileOperation) -> Result<()> {
    let check_abs = |p: &str| normalize_path(p).map(|_| ());
    match op {
        FileOperation::Copy { sources, destination, .. } | FileOperation::Move { sources, destination, .. } => {
            if sources.is_empty() {
                return Err(NexusError::invalid("chưa chọn mục nào"));
            }
            sources.iter().try_for_each(|s| check_abs(s))?;
            check_abs(destination)
        }
        FileOperation::Trash { paths } | FileOperation::DeletePermanently { paths } => {
            if paths.is_empty() {
                return Err(NexusError::invalid("chưa chọn mục nào"));
            }
            paths.iter().try_for_each(|s| check_abs(s))
        }
        FileOperation::Rename { path, new_name } => {
            check_abs(path)?;
            fsops::validate_name(new_name)
        }
        FileOperation::NewFolder { parent, name } | FileOperation::NewFile { parent, name } => {
            check_abs(parent)?;
            fsops::validate_name(name)
        }
    }
}

fn file_name(p: &str) -> String {
    Path::new(p).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| p.to_string())
}

fn items(paths: &[String]) -> String {
    if paths.len() == 1 { format!("\"{}\"", file_name(&paths[0])) } else { format!("{} mục", paths.len()) }
}

fn title_of(kind: &JobKind) -> String {
    match kind {
        JobKind::Undo { title, .. } => format!("Hoàn tác: {title}"),
        JobKind::Op(op) => match op {
            FileOperation::Copy { sources, destination, .. } => format!("Copy {} sang {}", items(sources), destination),
            FileOperation::Move { sources, destination, .. } => format!("Di chuyển {} sang {}", items(sources), destination),
            FileOperation::Rename { path, new_name } => format!("Đổi tên \"{}\" thành \"{}\"", file_name(path), new_name),
            FileOperation::Trash { paths } => format!("Xóa {} vào Thùng rác", items(paths)),
            FileOperation::DeletePermanently { paths } => format!("Xóa vĩnh viễn {}", items(paths)),
            FileOperation::NewFolder { name, .. } => format!("Tạo thư mục \"{name}\""),
            FileOperation::NewFile { name, .. } => format!("Tạo tệp \"{name}\""),
        },
    }
}

#[cfg(test)]
mod tests;
