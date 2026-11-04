//! FFI-facing types for file operations.

use serde::{Deserialize, Serialize};

/// What to do when an item with the same name already exists at the target.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, uniffi::Enum)]
pub enum ConflictPolicy {
    /// Keep both: the new item becomes "name (2).ext".
    KeepBoth,
    /// Replace: the existing item goes to the Recycle Bin first (so even a
    /// replace can be undone), then the new item takes its place.
    Replace,
    /// Leave the existing item alone and skip this one.
    Skip,
}

#[derive(Debug, Clone, PartialEq, uniffi::Enum)]
pub enum FileOperation {
    Copy { sources: Vec<String>, destination: String, conflict: ConflictPolicy },
    Move { sources: Vec<String>, destination: String, conflict: ConflictPolicy },
    Rename { path: String, new_name: String },
    /// Send to the Recycle Bin (undoable).
    Trash { paths: Vec<String> },
    /// Delete for good (not undoable; the UI must confirm first).
    DeletePermanently { paths: Vec<String> },
    NewFolder { parent: String, name: String },
    NewFile { parent: String, name: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum JobState {
    Queued,
    Running,
    Completed,
    /// Finished, but some items failed or were skipped (see `errors`).
    CompletedWithErrors,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct ItemError {
    pub path: String,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct JobStatus {
    pub id: u64,
    /// "Copy 3 mục sang D:/Ảnh"
    pub title: String,
    pub state: JobState,
    pub files_done: u64,
    pub files_total: u64,
    pub bytes_done: u64,
    pub bytes_total: u64,
    pub current_item: String,
    pub errors: Vec<ItemError>,
    pub skipped: u32,
    /// Paths created or moved into place, so the UI can select them.
    pub result_paths: Vec<String>,
    /// True when this job added an entry to the undo history.
    pub undoable: bool,
}

/// One entry of the undo history ("Lịch sử thao tác").
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct HistoryEntry {
    pub id: u64,
    pub title: String,
    pub time_ms: i64,
    pub undone: bool,
}

/// Inverse steps recorded while a job runs. Applied in reverse order.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum UndoAction {
    /// The job created this path: undo sends it to the Recycle Bin.
    RemoveCreated { path: String },
    /// The job moved/renamed `original` to `current`: undo moves it back.
    MoveBack { current: String, original: String },
    /// The job sent `original` to the Recycle Bin: undo restores it.
    RestoreTrashed { original: String, trashed_at_ms: i64 },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct JournalEntry {
    pub id: u64,
    pub time_ms: i64,
    pub title: String,
    pub actions: Vec<UndoAction>,
    pub undone: bool,
}
