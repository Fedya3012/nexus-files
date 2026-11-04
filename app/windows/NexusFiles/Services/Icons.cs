using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Media;
using Nexus.Core;

namespace NexusFiles.Services;

/// <summary>Segoe Fluent Icons glyphs and colours per file kind (Details/List views).</summary>
public static class Icons
{
    public const string Folder = "";
    public const string Back = "";
    public const string Forward = "";
    public const string Up = "";
    public const string Refresh = "";
    public const string Search = "";
    public const string Pin = "";
    public const string Drive = "";
    public const string Home = "";
    public const string Desktop = "";
    public const string Document = "";
    public const string Download = "";
    public const string Picture = "";
    public const string Music = "";
    public const string Video = "";
    public const string Command = "";
    public const string Recent = "";
    public const string GoTo = "";

    public static string ForKind(FileKind kind) => kind switch
    {
        FileKind.Folder => Folder,
        FileKind.Image => "",
        FileKind.Video => "",
        FileKind.Audio => "",
        FileKind.Pdf => "",
        FileKind.Document => "",
        FileKind.Spreadsheet => "",
        FileKind.Presentation => "",
        FileKind.Archive => "",
        FileKind.Code => "",
        FileKind.Text => "",
        FileKind.Executable => "",
        FileKind.Shortcut => "",
        FileKind.Font => "",
        _ => "",
    };

    public static string ForPlace(PlaceKind kind) => kind switch
    {
        PlaceKind.Home => Home,
        PlaceKind.Desktop => Desktop,
        PlaceKind.Documents => Document,
        PlaceKind.Downloads => Download,
        PlaceKind.Pictures => Picture,
        PlaceKind.Music => Music,
        PlaceKind.Videos => Video,
        PlaceKind.Drive => Drive,
        PlaceKind.Pinned => Folder,
        _ => Folder,
    };

    private static readonly Dictionary<FileKind, (byte r, byte g, byte b)> KindColors = new()
    {
        [FileKind.Image] = (0x16, 0x8A, 0x4A),
        [FileKind.Video] = (0xD9, 0x3A, 0x3A),
        [FileKind.Audio] = (0xE0, 0x8A, 0x00),
        [FileKind.Pdf] = (0xD9, 0x3A, 0x3A),
        [FileKind.Document] = (0x2B, 0x6C, 0xD9),
        [FileKind.Spreadsheet] = (0x16, 0x8A, 0x4A),
        [FileKind.Presentation] = (0xD9, 0x6A, 0x2B),
        [FileKind.Archive] = (0xB0, 0x8A, 0x00),
        [FileKind.Code] = (0x6E, 0x6C, 0xF0),
    };

    private static readonly Dictionary<FileKind, Brush> BrushCache = [];

    public static Brush BrushFor(FileKind kind)
    {
        if (kind == FileKind.Folder)
            return Res.Brush("NxFolder");
        if (!KindColors.TryGetValue(kind, out var c)) return Res.Brush("NxText3");
        if (BrushCache.TryGetValue(kind, out var b)) return b;
        b = new SolidColorBrush(Windows.UI.Color.FromArgb(255, c.r, c.g, c.b));
        BrushCache[kind] = b;
        return b;
    }
}
