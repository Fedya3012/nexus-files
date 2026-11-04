using Nexus.Core;

namespace NexusFiles.Services;

/// <summary>Vietnamese display formatting matching the mockups.</summary>
public static class Format
{
    public static string Size(ulong bytes)
    {
        string[] units = ["B", "KB", "MB", "GB", "TB"];
        double v = bytes;
        int u = 0;
        while (v >= 1024 && u < units.Length - 1) { v /= 1024; u++; }
        var s = u == 0 ? v.ToString("0") : v < 10 ? v.ToString("0.0") : v.ToString("0");
        return $"{s.Replace('.', ',')} {units[u]}";
    }

    /// <summary>"Hôm nay, 09:14", "Hôm qua, 17:40" or "02/11/2026".</summary>
    public static string Date(long epochMs)
    {
        if (epochMs <= 0) return "";
        var t = DateTimeOffset.FromUnixTimeMilliseconds(epochMs).LocalDateTime;
        var today = DateTime.Today;
        if (t.Date == today) return $"Hôm nay, {t:HH:mm}";
        if (t.Date == today.AddDays(-1)) return $"Hôm qua, {t:HH:mm}";
        return t.ToString("dd/MM/yyyy");
    }

    public static string DateTimeFull(long epochMs) =>
        epochMs <= 0 ? "—" : DateTimeOffset.FromUnixTimeMilliseconds(epochMs).LocalDateTime.ToString("dd/MM/yyyy, HH:mm");

    public static string Items(int n) => $"{n:N0} mục".Replace(',', '.');

    /// <summary>Turns a core error into a short sentence for the user.</summary>
    public static string Error(Exception e) => e switch
    {
        NexusException.NotFound nf => $"Không tìm thấy: {nf.path}",
        NexusException.PermissionDenied pd => $"Không có quyền truy cập: {pd.path}",
        NexusException.AlreadyExists ae => $"Đã có mục cùng tên: {System.IO.Path.GetFileName(ae.path)}",
        NexusException.Invalid inv => inv.message,
        NexusException.Cancelled => "Đã hủy",
        NexusException.Io io => $"{io.message} ({io.path})",
        _ => e.Message,
    };
}
