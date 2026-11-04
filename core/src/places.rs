//! Sidebar "Thư viện" (known folders) and "Ổ đĩa" (drives).

use crate::model::{DriveInfo, Place, PlaceKind};

pub fn known_folders() -> Vec<Place> {
    let mut out = Vec::new();
    let add = |out: &mut Vec<Place>, name: &str, dir: Option<std::path::PathBuf>, kind: PlaceKind| {
        if let Some(d) = dir.filter(|d| d.is_dir()) {
            out.push(Place { name: name.to_string(), path: d.display().to_string(), kind });
        }
    };
    add(&mut out, "Desktop", dirs::desktop_dir(), PlaceKind::Desktop);
    add(&mut out, "Documents", dirs::document_dir(), PlaceKind::Documents);
    add(&mut out, "Downloads", dirs::download_dir(), PlaceKind::Downloads);
    add(&mut out, "Pictures", dirs::picture_dir(), PlaceKind::Pictures);
    add(&mut out, "Music", dirs::audio_dir(), PlaceKind::Music);
    add(&mut out, "Videos", dirs::video_dir(), PlaceKind::Videos);
    if out.is_empty() {
        add(&mut out, "Home", dirs::home_dir(), PlaceKind::Home);
    }
    out
}

fn space(path: &str) -> (u64, u64) {
    (fs4::total_space(path).unwrap_or(0), fs4::available_space(path).unwrap_or(0))
}

#[cfg(windows)]
pub fn drives() -> Vec<DriveInfo> {
    use windows_sys::Win32::Storage::FileSystem::{GetDriveTypeW, GetLogicalDrives, GetVolumeInformationW};
    const DRIVE_NO_ROOT_DIR: u32 = 1;
    const DRIVE_FIXED: u32 = 3;
    const DRIVE_REMOVABLE: u32 = 2;
    const DRIVE_REMOTE: u32 = 4;
    const DRIVE_CDROM: u32 = 5;
    let mask = unsafe { GetLogicalDrives() };
    let mut out = Vec::new();
    for i in 0..26u32 {
        if mask & (1 << i) == 0 {
            continue;
        }
        let letter = (b'A' + i as u8) as char;
        let root = format!("{letter}:\\");
        let wide: Vec<u16> = root.encode_utf16().chain(std::iter::once(0)).collect();
        let kind = unsafe { GetDriveTypeW(wide.as_ptr()) };
        if kind == DRIVE_NO_ROOT_DIR {
            continue;
        }
        let mut label = [0u16; 261];
        let ok = unsafe {
            GetVolumeInformationW(
                wide.as_ptr(),
                label.as_mut_ptr(),
                label.len() as u32,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                0,
            )
        };
        // An empty card reader or DVD drive has no volume: skip it.
        if ok == 0 && (kind == DRIVE_REMOVABLE || kind == DRIVE_CDROM) {
            continue;
        }
        let len = label.iter().position(|&c| c == 0).unwrap_or(0);
        let mut name = String::from_utf16_lossy(&label[..len]);
        if name.is_empty() {
            name = match kind {
                DRIVE_FIXED => "Ổ cục bộ".into(),
                DRIVE_REMOVABLE => "USB".into(),
                DRIVE_REMOTE => "Ổ mạng".into(),
                _ => "Ổ đĩa".into(),
            };
        }
        let (total, free) = space(&root);
        out.push(DriveInfo { name: format!("{name} ({letter}:)"), path: root, total_bytes: total, free_bytes: free });
    }
    out
}

#[cfg(not(windows))]
pub fn drives() -> Vec<DriveInfo> {
    let mut out = Vec::new();
    let (t, f) = space("/");
    out.push(DriveInfo { name: "Hệ thống (/)".into(), path: "/".into(), total_bytes: t, free_bytes: f });
    if let Ok(mounts) = std::fs::read_to_string("/proc/mounts") {
        for line in mounts.lines() {
            let mut parts = line.split_whitespace();
            let (Some(dev), Some(mnt)) = (parts.next(), parts.next()) else { continue };
            let user_mount = mnt.starts_with("/media/") || mnt.starts_with("/mnt/") || mnt.starts_with("/run/media/");
            if dev.starts_with("/dev/") && user_mount {
                let mnt = mnt.replace("\\040", " ");
                let (t, f) = space(&mnt);
                let name = mnt.rsplit('/').next().unwrap_or(&mnt).to_string();
                out.push(DriveInfo { name, path: mnt, total_bytes: t, free_bytes: f });
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    #[test]
    fn has_at_least_one_drive() {
        let d = super::drives();
        assert!(!d.is_empty());
        assert!(d[0].total_bytes > 0);
    }
}
