//! Small persistent state kept as JSON files in the app data folder
//! (`%LOCALAPPDATA%\NexusFiles` on Windows). Every write goes to a temp file
//! first and is then renamed over the old one, so a crash never leaves a
//! half-written file. The MVP data is small (pins, visit counts, session,
//! undo journal); SQLite arrives with the search index in the Beta.

use std::fs;
use std::path::{Path, PathBuf};

use serde::de::DeserializeOwned;
use serde::Serialize;

use crate::error::{IoContext, Result};

#[derive(Debug, Clone)]
pub struct JsonStore {
    dir: PathBuf,
}

impl JsonStore {
    pub fn open(dir: &Path) -> Result<Self> {
        fs::create_dir_all(dir).at(dir)?;
        Ok(JsonStore { dir: dir.to_path_buf() })
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    fn file(&self, name: &str) -> PathBuf {
        self.dir.join(format!("{name}.json"))
    }

    /// Loads a value, falling back to the default when the file is missing or
    /// unreadable. A corrupt file is kept aside as `.bad` for inspection.
    pub fn load<T: DeserializeOwned + Default>(&self, name: &str) -> T {
        let path = self.file(name);
        match fs::read(&path) {
            Ok(bytes) => match serde_json::from_slice(&bytes) {
                Ok(v) => v,
                Err(_) => {
                    let _ = fs::rename(&path, path.with_extension("json.bad"));
                    T::default()
                }
            },
            Err(_) => T::default(),
        }
    }

    pub fn save<T: Serialize>(&self, name: &str, value: &T) -> Result<()> {
        let path = self.file(name);
        let tmp = path.with_extension("json.tmp");
        let bytes = serde_json::to_vec_pretty(value).map_err(|e| crate::error::NexusError::invalid(e.to_string()))?;
        fs::write(&tmp, bytes).at(&tmp)?;
        fs::rename(&tmp, &path).at(&path)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_and_corrupt_fallback() {
        let t = tempfile::tempdir().unwrap();
        let s = JsonStore::open(t.path()).unwrap();
        s.save("x", &vec![1, 2, 3]).unwrap();
        assert_eq!(s.load::<Vec<i32>>("x"), vec![1, 2, 3]);
        fs::write(t.path().join("x.json"), b"{not json").unwrap();
        assert!(s.load::<Vec<i32>>("x").is_empty());
        assert!(t.path().join("x.json.bad").exists());
    }
}
