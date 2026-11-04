//! Error type shared by every exported function.

use std::io;
use std::path::Path;

/// Errors surfaced to the UI. Variants carry the path so the UI can show a
/// precise message ("Không có quyền truy cập ...") without parsing strings.
#[derive(Debug, thiserror::Error, uniffi::Error)]
pub enum NexusError {
    #[error("not found: {path}")]
    NotFound { path: String },
    #[error("permission denied: {path}")]
    PermissionDenied { path: String },
    #[error("already exists: {path}")]
    AlreadyExists { path: String },
    #[error("invalid input: {message}")]
    Invalid { message: String },
    #[error("cancelled")]
    Cancelled,
    #[error("I/O error on {path}: {message}")]
    Io { path: String, message: String },
}

pub type Result<T, E = NexusError> = std::result::Result<T, E>;

impl NexusError {
    pub fn io(path: &Path, err: io::Error) -> Self {
        let path = path.display().to_string();
        match err.kind() {
            io::ErrorKind::NotFound => NexusError::NotFound { path },
            io::ErrorKind::PermissionDenied => NexusError::PermissionDenied { path },
            io::ErrorKind::AlreadyExists => NexusError::AlreadyExists { path },
            _ => NexusError::Io { path, message: err.to_string() },
        }
    }

    pub fn invalid(message: impl Into<String>) -> Self {
        NexusError::Invalid { message: message.into() }
    }
}

/// Small helper so call sites can write `fs::metadata(p).at(p)?`.
pub trait IoContext<T> {
    fn at(self, path: &Path) -> Result<T>;
}

impl<T> IoContext<T> for io::Result<T> {
    fn at(self, path: &Path) -> Result<T> {
        self.map_err(|e| NexusError::io(path, e))
    }
}
