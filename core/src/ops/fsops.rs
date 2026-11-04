//! Low-level, crash-safe file primitives with progress and cancellation.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use crate::error::{IoContext, NexusError, Result};

pub const PARTIAL_SUFFIX: &str = ".nexus-partial";

/// Progress sink: called with the number of new bytes copied.
pub trait Progress {
    fn add_bytes(&self, n: u64);
    fn cancelled(&self) -> bool;
}

pub struct NoProgress;
impl Progress for NoProgress {
    fn add_bytes(&self, _: u64) {}
    fn cancelled(&self) -> bool {
        false
    }
}

impl Progress for AtomicBool {
    fn add_bytes(&self, _: u64) {}
    fn cancelled(&self) -> bool {
        self.load(Ordering::Relaxed)
    }
}

/// Totals for the progress bar, gathered before a copy/move starts.
#[derive(Debug, Default, Clone, Copy)]
pub struct Totals {
    pub files: u64,
    pub bytes: u64,
}

pub fn measure(path: &Path, acc: &mut Totals) {
    let Ok(md) = fs::symlink_metadata(path) else { return };
    if md.is_dir() {
        if let Ok(rd) = fs::read_dir(path) {
            for e in rd.flatten() {
                measure(&e.path(), acc);
            }
        }
    } else {
        acc.files += 1;
        acc.bytes += md.len();
    }
}

/// "a.txt" → "a (2).txt", "a (2).txt" → "a (3).txt", "Folder" → "Folder (2)".
pub fn unique_name(dir: &Path, name: &str, is_dir: bool) -> PathBuf {
    let (stem, ext) = match (is_dir, name.rfind('.')) {
        (false, Some(i)) if i > 0 => (&name[..i], &name[i..]),
        _ => (name, ""),
    };
    let base = strip_counter(stem);
    let mut n = 2;
    loop {
        let candidate = dir.join(format!("{base} ({n}){ext}"));
        if fs::symlink_metadata(&candidate).is_err() {
            return candidate;
        }
        n += 1;
    }
}

fn strip_counter(stem: &str) -> &str {
    if let Some(open) = stem.rfind(" (") {
        let inner = &stem[open + 2..];
        if inner.ends_with(')') && inner[..inner.len() - 1].chars().all(|c| c.is_ascii_digit()) && inner.len() > 1 {
            return &stem[..open];
        }
    }
    stem
}

/// Rejects names Windows cannot store (also enforced on other platforms so a
/// folder stays portable when synced back to a PC).
pub fn validate_name(name: &str) -> Result<()> {
    let trimmed = name.trim();
    if trimmed.is_empty() || trimmed == "." || trimmed == ".." {
        return Err(NexusError::invalid("Tên không được để trống"));
    }
    if let Some(c) = name.chars().find(|c| matches!(c, '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*') || (*c as u32) < 32) {
        return Err(NexusError::invalid(format!("Tên không được chứa ký tự {c:?}")));
    }
    if name.ends_with('.') || name.ends_with(' ') {
        return Err(NexusError::invalid("Tên không được kết thúc bằng dấu chấm hoặc khoảng trắng"));
    }
    let stem = name.split('.').next().unwrap_or("").to_ascii_uppercase();
    const RESERVED: &[&str] = &["CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8", "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9"];
    if RESERVED.contains(&stem.as_str()) {
        return Err(NexusError::invalid(format!("\"{name}\" là tên dành riêng của Windows")));
    }
    Ok(())
}

pub fn exists(p: &Path) -> bool {
    fs::symlink_metadata(p).is_ok()
}

/// True if `inner` is `outer` or lies inside it.
pub fn is_within(inner: &Path, outer: &Path) -> bool {
    let canon = |p: &Path| fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf());
    canon(inner).starts_with(canon(outer))
}

fn is_cross_device(e: &io::Error) -> bool {
    e.kind() == io::ErrorKind::CrossesDevices || matches!(e.raw_os_error(), Some(17) if cfg!(windows)) || matches!(e.raw_os_error(), Some(18) if cfg!(unix))
}

fn partial_path(dest: &Path) -> PathBuf {
    let mut s = dest.as_os_str().to_owned();
    s.push(PARTIAL_SUFFIX);
    PathBuf::from(s)
}

