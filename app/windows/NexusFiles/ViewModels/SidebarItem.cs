using Microsoft.UI.Xaml.Media;
using Nexus.Core;
using NexusFiles.Services;

namespace NexusFiles.ViewModels;

/// <summary>A row in the sidebar (pinned folder, library folder or drive).</summary>
public sealed class SidebarItem
{
    public required string Name { get; init; }
    public required string Path { get; init; }
    public string Glyph { get; init; } = Icons.Folder;
    public Brush? GlyphBrush { get; init; }
    public bool IsPinned { get; init; }

    // Drives only.
    public bool ShowUsage { get; init; }
    public double UsagePercent { get; init; }
    public string UsageText { get; init; } = "";
    public bool IsLow { get; init; }

    public static SidebarItem FromPlace(Place p) => new()
    {
        Name = p.Name,
        Path = p.Path,
        Glyph = Icons.ForPlace(p.Kind),
        GlyphBrush = p.Kind == PlaceKind.Pinned ? Icons.BrushFor(FileKind.Folder) : null,
        IsPinned = p.Kind == PlaceKind.Pinned,
    };

    public static SidebarItem FromDrive(DriveInfo d)
    {
        var used = d.TotalBytes == 0 ? 0 : 100.0 * (d.TotalBytes - d.FreeBytes) / d.TotalBytes;
        var low = d.TotalBytes > 0 && d.FreeBytes < d.TotalBytes / 10;
        return new SidebarItem
        {
            Name = d.Name,
            Path = d.Path,
            Glyph = Icons.Drive,
            ShowUsage = d.TotalBytes > 0,
            UsagePercent = used,
            IsLow = low,
            UsageText = low ? $"Sắp đầy · {Format.Size(d.FreeBytes)} trống" : $"{Format.Size(d.FreeBytes)} trống / {Format.Size(d.TotalBytes)}",
        };
    }
}
