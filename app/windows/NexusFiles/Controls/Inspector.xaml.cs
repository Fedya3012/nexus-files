using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Media;
using Microsoft.UI.Xaml.Media.Imaging;
using Nexus.Core;
using NexusFiles.Services;
using NexusFiles.ViewModels;
using Windows.Storage;
using Windows.Storage.Streams;

namespace NexusFiles.Controls;

/// <summary>Right-hand Inspector: details and preview of the selection.</summary>
public sealed partial class Inspector : UserControl
{
    private int _generation;
    private string? _path;

    public Inspector()
    {
        InitializeComponent();
        ShowPlaceholder("Chọn một mục để xem chi tiết");
    }

    private bool PreviewMode => Tabs.SelectedItem == PreviewTab;

    private void OnTabChanged(SelectorBar sender, SelectorBarSelectionChangedEventArgs args) => _ = ReloadAsync();

    private IReadOnlyList<FileItem> _selection = [];
    private string _folder = "";

    /// <summary>Shows the selection of the active pane (or the folder itself when nothing is selected).</summary>
    public void Show(IReadOnlyList<FileItem> selection, string currentFolder)
    {
        _selection = selection;
        _folder = currentFolder;
        _ = ReloadAsync();
    }

    private async Task ReloadAsync()
    {
        var gen = ++_generation;
        ResetVisual();
        if (_selection.Count > 1)
        {
            ShowMulti(_selection);
            return;
        }
        var path = _selection.Count == 1 ? _selection[0].FullPath : _folder;
        if (string.IsNullOrEmpty(path))
        {
            ShowPlaceholder("Chọn một mục để xem chi tiết");
            return;
        }
        PreviewInfo info;
        try
        {
            info = await Task.Run(() => NexusNative.Preview(path));
        }
        catch (Exception e)
        {
            if (gen == _generation) ShowPlaceholder(Format.Error(e));
            return;
        }
        if (gen != _generation) return;
        _path = info.Path;
        Placeholder.Visibility = Visibility.Collapsed;
        Body.Visibility = Visibility.Visible;
        Actions.Visibility = Visibility.Visible;
        var e1 = info.Entry;
        NameText.Text = e1.Name;
        BigIcon.Glyph = Icons.ForKind(e1.Kind);
        BigIcon.Foreground = Icons.BrushFor(e1.Kind);

        var subtitle = e1.IsDir ? "Thư mục" : $"{e1.TypeLabel} · {Format.Size(e1.Size)}";
        InfoGrid.Children.Clear();
        InfoGrid.RowDefinitions.Clear();
        AddInfo("Đường dẫn", info.Path, mono: true);
        if (!e1.IsDir) AddInfo("Kích thước", $"{Format.Size(e1.Size)} ({e1.Size:N0} byte)".Replace(',', '.'));
        AddInfo("Ngày tạo", Format.DateTimeFull(e1.CreatedMs));
        AddInfo("Ngày sửa", Format.DateTimeFull(e1.ModifiedMs));
        var attrs = new List<string>();
        if (info.IsReadonly) attrs.Add("Chỉ đọc");
        if (e1.IsHidden) attrs.Add("Ẩn");
        if (e1.IsSymlink) attrs.Add("Liên kết");
        if (attrs.Count > 0) AddInfo("Thuộc tính", string.Join(", ", attrs));

        FolderList.Children.Clear();
        FolderList.Visibility = Visibility.Collapsed;
        switch (info.Content)
        {
            case PreviewContent.Image img:
                if (img.Width > 0) subtitle += $" · {img.Width} × {img.Height}";
                await ShowImageAsync(info.Path, gen);
                break;
            case PreviewContent.Folder f:
                subtitle = $"Thư mục · {f.Folders} thư mục, {f.Files} tệp";
                if (f.FirstNames.Length > 0)
                {
                    FolderList.Visibility = Visibility.Visible;
                    FolderList.Children.Add(new TextBlock { Text = "BÊN TRONG", Style = (Style)Application.Current.Resources["NxSectionHeader"], Margin = new Thickness(0, 4, 0, 2) });
                    foreach (var n in f.FirstNames)
                        FolderList.Children.Add(new TextBlock { Text = n, FontSize = 13, TextTrimming = TextTrimming.CharacterEllipsis, Foreground = Res.Brush("NxText2") });
                }
                break;
            case PreviewContent.Text t when PreviewMode:
                ShowText(t.TextValue, t.Truncated);
                break;
            case PreviewContent.Pdf when PreviewMode:
                await ShowPdfAsync(info.Path, gen);
                break;
            case PreviewContent.Media when PreviewMode:
                await ShowMediaAsync(info.Path);
                break;
            case PreviewContent.Text or PreviewContent.Pdf or PreviewContent.Media:
                await ShowShellThumbnailAsync(info.Path, gen);
                break;
            default:
                await ShowShellThumbnailAsync(info.Path, gen);
                break;
        }
        SubtitleText.Text = subtitle;
    }

    private void ResetVisual()
    {
        PreviewImage.Source = null;
        PreviewImage.Visibility = Visibility.Collapsed;
        TextScroller.Visibility = Visibility.Collapsed;
        PreviewText.Text = "";
        if (Media.MediaPlayer != null) Media.MediaPlayer.Pause();
        Media.Source = null;
        Media.Visibility = Visibility.Collapsed;
        BigIcon.Visibility = Visibility.Visible;
    }

    private void ShowPlaceholder(string text)
    {
        Body.Visibility = Visibility.Collapsed;
        Placeholder.Visibility = Visibility.Visible;
        PlaceholderText.Text = text;
        _path = null;
    }