/// Copies a file or a whole folder to `dest` (which must not exist). The data
/// is first written to `dest.nexus-partial` and renamed when complete, so an
/// interruption never leaves a truncated file under the real name.
pub fn copy_item(src: &Path, dest: &Path, p: &dyn Progress) -> Result<()> {
    if exists(dest) {
        return Err(NexusError::AlreadyExists { path: dest.display().to_string() });
    }
    let tmp = partial_path(dest);
    remove_any(&tmp);
    let result = copy_any(src, &tmp, p).and_then(|_| fs::rename(&tmp, dest).at(dest));
    if result.is_err() {
        remove_any(&tmp);
    }
    result
}

fn copy_any(src: &Path, dest: &Path, p: &dyn Progress) -> Result<()> {
    let md = fs::symlink_metadata(src).at(src)?;
    if md.is_dir() {
        fs::create_dir(dest).at(dest)?;
        for e in fs::read_dir(src).at(src)? {
            if p.cancelled() {
                return Err(NexusError::Cancelled);
            }
            let e = e.at(src)?;
            copy_any(&e.path(), &dest.join(e.file_name()), p)?;
        }
        if let Ok(m) = fs::metadata(src) {
            let _ = filetime::set_file_mtime(dest, filetime::FileTime::from_last_modification_time(&m));
        }
        Ok(())
    } else {
        copy_file(src, dest, p)
    }
}

#[cfg(windows)]
fn copy_file(src: &Path, dest: &Path, p: &dyn Progress) -> Result<()> {
    // CopyFileExW keeps timestamps, attributes and alternate data streams
    // (including Zone.Identifier, so Windows still warns about downloads).
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::{CopyFileExW, COPY_FILE_FAIL_IF_EXISTS, PROGRESS_CANCEL, PROGRESS_CONTINUE};

    struct Ctx<'a> {
        p: &'a dyn Progress,
        last: i64,
    }
    unsafe extern "system" fn cb(
        _total: i64,
        transferred: i64,
        _ss: i64,
        _sbt: i64,
        _stream: u32,
        _reason: u32,
        _h1: windows_sys::Win32::Foundation::HANDLE,
        _h2: windows_sys::Win32::Foundation::HANDLE,
        data: *const core::ffi::c_void,
    ) -> u32 {
        let ctx = &mut *(data as *mut Ctx);
        if transferred > ctx.last {
            ctx.p.add_bytes((transferred - ctx.last) as u64);
            ctx.last = transferred;
        }
        if ctx.p.cancelled() { PROGRESS_CANCEL } else { PROGRESS_CONTINUE }
    }
    let w = |x: &Path| x.as_os_str().encode_wide().chain(std::iter::once(0)).collect::<Vec<u16>>();
    let (ws, wd) = (w(src), w(dest));
    let mut ctx = Ctx { p, last: 0 };
    let ok = unsafe {
        CopyFileExW(ws.as_ptr(), wd.as_ptr(), Some(cb), &mut ctx as *mut Ctx as *const core::ffi::c_void, std::ptr::null_mut(), COPY_FILE_FAIL_IF_EXISTS)
    };
    if ok == 0 {
        if p.cancelled() {
            return Err(NexusError::Cancelled);
        }
        return Err(NexusError::io(src, io::Error::last_os_error()));
    }
    Ok(())
}

#[cfg(not(windows))]
fn copy_file(src: &Path, dest: &Path, p: &dyn Progress) -> Result<()> {
    use std::io::{Read, Write};
    let mut input = fs::File::open(src).at(src)?;
    let md = input.metadata().at(src)?;
    let mut output = fs::OpenOptions::new().write(true).create_new(true).open(dest).at(dest)?;
    let mut buf = vec![0u8; 1 << 20];
    loop {
        if p.cancelled() {
            return Err(NexusError::Cancelled);
        }
        let n = input.read(&mut buf).at(src)?;
        if n == 0 {
            break;
        }
        output.write_all(&buf[..n]).at(dest)?;
        p.add_bytes(n as u64);
    }
    output.sync_all().at(dest)?;
    drop(output);
    let _ = fs::set_permissions(dest, md.permissions());
    let _ = filetime::set_file_times(
        dest,
        filetime::FileTime::from_last_access_time(&md),
        filetime::FileTime::from_last_modification_time(&md),
    );
    Ok(())
}

