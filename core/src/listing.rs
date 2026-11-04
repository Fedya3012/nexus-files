//! Fast folder listing.
//!
//! The MVP gate is "10,000 files listed in under 50 ms". On Windows,
//! `DirEntry::metadata()` reuses the data returned by `FindNextFileW`, so no
//! extra syscall is made per file; on Linux it is one `statx` per entry.

use std::cmp::Ordering;
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use crate::error::{IoContext, NexusError, Result};
use crate::fold::fold;
use crate::kinds::classify;
use crate::model::*;

pub fn system_time_ms(t: std::io::Result<SystemTime>) -> i64 {
    t.ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

#[cfg(windows)]
fn hidden_flag(name: &str, md: &fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    const FILE_ATTRIBUTE_HIDDEN: u32 = 0x2;
    md.file_attributes() & FILE_ATTRIBUTE_HIDDEN != 0 || name.starts_with('.')
}

#[cfg(not(windows))]
fn hidden_flag(name: &str, _md: &fs::Metadata) -> bool {
    name.starts_with('.')
}

/// Builds a `FileEntry` from a name and metadata (symlink metadata is
/// followed for the size/kind so a link to a folder behaves like a folder).
pub fn entry_from(name: String, path: &Path, md: &fs::Metadata) -> FileEntry {
    let is_symlink = md.file_type().is_symlink();
    let target_md = if is_symlink { fs::metadata(path).ok() } else { None };
    let md_eff = target_md.as_ref().unwrap_or(md);
    let is_dir = md_eff.is_dir();
    let extension = if is_dir {
        String::new()
    } else {
        Path::new(&name)
            .extension()
            .map(|e| e.to_string_lossy().to_lowercase())
            .unwrap_or_default()
    };
    let (kind, type_label) = if is_dir {
        (FileKind::Folder, "Thư mục".to_string())
    } else {
        classify(&extension)
    };
    FileEntry {
        is_hidden: hidden_flag(&name, md),
        name,
        is_dir,
        is_symlink,
        size: if is_dir { 0 } else { md_eff.len() },
        modified_ms: system_time_ms(md_eff.modified()),
        created_ms: system_time_ms(md_eff.created()),
        extension,
        kind,
        type_label,
    }
}

pub fn list_directory(path: &str, opts: ListOptions) -> Result<DirListing> {
    let started = Instant::now();
    let dir = normalize_path(path)?;
    let read = fs::read_dir(&dir).at(&dir)?;
    let mut entries = Vec::with_capacity(256);
    let mut unreadable = 0u32;
    for item in read {
        let Ok(item) = item else {
            unreadable += 1;
            continue;
        };
        let name = item.file_name().to_string_lossy().into_owned();
        let Ok(md) = item.metadata() else {
            unreadable += 1;
            continue;
        };
        let entry = entry_from(name, &item.path(), &md);
        if entry.is_hidden && !opts.show_hidden {
            continue;
        }
        entries.push(entry);
    }
    sort_entries(&mut entries, opts.sort);
    Ok(DirListing {
        path: dir.display().to_string(),
        entries,
        unreadable_count: unreadable,
        elapsed_ms: started.elapsed().as_millis() as u32,
    })
}

/// Natural, accent-insensitive comparison: "file 2" < "file 10", "Ánh" ~ "anh".
pub fn natural_cmp(a: &str, b: &str) -> Ordering {
    natural_cmp_folded(&fold(a), &fold(b)).then_with(|| a.cmp(b))
}

/// Natural comparison of two already-folded strings.
fn natural_cmp_folded(fa: &str, fb: &str) -> Ordering {
    let (a, b) = (fa.as_bytes(), fb.as_bytes());
    let (mut i, mut j) = (0, 0);
    while i < a.len() && j < b.len() {
        if a[i].is_ascii_digit() && b[j].is_ascii_digit() {
            let si = i;
            while i < a.len() && a[i].is_ascii_digit() {
                i += 1;
            }
            let sj = j;
            while j < b.len() && b[j].is_ascii_digit() {
                j += 1;
            }
            let na = trim_zeros(&a[si..i]);
            let nb = trim_zeros(&b[sj..j]);
            let ord = na.len().cmp(&nb.len()).then_with(|| na.cmp(nb));
            if ord != Ordering::Equal {
                return ord;
            }
        } else {
            if a[i] != b[j] {
                // Byte order of UTF-8 equals char order.
                return a[i].cmp(&b[j]);
            }
            i += 1;
            j += 1;
        }
    }
    (a.len() - i).cmp(&(b.len() - j))
}

fn trim_zeros(d: &[u8]) -> &[u8] {
    let k = d.iter().position(|&c| c != b'0').unwrap_or(d.len());
    &d[k..]
}

pub fn sort_entries(entries: &mut Vec<FileEntry>, sort: SortOptions) {
    // Fold every name once (not once per comparison): this keeps sorting
    // 10,000 entries at a few milliseconds.
    let mut keyed: Vec<(String, String, FileEntry)> = entries
        .drain(..)
        .map(|e| {
            let label = if sort.key == SortKey::Type { fold(&e.type_label) } else { String::new() };
            (fold(&e.name), label, e)
        })
        .collect();
    keyed.sort_by(|(na, la, a), (nb, lb, b)| {
        if sort.folders_first && a.is_dir != b.is_dir {
            return if a.is_dir { Ordering::Less } else { Ordering::Greater };
        }
        let by_name = || natural_cmp_folded(na, nb).then_with(|| a.name.cmp(&b.name));
        let ord = match sort.key {
            SortKey::Name => by_name(),
            SortKey::Modified => a.modified_ms.cmp(&b.modified_ms).then_with(by_name),
            SortKey::Size => a.size.cmp(&b.size).then_with(by_name),
            SortKey::Type => natural_cmp_folded(la, lb).then_with(by_name),
        };
        if sort.descending { ord.reverse() } else { ord }
    });
    entries.extend(keyed.into_iter().map(|(_, _, e)| e));
}

/// Expands `~` and `%VAR%`/`$VAR`, makes the path absolute and removes `.`/`..`
/// without touching the disk (so it also works for paths that are gone).
pub fn normalize_path(input: &str) -> Result<PathBuf> {
    let trimmed = input.trim().trim_matches('"');
    if trimmed.is_empty() {
        return Err(NexusError::invalid("empty path"));
    }
    let expanded = expand_vars(trimmed);
    let mut p = PathBuf::from(&expanded);
    if expanded == "~" || expanded.starts_with("~/") || expanded.starts_with("~\\") {
        if let Some(home) = dirs::home_dir() {
            p = home.join(expanded[1..].trim_start_matches(['/', '\\']));
        }
    }
    // "C:" alone means the drive root for a file manager.
    #[cfg(windows)]
    if expanded.len() == 2 && expanded.ends_with(':') {
        p = PathBuf::from(format!("{expanded}\\"));
    }
    if p.is_relative() {
        return Err(NexusError::invalid(format!("not an absolute path: {trimmed}")));
    }
    let mut out = PathBuf::new();
    for c in p.components() {
        match c {
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            other => out.push(other.as_os_str()),
        }
    }
    Ok(out)
}

fn expand_vars(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(start) = rest.find('%') {
        let after = &rest[start + 1..];
        if let Some(end) = after.find('%') {
            let name = &after[..end];
            if let Ok(v) = std::env::var(name) {
                out.push_str(&rest[..start]);
                out.push_str(&v);
                rest = &after[end + 1..];
                continue;
            }
        }
        out.push_str(&rest[..=start]);
        rest = after;
    }
    out.push_str(rest);
    if let Some(stripped) = out.strip_prefix('$') {
        let (name, tail) = stripped.split_at(stripped.find(['/', '\\']).unwrap_or(stripped.len()));
        if let Ok(v) = std::env::var(name) {
            return format!("{v}{tail}");
        }
    }
    out
}

/// Splits a path into breadcrumb segments: `C:/Users/Fedya` →
/// [Windows (C:)…, Users, Fedya]. The drive label is added by the UI.
pub fn path_segments(path: &str) -> Result<Vec<PathSegment>> {
    let p = normalize_path(path)?;
    let mut acc = PathBuf::new();
    let mut out = Vec::new();
    for c in p.components() {
        acc.push(c.as_os_str());
        let name = match c {
            Component::Prefix(pre) => pre.as_os_str().to_string_lossy().into_owned(),
            Component::RootDir => {
                if out.is_empty() {
                    "/".to_string()
                } else {
                    // Windows: "C:" + "\" → keep a single "C:" crumb pointing at "C:\".
                    if let Some(last) = out.last_mut() {
                        let last: &mut PathSegment = last;
                        last.path = acc.display().to_string();
                    }
                    continue;
                }
            }
            other => other.as_os_str().to_string_lossy().into_owned(),
        };
        out.push(PathSegment { name, path: acc.display().to_string() });
    }
    Ok(out)
}

/// Parent folder, or `None` at a root.
pub fn parent_of(path: &str) -> Option<String> {
    let p = normalize_path(path).ok()?;
    p.parent().map(|x| x.display().to_string())
}

/// Number of direct children of each folder (`None` when unreadable). Used to
/// fill the "24 mục" size cell of folders lazily, after the listing is shown.
pub fn child_counts(dir: &str, names: &[String]) -> Vec<Option<u32>> {
    let base = PathBuf::from(dir);
    names
        .iter()
        .map(|n| fs::read_dir(base.join(n)).ok().map(|rd| rd.count() as u32))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn opts() -> ListOptions {
        ListOptions { show_hidden: false, sort: SortOptions::default() }
    }

    #[test]
    fn lists_and_sorts_naturally_with_folders_first() {
        let t = tempfile::tempdir().unwrap();
        for n in ["file 10.txt", "file 2.txt", "Ảnh.png", ".hidden"] {
            fs::write(t.path().join(n), b"x").unwrap();
        }
        fs::create_dir(t.path().join("zeta")).unwrap();
        let l = list_directory(t.path().to_str().unwrap(), opts()).unwrap();
        let names: Vec<_> = l.entries.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, ["zeta", "Ảnh.png", "file 2.txt", "file 10.txt"]);
        assert_eq!(l.entries[1].type_label, "Ảnh PNG");
        assert_eq!(l.entries[0].kind, FileKind::Folder);
    }

    #[test]
    fn shows_hidden_on_request() {
        let t = tempfile::tempdir().unwrap();
        fs::write(t.path().join(".env"), b"x").unwrap();
        let mut o = opts();
        o.show_hidden = true;
        assert_eq!(list_directory(t.path().to_str().unwrap(), o).unwrap().entries.len(), 1);
    }

    #[test]
    fn sorts_by_size_descending() {
        let t = tempfile::tempdir().unwrap();
        fs::write(t.path().join("a"), vec![0u8; 10]).unwrap();
        fs::write(t.path().join("b"), vec![0u8; 30]).unwrap();
        let mut o = opts();
        o.sort = SortOptions { key: SortKey::Size, descending: true, folders_first: true };
        let l = list_directory(t.path().to_str().unwrap(), o).unwrap();
        assert_eq!(l.entries[0].name, "b");
    }

    #[test]
    fn missing_folder_is_not_found() {
        let err = list_directory("/definitely/not/here", opts()).unwrap_err();
        assert!(matches!(err, NexusError::NotFound { .. }));
    }

    #[test]
    fn normalizes_dots_and_home() {
        let home = dirs::home_dir().unwrap();
        assert_eq!(normalize_path("~/a/./b/../c").unwrap(), home.join("a/c"));
        assert!(normalize_path("relative/path").is_err());
    }

    #[cfg(unix)]
    #[test]
    fn breadcrumbs_unix() {
        let s = path_segments("/home/fedya/Docs").unwrap();
        let names: Vec<_> = s.iter().map(|x| x.name.as_str()).collect();
        assert_eq!(names, ["/", "home", "fedya", "Docs"]);
        assert_eq!(s[3].path, "/home/fedya/Docs");
    }

    /// MVP gate: 10,000 files in under 50 ms. Debug builds are ~10x slower,
    /// so the strict budget is checked in release (`cargo test --release`).
    #[test]
    fn lists_ten_thousand_files_fast() {
        let t = tempfile::tempdir().unwrap();
        for i in 0..10_000 {
            fs::write(t.path().join(format!("Báo cáo {i}.xlsx")), b"").unwrap();
        }
        let p = t.path().to_str().unwrap();
        list_directory(p, opts()).unwrap(); // warm the OS cache like a real second visit
        let l = list_directory(p, opts()).unwrap();
        assert_eq!(l.entries.len(), 10_000);
        let budget = if cfg!(debug_assertions) { 1000 } else { 50 };
        assert!(l.elapsed_ms < budget, "listing took {} ms", l.elapsed_ms);
    }
}
