//! Command registry for the Command Palette (`Ctrl+Shift+P` or `>` in the
//! Omnibar). The core owns ids, titles, keywords and shortcuts so Windows and
//! Android show the same names; each UI maps the id to its own action.

use crate::fuzzy::Fuzzy;
use crate::model::TextRange;

#[derive(Debug, Clone, uniffi::Record)]
pub struct CommandInfo {
    pub id: String,
    pub title: String,
    pub category: String,
    /// Windows shortcut text, empty if none.
    pub shortcut: String,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct CommandMatch {
    pub command: CommandInfo,
    pub highlights: Vec<TextRange>,
    pub score: u32,
}

/// (id, title, category, shortcut, extra keywords)
const COMMANDS: &[(&str, &str, &str, &str, &str)] = &[
    ("file.new_folder", "Tạo thư mục mới", "Tệp", "Ctrl+Shift+N", "new folder mkdir"),
    ("file.new_text", "Tạo tệp văn bản mới", "Tệp", "", "new file txt"),
    ("file.open", "Mở", "Tệp", "Enter", "open"),
    ("file.rename", "Đổi tên", "Tệp", "F2", "rename"),
    ("file.delete", "Xóa (vào Thùng rác)", "Tệp", "Delete", "delete trash recycle"),
    ("file.delete_permanent", "Xóa vĩnh viễn", "Tệp", "Shift+Delete", "delete permanently"),
    ("file.copy_path", "Sao chép đường dẫn", "Tệp", "Ctrl+Shift+C", "copy path"),
    ("file.properties", "Thuộc tính (Windows)", "Tệp", "Alt+Enter", "properties"),
    ("file.open_with", "Mở bằng…", "Tệp", "", "open with"),
    ("file.show_in_explorer", "Mở trong File Explorer", "Tệp", "", "explorer"),
    ("file.open_terminal", "Mở Terminal tại đây", "Tệp", "Ctrl+`", "terminal powershell cmd"),
    ("edit.cut", "Cắt", "Chỉnh sửa", "Ctrl+X", "cut"),
    ("edit.copy", "Copy", "Chỉnh sửa", "Ctrl+C", "copy sao chep"),
    ("edit.paste", "Dán", "Chỉnh sửa", "Ctrl+V", "paste"),
    ("edit.undo", "Hoàn tác", "Chỉnh sửa", "Ctrl+Z", "undo"),
    ("edit.select_all", "Chọn tất cả", "Chỉnh sửa", "Ctrl+A", "select all"),
    ("edit.invert_selection", "Đảo vùng chọn", "Chỉnh sửa", "", "invert selection"),
    ("nav.back", "Quay lại", "Điều hướng", "Alt+Left", "back"),
    ("nav.forward", "Tiến tới", "Điều hướng", "Alt+Right", "forward"),
    ("nav.up", "Lên thư mục cha", "Điều hướng", "Alt+Up", "up parent"),
    ("nav.refresh", "Làm mới", "Điều hướng", "F5", "refresh reload"),
    ("nav.goto", "Đi tới thư mục…", "Điều hướng", "Ctrl+G", "goto jump"),
    ("nav.omnibar", "Tìm hoặc gõ đường dẫn", "Điều hướng", "Ctrl+L", "search address"),
    ("nav.pin", "Ghim / bỏ ghim thư mục này", "Điều hướng", "Ctrl+D", "pin bookmark favorite"),
    ("tab.new", "Tab mới", "Tab", "Ctrl+T", "new tab"),
    ("tab.close", "Đóng tab", "Tab", "Ctrl+W", "close tab"),
    ("tab.reopen", "Mở lại tab vừa đóng", "Tab", "Ctrl+Shift+T", "reopen restore tab"),
    ("tab.next", "Tab kế tiếp", "Tab", "Ctrl+Tab", "next tab"),
    ("tab.previous", "Tab trước", "Tab", "Ctrl+Shift+Tab", "previous tab"),
    ("tab.duplicate", "Nhân đôi tab", "Tab", "", "duplicate tab"),
    ("view.details", "Xem dạng Chi tiết", "Xem", "Ctrl+1", "details view columns"),
    ("view.list", "Xem dạng Danh sách", "Xem", "Ctrl+2", "list view"),
    ("view.icons", "Xem dạng Biểu tượng", "Xem", "Ctrl+3", "icons grid thumbnails"),
    ("view.dual_pane", "Bật/tắt hai pane", "Xem", "Ctrl+Shift+D", "dual pane split"),
    ("view.switch_pane", "Chuyển sang pane bên kia", "Xem", "Tab", "switch pane"),
    ("view.inspector", "Bật/tắt Inspector", "Xem", "Ctrl+I", "inspector preview details"),
    ("view.sidebar", "Bật/tắt Sidebar", "Xem", "Ctrl+B", "sidebar"),
    ("view.hidden", "Hiện/ẩn file ẩn", "Xem", "Ctrl+H", "hidden files"),
    ("sort.name", "Sắp xếp theo tên", "Sắp xếp", "", "sort name"),
    ("sort.modified", "Sắp xếp theo ngày sửa", "Sắp xếp", "", "sort date modified"),
    ("sort.size", "Sắp xếp theo kích thước", "Sắp xếp", "", "sort size"),
    ("sort.type", "Sắp xếp theo loại", "Sắp xếp", "", "sort type"),
    ("app.theme_light", "Giao diện sáng", "Ứng dụng", "", "theme light"),
    ("app.theme_dark", "Giao diện tối", "Ứng dụng", "", "theme dark"),
    ("app.theme_system", "Giao diện theo hệ thống", "Ứng dụng", "", "theme system auto"),
    ("app.palette", "Bảng lệnh", "Ứng dụng", "Ctrl+Shift+P", "command palette"),
    ("app.jobs", "Xem tiến trình copy/move", "Ứng dụng", "", "jobs progress queue"),
];

pub fn all_commands() -> Vec<CommandInfo> {
    COMMANDS
        .iter()
        .map(|(id, title, cat, sc, _)| CommandInfo {
            id: id.to_string(),
            title: title.to_string(),
            category: cat.to_string(),
            shortcut: sc.to_string(),
        })
        .collect()
}

pub fn match_commands(query: &str, limit: usize) -> Vec<CommandMatch> {
    let mut fz = Fuzzy::new(query);
    if fz.is_empty() {
        return all_commands()
            .into_iter()
            .take(limit)
            .map(|c| CommandMatch { command: c, highlights: vec![], score: 0 })
            .collect();
    }
    let mut out: Vec<CommandMatch> = Vec::new();
    for (i, cmd) in all_commands().into_iter().enumerate() {
        let title_hit = fz.score(&cmd.title);
        let kw_hit = fz.score(COMMANDS[i].4).map(|(s, _)| s * 3 / 4);
        let (score, highlights) = match (title_hit, kw_hit) {
            (Some((s, h)), Some(k)) if k > s => (k, h),
            (Some((s, h)), _) => (s, h),
            (None, Some(k)) => (k, vec![]),
            (None, None) => continue,
        };
        out.push(CommandMatch { command: cmd, highlights, score });
    }
    out.sort_by_key(|m| std::cmp::Reverse(m.score));
    out.truncate(limit);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_are_unique() {
        let mut ids: Vec<_> = COMMANDS.iter().map(|c| c.0).collect();
        ids.sort();
        let n = ids.len();
        ids.dedup();
        assert_eq!(ids.len(), n);
    }

    #[test]
    fn finds_by_vietnamese_and_english() {
        assert_eq!(match_commands("doi ten", 3)[0].command.id, "file.rename");
        assert_eq!(match_commands("rename", 3)[0].command.id, "file.rename");
        assert_eq!(match_commands("tab moi", 3)[0].command.id, "tab.new");
    }
}