/// Moves `src` to `dest` (which must not exist). Same volume: an instant
/// rename. Different volume: copy (via `.nexus-partial`), verify the size,
/// and only then delete the source, so a power cut keeps the source intact.
pub fn move_item(src: &Path, dest: &Path, p: &dyn Progress) -> Result<()> {
    if exists(dest) {
        return Err(NexusError::AlreadyExists { path: dest.display().to_string() });
    }
    match fs::rename(src, dest) {
        Ok(()) => {
            let mut t = Totals::default();
            measure(dest, &mut t);
            p.add_bytes(t.bytes);
            Ok(())
        }
        Err(e) if is_cross_device(&e) => move_across_volumes(src, dest, p),
        Err(e) => Err(NexusError::io(src, e)),
    }
}

pub(crate) fn move_across_volumes(src: &Path, dest: &Path, p: &dyn Progress) -> Result<()> {
    let mut before = Totals::default();
    measure(src, &mut before);
    copy_item(src, dest, p)?;
    let mut after = Totals::default();
    measure(dest, &mut after);
    if before.bytes != after.bytes || before.files != after.files {
        remove_any(dest);
        return Err(NexusError::Io {
            path: dest.display().to_string(),
            message: "bản sao không khớp kích thước, đã giữ nguyên bản gốc".into(),
        });
    }
    let md = fs::symlink_metadata(src).at(src)?;
    if md.is_dir() { fs::remove_dir_all(src).at(src) } else { fs::remove_file(src).at(src) }
}

pub fn remove_any(p: &Path) {
    match fs::symlink_metadata(p) {
        Ok(md) if md.is_dir() => {
            let _ = fs::remove_dir_all(p);
        }
        Ok(_) => {
            let _ = fs::remove_file(p);
        }
        Err(_) => {}
    }
}

/// Ids of the items in the trash at one moment.
#[derive(Default)]
pub struct TrashSnapshot(std::collections::HashSet<std::ffi::OsString>);

impl TrashSnapshot {
    #[cfg(any(windows, all(unix, not(target_os = "macos"), not(target_os = "ios"), not(target_os = "android"))))]
    pub fn take() -> Self {
        TrashSnapshot(trash::os_limited::list().map(|v| v.into_iter().map(|i| i.id).collect()).unwrap_or_default())
    }

    #[cfg(not(any(windows, all(unix, not(target_os = "macos"), not(target_os = "ios"), not(target_os = "android")))))]
    pub fn take() -> Self {
        TrashSnapshot::default()
    }

    #[allow(dead_code)]
    fn contains(&self, id: &std::ffi::OsStr) -> bool {
        self.0.contains(id)
    }
}

pub fn trash_one(p: &Path) -> Result<()> {
    trash::delete(p).map_err(|e| NexusError::Io { path: p.display().to_string(), message: e.to_string() })
}

#[cfg(any(windows, all(unix, not(target_os = "macos"), not(target_os = "ios"), not(target_os = "android"))))]
pub fn restore_trashed(original: &Path, trashed_at_ms: i64, snapshot: &TrashSnapshot) -> Result<()> {
    let err = |m: String| NexusError::Io { path: original.display().to_string(), message: m };
    if exists(original) {
        return Err(NexusError::AlreadyExists { path: original.display().to_string() });
    }
    let name = original.file_name().ok_or_else(|| err("tên không hợp lệ".into()))?;
    let parent = original.parent().ok_or_else(|| err("không có thư mục cha".into()))?;
    let items = trash::os_limited::list().map_err(|e| err(e.to_string()))?;
    let since = trashed_at_ms / 1000 - 60;
    let item = items
        .into_iter()
        // Only items that were already in the trash when the undo began: an undo that
        // first trashes a newer copy with the same name must not restore that copy.
        .filter(|i| snapshot.contains(&i.id))
        .filter(|i| i.name == name && same_dir(&i.original_parent, parent) && i.time_deleted >= since)
        .max_by_key(|i| i.time_deleted)
        .ok_or_else(|| err("không còn trong Thùng rác".into()))?;
    trash::os_limited::restore_all([item]).map_err(|e| err(e.to_string()))
}

