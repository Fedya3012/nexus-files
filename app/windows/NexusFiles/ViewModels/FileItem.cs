using CommunityToolkit.Mvvm.ComponentModel;
using Microsoft.UI.Xaml.Media;
using Nexus.Core;
using NexusFiles.Services;

namespace NexusFiles.ViewModels;

/// <summary>One row/tile in a pane.</summary>
public sealed class FileItem : ObservableObject
{
    private string _sizeText;
    private ImageSource? _thumbnail;
    private bool _isCut;

    public FileItem(string directory, FileEntry entry)
    {
        Entry = entry;
        FullPath = System.IO.Path.Combine(directory, entry.Name);
        _sizeText = entry.IsDir ? "" : Format.Size(entry.Size);
    }

    public FileEntry Entry { get; }
    public string FullPath { get; }
    public string Name => Entry.Name;
    public bool IsDir => Entry.IsDir;
    public string Glyph => Icons.ForKind(Entry.Kind);
    public Brush GlyphBrush => Icons.BrushFor(Entry.Kind);
    public string ModifiedText => Format.Date(Entry.ModifiedMs);
    public string TypeText => Entry.TypeLabel;
    public double ItemOpacity => Entry.IsHidden || _isCut ? 0.55 : 1.0;

    public string SizeText
    {
        get => _sizeText;
        set => SetProperty(ref _sizeText, value);
    }

    /// <summary>Shell thumbnail, loaded lazily for the Icons view.</summary>
    public ImageSource? Thumbnail
    {
        get => _thumbnail;
        set
        {
            if (SetProperty(ref _thumbnail, value)) OnPropertyChanged(nameof(HasThumbnail));
        }
    }

    public bool HasThumbnail => _thumbnail != null;
    public bool ThumbnailRequested { get; set; }

    /// <summary>Dimmed while it sits on the clipboard after "Cắt".</summary>
    public bool IsCut
    {
        get => _isCut;
        set
        {
            if (SetProperty(ref _isCut, value)) OnPropertyChanged(nameof(ItemOpacity));
        }
    }
}