    private void ShowMulti(IReadOnlyList<FileItem> items)
    {
        Placeholder.Visibility = Visibility.Collapsed;
        Body.Visibility = Visibility.Visible;
        Actions.Visibility = Visibility.Collapsed;
        _path = null;
        BigIcon.Glyph = "";
        BigIcon.Foreground = Res.Brush("NxAccent");
        NameText.Text = $"{items.Count} mục đã chọn";
        var files = items.Where(i => !i.IsDir).ToList();
        SubtitleText.Text = $"{items.Count - files.Count} thư mục, {files.Count} tệp · {Format.Size((ulong)files.Sum(f => (long)f.Entry.Size))}";
        InfoGrid.Children.Clear();
        InfoGrid.RowDefinitions.Clear();
        FolderList.Children.Clear();
        FolderList.Visibility = Visibility.Visible;
        foreach (var i in items.Take(30))
            FolderList.Children.Add(new TextBlock { Text = i.Name, FontSize = 13, TextTrimming = TextTrimming.CharacterEllipsis });
    }

    private void AddInfo(string label, string value, bool mono = false)
    {
        int row = InfoGrid.RowDefinitions.Count;
        InfoGrid.RowDefinitions.Add(new RowDefinition { Height = GridLength.Auto });
        var l = new TextBlock { Text = label, Foreground = Res.Brush("NxText3"), FontSize = 13 };
        var v = new TextBlock { Text = value, FontSize = 13, TextWrapping = TextWrapping.Wrap, IsTextSelectionEnabled = true };
        if (mono) v.FontFamily = (FontFamily)Application.Current.Resources["NxMono"];
        Grid.SetRow(l, row);
        Grid.SetRow(v, row);
        Grid.SetColumn(v, 1);
        InfoGrid.Children.Add(l);
        InfoGrid.Children.Add(v);
    }

    private async Task ShowImageAsync(string path, int gen)
    {
        try
        {
            var file = await StorageFile.GetFileFromPathAsync(path);
            using var stream = await file.OpenReadAsync();
            var bmp = new BitmapImage { DecodePixelWidth = 640 };
            await bmp.SetSourceAsync(stream);
            if (gen != _generation) return;
            PreviewImage.Source = bmp;
            PreviewImage.Visibility = Visibility.Visible;
            BigIcon.Visibility = Visibility.Collapsed;
        }
        catch
        {
            await ShowShellThumbnailAsync(path, gen);
        }
    }

    private async Task ShowShellThumbnailAsync(string path, int gen)
    {
        try
        {
            IStorageItemProperties item = Directory.Exists(path)
                ? await StorageFolder.GetFolderFromPathAsync(path)
                : await StorageFile.GetFileFromPathAsync(path);
            using var thumb = await item.GetThumbnailAsync(Windows.Storage.FileProperties.ThumbnailMode.SingleItem, 256);
            if (thumb == null || thumb.Size == 0 || gen != _generation) return;
            var bmp = new BitmapImage();
            await bmp.SetSourceAsync(thumb);
            if (gen != _generation) return;
            PreviewImage.Source = bmp;
            PreviewImage.Visibility = Visibility.Visible;
            BigIcon.Visibility = Visibility.Collapsed;
        }
        catch
        {
            // Glyph stays.
        }
    }

    private void ShowText(string text, bool truncated)
    {
        PreviewText.Text = truncated ? text + "\n\n… (chỉ hiện 64 KB đầu)" : text;
        TextScroller.Visibility = Visibility.Visible;
        BigIcon.Visibility = Visibility.Collapsed;
    }

    /// <summary>Renders the first page with the PDF engine built into Windows.</summary>
    private async Task ShowPdfAsync(string path, int gen)
    {
        try
        {
            var file = await StorageFile.GetFileFromPathAsync(path);
            var doc = await Windows.Data.Pdf.PdfDocument.LoadFromFileAsync(file);
            if (doc.PageCount == 0) return;
            using var page = doc.GetPage(0);
            using var stream = new InMemoryRandomAccessStream();
            await page.RenderToStreamAsync(stream, new Windows.Data.Pdf.PdfPageRenderOptions { DestinationWidth = 640 });
            var bmp = new BitmapImage();
            await bmp.SetSourceAsync(stream);
            if (gen != _generation) return;
            PreviewImage.Source = bmp;
            PreviewImage.Visibility = Visibility.Visible;
            BigIcon.Visibility = Visibility.Collapsed;
        }
        catch
        {
            await ShowShellThumbnailAsync(path, gen);
        }
    }

    private async Task ShowMediaAsync(string path)
    {
        try
        {
            var file = await StorageFile.GetFileFromPathAsync(path);
            Media.Source = Windows.Media.Core.MediaSource.CreateFromStorageFile(file);
            Media.Visibility = Visibility.Visible;
            BigIcon.Visibility = Visibility.Collapsed;
        }
        catch
        {
            // Unsupported codec: keep the glyph.
        }
    }

    public void StopMedia()
    {
        if (Media.MediaPlayer != null) Media.MediaPlayer.Pause();
    }

    private void OnOpen(object sender, RoutedEventArgs e)
    {
        if (_path == null) return;
        if (Directory.Exists(_path)) OpenFolderRequested?.Invoke(this, _path);
        else Shell.Open(_path);
    }

    private void OnShowInExplorer(object sender, RoutedEventArgs e)
    {
        if (_path != null) Shell.ShowInExplorer(_path);
    }

    private void OnProperties(object sender, RoutedEventArgs e)
    {
        if (_path != null) Shell.Properties(_path);
    }

    public event EventHandler<string>? OpenFolderRequested;
}
