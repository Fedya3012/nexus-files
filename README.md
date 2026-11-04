# Nexus Files

Trình quản lý tệp cho Windows, lõi Rust dùng chung (UniFFI) và giao diện WinUI 3. Dùng cá nhân.

## Tải về và chạy

1. Mở tab **Actions** của repo, chọn lần chạy **build** mới nhất có dấu tích xanh.
2. Tải **NexusFiles-win-x64** ở mục Artifacts, giải nén.
3. Chạy `NexusFiles.exe`. Không cần cài đặt; bản này đã kèm sẵn .NET và Windows App SDK.

Khi gắn tag `v*` (ví dụ `v0.1.0`), bản zip được đưa lên mục Releases.

## Phím tắt chính

| Phím | Việc |
|---|---|
| Ctrl+F / Ctrl+E | Tìm kiếm (Omnibar) |
| Ctrl+Shift+P | Bảng lệnh |
| Ctrl+G | Đi tới thư mục |
| Ctrl+L | Sửa đường dẫn |
| Ctrl+T / Ctrl+W / Ctrl+Shift+T | Tab mới / đóng / mở lại |
| Ctrl+Shift+D, Tab | Bật hai pane, chuyển pane |
| F2, Delete, Ctrl+Z | Đổi tên, xóa vào Thùng rác, hoàn tác |
| Ctrl+1/2/3 | Chi tiết / Danh sách / Biểu tượng |
| Ctrl+I, Ctrl+B | Bảng thông tin, thanh bên |
| Ctrl+D | Ghim thư mục hiện tại |

Trong Omnibar: `>` để chạy lệnh, `@` để đi tới thư mục, bộ lọc `ext:pdf`, `size:>10mb`, `sửa:7d`, `kind:ảnh`.

## Tự build

Cần Windows, Rust (stable) và .NET 10 SDK:

```powershell
./build.ps1            # ra dist/NexusFiles-win-x64/NexusFiles.exe
```

## Cấu trúc

- `core/`: lõi Rust (liệt kê thư mục, tìm kiếm có bỏ dấu, Omnibar, hàng đợi copy/move có Undo, phiên làm việc). `cargo test` chạy được trên mọi hệ điều hành.
- `app/windows/NexusFiles/`: ứng dụng WinUI 3.
- `tests/ffi_smoke.py`: kiểm tra lớp FFI qua binding Python.

Dữ liệu (ghim, phiên, cài đặt, nhật ký thao tác) nằm ở `%LOCALAPPDATA%\NexusFiles`.