#[cfg(not(any(windows, all(unix, not(target_os = "macos"), not(target_os = "ios"), not(target_os = "android")))))]
pub fn restore_trashed(original: &Path, _trashed_at_ms: i64, _snapshot: &TrashSnapshot) -> Result<()> {
    Err(NexusError::Io { path: original.display().to_string(), message: "khôi phục từ Thùng rác chưa hỗ trợ trên nền tảng này".into() })
}

fn same_dir(a: &Path, b: &Path) -> bool {
    if cfg!(windows) {
        a.to_string_lossy().to_lowercase().trim_end_matches('\\') == b.to_string_lossy().to_lowercase().trim_end_matches('\\')
    } else {
        a == b
    }
}

/// Removes leftovers of copies that were interrupted by a crash or power cut.
pub fn cleanup_partials(dir: &Path) -> u32 {
    let mut n = 0;
    if let Ok(rd) = fs::read_dir(dir) {
        for e in rd.flatten() {
            if e.file_name().to_string_lossy().ends_with(PARTIAL_SUFFIX) {
                remove_any(&e.path());
                n += 1;
            }
        }
    }
    n
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unique_names_like_explorer() {
        let t = tempfile::tempdir().unwrap();
        fs::write(t.path().join("a.txt"), b"").unwrap();
        assert_eq!(unique_name(t.path(), "a.txt", false), t.path().join("a (2).txt"));
        fs::write(t.path().join("a (2).txt"), b"").unwrap();
        assert_eq!(unique_name(t.path(), "a (2).txt", false), t.path().join("a (3).txt"));
        assert_eq!(unique_name(t.path(), "Folder.v2", true), t.path().join("Folder.v2 (2)"));
        assert_eq!(unique_name(t.path(), ".env", false), t.path().join(".env (2)"));
    }

    #[test]
    fn name_validation() {
        assert!(validate_name("Báo cáo.xlsx").is_ok());
        assert!(validate_name("a/b").is_err());
        assert!(validate_name("con.txt").is_err());
        assert!(validate_name("x.").is_err());
        assert!(validate_name("  ").is_err());
    }

    #[test]
    fn cross_volume_move_copies_then_deletes_source() {
        let t = tempfile::tempdir().unwrap();
        let src = t.path().join("dir");
        fs::create_dir_all(src.join("sub")).unwrap();
        fs::write(src.join("sub/f.bin"), vec![7u8; 3 * 1024 * 1024]).unwrap();
        let dest = t.path().join("moved");
        move_across_volumes(&src, &dest, &NoProgress).unwrap();
        assert!(!src.exists());
        assert_eq!(fs::read(dest.join("sub/f.bin")).unwrap().len(), 3 * 1024 * 1024);
        assert!(!partial_path(&dest).exists());
    }

    #[test]
    fn cancelled_copy_leaves_no_partial_and_keeps_source() {
        let t = tempfile::tempdir().unwrap();
        let src = t.path().join("big.bin");
        fs::write(&src, vec![1u8; 4 * 1024 * 1024]).unwrap();
        let dest = t.path().join("copy.bin");
        let cancel = AtomicBool::new(true);
        assert!(matches!(copy_item(&src, &dest, &cancel), Err(NexusError::Cancelled)));
        assert!(!dest.exists());
        assert!(!partial_path(&dest).exists());
        assert!(src.exists());
    }

    #[test]
    fn copy_preserves_mtime() {
        let t = tempfile::tempdir().unwrap();
        let src = t.path().join("a.txt");
        fs::write(&src, b"hello").unwrap();
        let old = filetime::FileTime::from_unix_time(1_600_000_000, 0);
        filetime::set_file_mtime(&src, old).unwrap();
        let dest = t.path().join("b.txt");
        copy_item(&src, &dest, &NoProgress).unwrap();
        let m = fs::metadata(&dest).unwrap();
        assert_eq!(filetime::FileTime::from_last_modification_time(&m), old);
    }
}
