//! Data for the Inspector ("Chi tiết" + "Xem trước"). The UI renders images,
//! PDF pages and media itself with built-in Windows components; the core
//! supplies metadata, text excerpts and folder summaries.

use std::fs;
use std::io::Read;
use std::path::Path;

use crate::error::{IoContext, Result};
use crate::kinds::is_textual;
use crate::listing::{entry_from, normalize_path};
use crate::model::{FileEntry, FileKind};

const TEXT_PREVIEW_BYTES: usize = 64 * 1024;

#[derive(Debug, Clone, PartialEq, uniffi::Enum)]
pub enum PreviewContent {
    /// Nothing to preview beyond the details.
    None,
    Text { text: String, truncated: bool },
    /// The UI loads the image; dimensions are read from the header only.
    Image { width: u32, height: u32 },
    Folder { folders: u32, files: u32, first_names: Vec<String> },
    Pdf,
    Media,
}

#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct PreviewInfo {
    pub path: String,
    pub entry: FileEntry,
    pub is_readonly: bool,
    pub content: PreviewContent,
}

pub fn preview(path: &str) -> Result<PreviewInfo> {
    let p = normalize_path(path)?;
    let md = fs::symlink_metadata(&p).at(&p)?;
    let name = p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| p.display().to_string());
    let entry = entry_from(name, &p, &md);
    let content = match entry.kind {
        FileKind::Folder => folder_summary(&p),
        FileKind::Image => match imagesize::size(&p) {
            Ok(sz) => PreviewContent::Image { width: sz.width as u32, height: sz.height as u32 },
            Err(_) => PreviewContent::Image { width: 0, height: 0 },
        },
        FileKind::Pdf => PreviewContent::Pdf,
        FileKind::Video | FileKind::Audio => PreviewContent::Media,
        _ if is_textual(&entry.extension) || (entry.extension.is_empty() && entry.size < 1_000_000) => text_excerpt(&p),
        _ => PreviewContent::None,
    };
    Ok(PreviewInfo { path: p.display().to_string(), is_readonly: md.permissions().readonly(), entry, content })
}

fn folder_summary(p: &Path) -> PreviewContent {
    let (mut folders, mut files, mut names) = (0u32, 0u32, Vec::new());
    if let Ok(rd) = fs::read_dir(p) {
        for e in rd.flatten() {
            if e.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                folders += 1;
            } else {
                files += 1;
            }
            if names.len() < 12 {
                names.push(e.file_name().to_string_lossy().into_owned());
            }
        }
    }
    names.sort_by(|a, b| crate::listing::natural_cmp(a, b));
    PreviewContent::Folder { folders, files, first_names: names }
}

fn text_excerpt(p: &Path) -> PreviewContent {
    let Ok(f) = fs::File::open(p) else { return PreviewContent::None };
    let mut buf = Vec::with_capacity(TEXT_PREVIEW_BYTES);
    if f.take(TEXT_PREVIEW_BYTES as u64 + 1).read_to_end(&mut buf).is_err() {
        return PreviewContent::None;
    }
    let truncated = buf.len() > TEXT_PREVIEW_BYTES;
    buf.truncate(TEXT_PREVIEW_BYTES);
    if buf.contains(&0) {
        return PreviewContent::None; // binary
    }
    // Drop a UTF-8 BOM and any multi-byte char cut at the end.
    let start = if buf.starts_with(&[0xEF, 0xBB, 0xBF]) { 3 } else { 0 };
    let text = match std::str::from_utf8(&buf[start..]) {
        Ok(s) => s.to_string(),
        Err(e) if e.error_len().is_none() => String::from_utf8_lossy(&buf[start..start + e.valid_up_to()]).into_owned(),
        Err(_) => String::from_utf8_lossy(&buf[start..]).into_owned(),
    };
    PreviewContent::Text { text, truncated }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_folder_and_binary() {
        let t = tempfile::tempdir().unwrap();
        fs::write(t.path().join("a.md"), "# Tiêu đề\nnội dung").unwrap();
        fs::write(t.path().join("b.bin"), [0u8, 1, 2]).unwrap();
        let a = preview(t.path().join("a.md").to_str().unwrap()).unwrap();
        assert_eq!(a.content, PreviewContent::Text { text: "# Tiêu đề\nnội dung".into(), truncated: false });
        assert_eq!(preview(t.path().join("b.bin").to_str().unwrap()).unwrap().content, PreviewContent::None);
        match preview(t.path().to_str().unwrap()).unwrap().content {
            PreviewContent::Folder { files, .. } => assert_eq!(files, 2),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn png_dimensions() {
        let t = tempfile::tempdir().unwrap();
        // 1x1 PNG
        let png: &[u8] = &[
            0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0, 0, 0, 0x0D, 0x49, 0x48, 0x44, 0x52, 0, 0, 0, 3, 0, 0, 0, 2, 8, 6, 0, 0, 0, 0, 0, 0, 0,
        ];
        fs::write(t.path().join("x.png"), png).unwrap();
        let p = preview(t.path().join("x.png").to_str().unwrap()).unwrap();
        assert_eq!(p.content, PreviewContent::Image { width: 3, height: 2 });
    }
}
