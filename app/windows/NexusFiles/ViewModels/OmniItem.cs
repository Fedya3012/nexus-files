using Nexus.Core;
using NexusFiles.Services;

namespace NexusFiles.ViewModels;

public enum OmniItemKind { Header, File, Folder, Command, Place }

/// <summary>A row in the Omnibar result list (or a section header).</summary>
public sealed class OmniItem
{
    public OmniItemKind Kind { get; init; }
    public string Title { get; init; } = "";
    public string Subtitle { get; init; } = "";
    public string Detail { get; init; } = "";
    public string Shortcut { get; init; } = "";
    public string Glyph { get; init; } = "";
    public Microsoft.UI.Xaml.Media.Brush? GlyphBrush { get; init; }
    public IReadOnlyList<TextRange> Highlights { get; init; } = [];
    public string? Path { get; init; }
    public string? CommandId { get; init; }
    public bool IsHeader => Kind == OmniItemKind.Header;

    public static OmniItem Header(string text) => new() { Kind = OmniItemKind.Header, Title = text };

    public static OmniItem FromHit(SearchHit h) => new()
    {
        Kind = h.IsDir ? OmniItemKind.Folder : OmniItemKind.File,
        Title = h.Name,
        Subtitle = h.Parent,
        Detail = h.IsDir ? "" : $"{Format.Date(h.ModifiedMs)} · {Format.Size(h.Size)}",
        Glyph = Icons.ForKind(h.Kind),
        GlyphBrush = Icons.BrushFor(h.Kind),
        Highlights = h.Highlights,
        Path = h.Path,
    };

    public static OmniItem FromCommand(CommandMatch c) => new()
    {
        Kind = OmniItemKind.Command,
        Title = c.Command.Title,
        Subtitle = c.Command.Category,
        Shortcut = c.Command.Shortcut,
        Glyph = Icons.Command,
        Highlights = c.Highlights,
        CommandId = c.Command.Id,
    };

    public static OmniItem FromPlace(PlaceMatch p) => new()
    {
        Kind = OmniItemKind.Place,
        Title = p.Place.Name,
        Subtitle = p.Place.Path,
        Glyph = Icons.ForPlace(p.Place.Kind),
        GlyphBrush = Icons.BrushFor(FileKind.Folder),
        Highlights = p.Highlights,
        Path = p.Place.Path,
    };
}
