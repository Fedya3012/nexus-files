//! File type classification from the extension.

use crate::model::FileKind;

/// Returns the kind and a short Vietnamese label ("Excel", "Ảnh PNG", ...),
/// matching the "Loại" column of the mockup.
pub fn classify(ext_lower: &str) -> (FileKind, String) {
    use FileKind::*;
    let (kind, label): (FileKind, &str) = match ext_lower {
        "jpg" | "jpeg" | "jfif" => (Image, "Ảnh JPEG"),
        "png" => (Image, "Ảnh PNG"),
        "gif" => (Image, "Ảnh GIF"),
        "webp" => (Image, "Ảnh WebP"),
        "heic" | "heif" => (Image, "Ảnh HEIC"),
        "bmp" => (Image, "Ảnh BMP"),
        "tif" | "tiff" => (Image, "Ảnh TIFF"),
        "svg" => (Image, "Ảnh SVG"),
        "ico" => (Image, "Biểu tượng"),
        "raw" | "cr2" | "cr3" | "nef" | "arw" | "dng" | "raf" => (Image, "Ảnh RAW"),
        "psd" => (Image, "Photoshop"),
        "mp4" => (Video, "Video MP4"),
        "mkv" => (Video, "Video MKV"),
        "mov" => (Video, "Video MOV"),
        "avi" | "wmv" | "webm" | "m4v" | "flv" | "3gp" => (Video, "Video"),
        "mp3" => (Audio, "Nhạc MP3"),
        "flac" => (Audio, "Nhạc FLAC"),
        "m4a" | "aac" | "ogg" | "opus" | "wav" | "wma" | "aiff" => (Audio, "Âm thanh"),
        "pdf" => (Pdf, "PDF"),
        "doc" | "docx" | "odt" | "rtf" => (Document, "Word"),
        "xls" | "xlsx" | "xlsm" | "ods" => (Spreadsheet, "Excel"),
        "csv" => (Spreadsheet, "CSV"),
        "ppt" | "pptx" | "odp" => (Presentation, "PowerPoint"),
        "zip" => (Archive, "ZIP"),
        "rar" => (Archive, "RAR"),
        "7z" => (Archive, "7-Zip"),
        "tar" | "gz" | "tgz" | "bz2" | "xz" | "zst" => (Archive, "Tệp nén"),
        "iso" => (Archive, "Ảnh đĩa"),
        "exe" | "msi" => (Executable, "Ứng dụng"),
        "bat" | "cmd" => (Executable, "Batch"),
        "ps1" => (Code, "PowerShell"),
        "lnk" => (Shortcut, "Lối tắt"),
        "url" => (Shortcut, "Liên kết web"),
        "json" => (Code, "JSON"),
        "xml" => (Code, "XML"),
        "yaml" | "yml" => (Code, "YAML"),
        "toml" | "ini" | "cfg" | "conf" => (Code, "Cấu hình"),
        "html" | "htm" => (Code, "HTML"),
        "css" | "scss" => (Code, "CSS"),
        "js" | "mjs" | "ts" | "tsx" | "jsx" => (Code, "JavaScript"),
        "rs" => (Code, "Rust"),
        "cs" => (Code, "C#"),
        "py" => (Code, "Python"),
        "java" | "kt" | "kts" => (Code, "Java/Kotlin"),
        "c" | "h" | "cpp" | "hpp" | "cc" => (Code, "C/C++"),
        "go" => (Code, "Go"),
        "sh" => (Code, "Shell"),
        "sql" => (Code, "SQL"),
        "md" | "markdown" => (Text, "Markdown"),
        "txt" | "log" => (Text, "Văn bản"),
        "ttf" | "otf" | "woff" | "woff2" => (Font, "Phông chữ"),
        "" => (Other, "Tệp"),
        _ => (Other, ""),
    };
    let label = if label.is_empty() { format!("Tệp {}", ext_lower.to_uppercase()) } else { label.to_string() };
    (kind, label)
}

/// True when the kind can be shown as plain text in the preview pane.
pub fn is_textual(ext_lower: &str) -> bool {
    matches!(classify(ext_lower).0, FileKind::Code | FileKind::Text)
        || matches!(ext_lower, "csv" | "svg" | "bat" | "cmd" | "gitignore" | "editorconfig" | "env")
}
