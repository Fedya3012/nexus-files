//! Nexus Files shared core.
//!
//! Everything platform-neutral lives here and is exposed to the Windows UI
//! (C#, via uniffi-bindgen-cs) and later Android (Kotlin) through UniFFI.

uniffi::setup_scaffolding!();

pub mod commands;
pub mod core;
pub mod error;
pub mod fold;
pub mod fuzzy;
pub mod kinds;
pub mod listing;
pub mod model;
pub mod ops;
pub mod places;
pub mod preview;
pub mod query;
pub mod search;
pub mod session;
pub mod store;

use error::Result;
use model::*;

/// Lists a folder (sorted, hidden files filtered by `options`).
#[uniffi::export]
pub fn list_directory(path: String, options: ListOptions) -> Result<DirListing> {
    listing::list_directory(&path, options)
}

/// Re-sorts entries already on screen (when the user clicks a column).
#[uniffi::export]
pub fn sort_entries(mut entries: Vec<FileEntry>, sort: SortOptions) -> Vec<FileEntry> {
    listing::sort_entries(&mut entries, sort);
    entries
}

#[uniffi::export]
pub fn path_segments(path: String) -> Result<Vec<PathSegment>> {
    listing::path_segments(&path)
}

/// Expands `~`/`%VAR%` and resolves `.`/`..`; errors on relative paths.
#[uniffi::export]
pub fn normalize_path(path: String) -> Result<String> {
    listing::normalize_path(&path).map(|p| p.display().to_string())
}

#[uniffi::export]
pub fn parent_path(path: String) -> Option<String> {
    listing::parent_of(&path)
}

#[uniffi::export]
pub fn child_counts(dir: String, names: Vec<String>) -> Vec<Option<u32>> {
    listing::child_counts(&dir, &names)
}

#[uniffi::export]
pub fn known_folders() -> Vec<Place> {
    places::known_folders()
}

#[uniffi::export]
pub fn drives() -> Vec<DriveInfo> {
    places::drives()
}

#[uniffi::export]
pub fn preview(path: String) -> Result<preview::PreviewInfo> {
    preview::preview(&path)
}

#[uniffi::export]
pub fn all_commands() -> Vec<commands::CommandInfo> {
    commands::all_commands()
}

/// Parses an Omnibar query without running it (for the filter chips).
#[uniffi::export]
pub fn parse_query(input: String) -> query::ParsedQuery {
    query::parse(&input).public
}

/// Deletes `*.nexus-partial` leftovers in a folder after a crash.
#[uniffi::export]
pub fn cleanup_partials(dir: String) -> u32 {
    ops::fsops::cleanup_partials(std::path::Path::new(&dir))
}

#[uniffi::export]
pub fn core_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}
