//! Plain data types that cross the FFI boundary.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, uniffi::Enum)]
pub enum FileKind {
    Folder,
    Image,
    Video,
    Audio,
    Pdf,
    Document,
    Spreadsheet,
    Presentation,
    Archive,
    Code,
    Text,
    Executable,
    Shortcut,
    Font,
    Other,
}

/// One row in a folder listing. The full path is `DirListing.path` + `name`;
/// it is not repeated per entry to keep the FFI payload small.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct FileEntry {
    pub name: String,
    pub is_dir: bool,
    pub is_hidden: bool,
    pub is_symlink: bool,
    /// Bytes; 0 for folders (folder sizes come later, with the index).
    pub size: u64,
    /// Unix epoch milliseconds, 0 when unknown.
    pub modified_ms: i64,
    pub created_ms: i64,
    /// Lowercase extension without the dot ("pdf"), empty for folders.
    pub extension: String,
    pub kind: FileKind,
    /// Short label for the "Loại" column ("Excel", "Ảnh PNG", "Thư mục").
    pub type_label: String,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct DirListing {
    /// Normalised absolute path of the folder.
    pub path: String,
    pub entries: Vec<FileEntry>,
    /// Entries skipped because their metadata could not be read.
    pub unreadable_count: u32,
    pub elapsed_ms: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, uniffi::Enum)]
pub enum SortKey {
    Name,
    Modified,
    Size,
    Type,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, uniffi::Record)]
pub struct SortOptions {
    pub key: SortKey,
    pub descending: bool,
    pub folders_first: bool,
}

impl Default for SortOptions {
    fn default() -> Self {
        SortOptions { key: SortKey::Name, descending: false, folders_first: true }
    }
}

#[derive(Debug, Clone, Copy, uniffi::Record)]
pub struct ListOptions {
    pub show_hidden: bool,
    pub sort: SortOptions,
}

/// A span of UTF-16 code units to highlight in a result title.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Record)]
pub struct TextRange {
    pub start: u32,
    pub length: u32,
}

/// One breadcrumb segment of the address bar.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct PathSegment {
    pub name: String,
    pub path: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum PlaceKind {
    Home,
    Desktop,
    Documents,
    Downloads,
    Pictures,
    Music,
    Videos,
    Drive,
    Pinned,
    Recent,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct Place {
    pub name: String,
    pub path: String,
    pub kind: PlaceKind,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct DriveInfo {
    pub name: String,
    pub path: String,
    pub total_bytes: u64,
    pub free_bytes: u64,
}
