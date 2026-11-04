using Microsoft.UI.Input;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Input;
using Microsoft.UI.Xaml.Media.Imaging;
using Nexus.Core;
using NexusFiles.ViewModels;
using Windows.ApplicationModel.DataTransfer;
using Windows.Storage;
using Windows.System;
using Windows.UI.Core;

namespace NexusFiles.Controls;

public enum OpenTarget { Here, NewTab, OtherPane }

public sealed class DropRequest
{
    public required List<string> Sources { get; init; }
    public required string TargetFolder { get; init; }
    public required bool Move { get; init; }
}

/// <summary>One file pane: header, column headers and the Details/List/Icons views.</summary>
public sealed partial class FilePane : UserControl
{
    public const string InternalPathsFormat = "NexusFiles.Paths";

    private PaneViewModel? _vm;
    private static readonly SemaphoreSlim ThumbnailGate = new(4);

    public FilePane()
    {
        InitializeComponent();
        foreach (var view in new ListViewBase[] { List, Tiles })
        {
            view.SelectionChanged += (_, _) => OnSelectionChanged();
            view.DoubleTapped += OnDoubleTapped;
            view.PreviewKeyDown += OnPreviewKeyDown;
            view.ContextRequested += OnContextRequested;
            view.DragItemsStarting += OnDragItemsStarting;
            view.DragOver += OnDragOver;
            view.Drop += OnDrop;
            view.GotFocus += (_, _) => Activated?.Invoke(this, EventArgs.Empty);
        }
        Tiles.ContainerContentChanging += OnTileContainerChanging;
        Root.PointerPressed += (_, _) => Activated?.Invoke(this, EventArgs.Empty);
        DragOver += OnDragOver;
        Drop += OnDrop;
    }

    public event EventHandler? Activated;
    public event EventHandler? SelectionChanged;
    public event EventHandler<(FileItem item, OpenTarget target)>? OpenRequested;
    public event EventHandler<string>? CommandInvoked;
    public event EventHandler<DropRequest>? DropRequested;

    public PaneViewModel? ViewModel
    {
        get => _vm;
        set
        {
            if (_vm == value) return;
            if (_vm != null)
            {
                _vm.RememberedSelection = SelectedItems.Select(i => i.FullPath).ToList();
                _vm.PropertyChanged -= OnVmPropertyChanged;
                _vm.Loaded -= OnVmLoaded;
            }
            _vm = value;
            if (_vm != null)
            {
                _vm.PropertyChanged += OnVmPropertyChanged;
                _vm.Loaded += OnVmLoaded;
                foreach (var p in _vm.RememberedSelection) _vm.PendingSelection.Add(p);
            }
            Rebind();
        }
    }

    private ListViewBase ActiveView => _vm?.ViewMode == ViewMode.Icons ? Tiles : List;

    public IReadOnlyList<FileItem> SelectedItems => ActiveView.SelectedItems.OfType<FileItem>().ToList();

    public FileItem? FocusedItem =>
        (FocusManager.GetFocusedElement(XamlRoot) as SelectorItem)?.Content as FileItem ?? SelectedItems.FirstOrDefault();

    private void Rebind()
    {
        if (_vm == null)
        {
            List.ItemsSource = null;
            Tiles.ItemsSource = null;
            return;
        }
        ApplyViewMode();
        UpdateHeader();
        OnVmLoaded(this, EventArgs.Empty);
    }

    private void OnVmPropertyChanged(object? sender, System.ComponentModel.PropertyChangedEventArgs e)
    {
        switch (e.PropertyName)
        {
            case nameof(PaneViewModel.ViewMode):
                ApplyViewMode();
                break;
            case nameof(PaneViewModel.IsActive):
            case nameof(PaneViewModel.Title):
            case nameof(PaneViewModel.Summary):
            case nameof(PaneViewModel.Sort):
            case nameof(PaneViewModel.IsLoading):
            case nameof(PaneViewModel.ErrorText):
                UpdateHeader();
                break;
        }
    }

    private void ApplyViewMode()
    {
        if (_vm == null) return;
        var selected = (List.Visibility == Visibility.Visible ? List : Tiles).SelectedItems.OfType<FileItem>().Select(i => i.FullPath).ToList();
        var mode = _vm.ViewMode;
        List.ItemTemplate = (DataTemplate)Resources[mode == ViewMode.List ? "ListTemplate" : "DetailsTemplate"];
        List.Visibility = mode == ViewMode.Icons ? Visibility.Collapsed : Visibility.Visible;
        Tiles.Visibility = mode == ViewMode.Icons ? Visibility.Visible : Visibility.Collapsed;
        ColumnHeaders.Visibility = mode == ViewMode.Details ? Visibility.Visible : Visibility.Collapsed;
        DetailsToggle.IsChecked = mode == ViewMode.Details;
        ListToggle.IsChecked = mode == ViewMode.List;
        IconsToggle.IsChecked = mode == ViewMode.Icons;
        List.ItemsSource = mode == ViewMode.Icons ? null : _vm.Items;
        Tiles.ItemsSource = mode == ViewMode.Icons ? _vm.Items : null;
        SelectPaths(selected, focus: false);
    }

    private void UpdateHeader()
    {
        if (_vm == null) return;
        TitleText.Text = _vm.Title;
        SummaryText.Text = _vm.Summary;
        LoadingRing.IsActive = _vm.IsLoading;
        ActiveBar.Visibility = _vm.IsActive ? Visibility.Visible : Visibility.Collapsed;
        var s = _vm.Sort;
        var arrow = s.Descending ? "↓" : "↑";
        SortText.Text = $"{SortName(s.Key)} {arrow}";
        NameHeader.Content = "Tên" + (s.Key == SortKey.Name ? $" {arrow}" : "");
        ModifiedHeader.Content = "Ngày sửa" + (s.Key == SortKey.Modified ? $" {arrow}" : "");
        TypeHeader.Content = "Loại" + (s.Key == SortKey.Type ? $" {arrow}" : "");
        SizeHeader.Content = "Kích thước" + (s.Key == SortKey.Size ? $" {arrow}" : "");
        UpdateEmptyState();
    }

    private static string SortName(SortKey k) => k switch
    {
        SortKey.Modified => "Ngày sửa",
        SortKey.Size => "Kích thước",
        SortKey.Type => "Loại",
        _ => "Tên",
    };

    private void UpdateEmptyState()
    {
        if (_vm == null) return;
        if (_vm.HasError)
        {
            EmptyState.Visibility = Visibility.Visible;
            EmptyIcon.Glyph = "";
            EmptyText.Text = _vm.ErrorText;
        }
        else if (!_vm.IsLoading && _vm.Items.Count == 0)
        {
            EmptyState.Visibility = Visibility.Visible;
            EmptyIcon.Glyph = "";
            EmptyText.Text = "Thư mục này trống";
        }
        else
        {
            EmptyState.Visibility = Visibility.Collapsed;
        }
    }

    private void OnVmLoaded(object? sender, EventArgs e)
    {
        if (_vm == null) return;
        var view = ActiveView;
        if (!ReferenceEquals(view.ItemsSource, _vm.Items)) view.ItemsSource = _vm.Items;
        UpdateHeader();
        if (_vm.PendingSelection.Count > 0)
        {
            SelectPaths(_vm.PendingSelection.ToList(), focus: _vm.IsActive);
            _vm.PendingSelection.Clear();
        }
        else if (_vm.IsActive && view.Items.Count > 0 && view.SelectedItems.Count == 0)
        {
            view.ScrollIntoView(view.Items[0]);
        }
        SelectionChanged?.Invoke(this, EventArgs.Empty);
    }

    public void SelectPaths(IReadOnlyCollection<string> paths, bool focus)
    {
        if (_vm == null) return;
        var view = ActiveView;
        var set = new HashSet<string>(paths, StringComparer.OrdinalIgnoreCase);
        view.SelectedItems.Clear();
        FileItem? first = null;
        foreach (var item in _vm.Items)
        {
            if (!set.Contains(item.FullPath)) continue;
            view.SelectedItems.Add(item);
            first ??= item;
        }
        if (first != null)
        {
            view.ScrollIntoView(first);
            if (focus) FocusItem(first);
        }
    }

    private void FocusItem(FileItem item)
    {
        var view = ActiveView;
        DispatcherQueue.TryEnqueue(Microsoft.UI.Dispatching.DispatcherQueuePriority.Low, () =>
        {
            if (view.ContainerFromItem(item) is Control c) c.Focus(FocusState.Keyboard);
            else view.Focus(FocusState.Keyboard);
        });
    }

    public void FocusItems()
    {
        var view = ActiveView;
        var target = view.SelectedItems.OfType<FileItem>().FirstOrDefault() ?? view.Items.OfType<FileItem>().FirstOrDefault();
        if (target != null) FocusItem(target);
        else view.Focus(FocusState.Keyboard);
    }

    public void SelectAll() => ActiveView.SelectAll();

    public void InvertSelection()
    {
        var view = ActiveView;
        var selected = new HashSet<object>(view.SelectedItems);
        view.SelectedItems.Clear();
        foreach (var i in view.Items)
            if (!selected.Contains(i)) view.SelectedItems.Add(i);
    }

    private void OnSelectionChanged() => SelectionChanged?.Invoke(this, EventArgs.Empty);

    // ---- input -------------------------------------------------------------

    private static bool IsDown(VirtualKey key) =>
        InputKeyboardSource.GetKeyStateForCurrentThread(key).HasFlag(CoreVirtualKeyStates.Down);

    private void OnDoubleTapped(object sender, DoubleTappedRoutedEventArgs e)
    {
        if ((e.OriginalSource as FrameworkElement)?.DataContext is FileItem item)
        {
            OpenRequested?.Invoke(this, (item, IsDown(VirtualKey.Control) ? OpenTarget.NewTab : OpenTarget.Here));
            e.Handled = true;
        }
    }

    private void OnPreviewKeyDown(object sender, KeyRoutedEventArgs e)
    {
        bool ctrl = IsDown(VirtualKey.Control), shift = IsDown(VirtualKey.Shift);
        string? cmd = (e.Key, ctrl, shift) switch
        {
            (VirtualKey.Back, false, false) => "nav.up",
            (VirtualKey.F2, false, false) => "file.rename",
            (VirtualKey.Delete, false, false) => "file.delete",
            (VirtualKey.Delete, false, true) => "file.delete_permanent",
            (VirtualKey.C, true, false) => "edit.copy",
            (VirtualKey.X, true, false) => "edit.cut",
            (VirtualKey.V, true, false) => "edit.paste",
            (VirtualKey.A, true, false) => "edit.select_all",
            (VirtualKey.C, true, true) => "file.copy_path",
            (VirtualKey.N, true, true) => "file.new_folder",
            (VirtualKey.Tab, false, false) => "view.switch_pane",
            _ => null,
        };
        if (e.Key == VirtualKey.Enter)
        {
            var item = FocusedItem;
            if (item != null)
            {
                var target = ctrl ? OpenTarget.NewTab : shift ? OpenTarget.OtherPane : OpenTarget.Here;
                if (IsDown(VirtualKey.Menu)) CommandInvoked?.Invoke(this, "file.properties");
                else OpenRequested?.Invoke(this, (item, target));
                e.Handled = true;
            }
            return;
        }
        if (cmd != null)
        {
            CommandInvoked?.Invoke(this, cmd);
            e.Handled = true;
        }
    }

    private void OnSortMenu(object sender, RoutedEventArgs e)
    {
        if (_vm != null && sender is FrameworkElement { Tag: string key } && Enum.TryParse<SortKey>(key, out var k)) _vm.SetSort(k);
    }

    private void OnColumnHeader(object sender, RoutedEventArgs e) => OnSortMenu(sender, e);

    private void OnViewToggle(object sender, RoutedEventArgs e)
    {
        if (_vm != null && sender is FrameworkElement { Tag: string mode } && Enum.TryParse<ViewMode>(mode, out var m))
            _vm.ViewMode = m;
        ApplyViewMode();
    }

    // ---- context menu ------------------------------------------------------

    private void OnContextRequested(UIElement sender, ContextRequestedEventArgs e)
    {
        var view = ActiveView;
        var item = (e.OriginalSource as FrameworkElement)?.DataContext as FileItem;
        if (item != null && !view.SelectedItems.Contains(item))
        {
            view.SelectedItems.Clear();
            view.SelectedItems.Add(item);
        }
        if (item == null) view.SelectedItems.Clear();

        var menu = new MenuFlyout();
        void Add(string text, string glyph, string command, string? accel = null)
        {
            var mi = new MenuFlyoutItem { Text = text, Icon = new FontIcon { Glyph = glyph } };
            if (accel != null) mi.KeyboardAcceleratorTextOverride = accel;
            mi.Click += (_, _) => CommandInvoked?.Invoke(this, command);
            menu.Items.Add(mi);
        }
        if (item != null)
        {
            var open = new MenuFlyoutItem { Text = "Mở", Icon = new FontIcon { Glyph = "" }, FontWeight = Microsoft.UI.Text.FontWeights.SemiBold };
            open.Click += (_, _) => OpenRequested?.Invoke(this, (item, OpenTarget.Here));
            menu.Items.Add(open);
            if (item.IsDir)
            {
                var tab = new MenuFlyoutItem { Text = "Mở trong tab mới", Icon = new FontIcon { Glyph = "" }, KeyboardAcceleratorTextOverride = "Ctrl+Enter" };
                tab.Click += (_, _) => OpenRequested?.Invoke(this, (item, OpenTarget.NewTab));
                menu.Items.Add(tab);
                var other = new MenuFlyoutItem { Text = "Mở ở pane bên kia", Icon = new FontIcon { Glyph = "" }, KeyboardAcceleratorTextOverride = "Shift+Enter" };
                other.Click += (_, _) => OpenRequested?.Invoke(this, (item, OpenTarget.OtherPane));
                menu.Items.Add(other);
            }
            else
            {
                Add("Mở bằng…", "", "file.open_with");
            }
            menu.Items.Add(new MenuFlyoutSeparator());
            Add("Cắt", "", "edit.cut", "Ctrl+X");
            Add("Copy", "", "edit.copy", "Ctrl+C");
            Add("Dán", "", "edit.paste", "Ctrl+V");
            Add("Đổi tên", "", "file.rename", "F2");
            Add("Xóa", "", "file.delete", "Delete");
            menu.Items.Add(new MenuFlyoutSeparator());
            Add("Sao chép đường dẫn", "", "file.copy_path", "Ctrl+Shift+C");
            if (item.IsDir) Add("Ghim / bỏ ghim", "", "nav.pin_selected");
            Add("Mở trong File Explorer", "", "file.show_in_explorer");
            Add("Thuộc tính", "", "file.properties", "Alt+Enter");
        }
        else
        {
            var newSub = new MenuFlyoutSubItem { Text = "Mới", Icon = new FontIcon { Glyph = "" } };
            var nf = new MenuFlyoutItem { Text = "Thư mục", Icon = new FontIcon { Glyph = "" }, KeyboardAcceleratorTextOverride = "Ctrl+Shift+N" };
            nf.Click += (_, _) => CommandInvoked?.Invoke(this, "file.new_folder");
            var nt = new MenuFlyoutItem { Text = "Tệp văn bản", Icon = new FontIcon { Glyph = "" } };
            nt.Click += (_, _) => CommandInvoked?.Invoke(this, "file.new_text");
            newSub.Items.Add(nf);
            newSub.Items.Add(nt);
            menu.Items.Add(newSub);
            Add("Dán", "", "edit.paste", "Ctrl+V");
            Add("Hoàn tác", "", "edit.undo", "Ctrl+Z");
            menu.Items.Add(new MenuFlyoutSeparator());
            Add("Làm mới", "", "nav.refresh", "F5");
            Add("Ghim / bỏ ghim thư mục này", "", "nav.pin", "Ctrl+D");
            Add("Mở Terminal tại đây", "", "file.open_terminal");
            Add("Mở trong File Explorer", "", "file.show_in_explorer");
        }
        if (e.TryGetPosition(view, out var pt)) menu.ShowAt(view, pt);
        else menu.ShowAt((FrameworkElement)sender);
        e.Handled = true;
    }

    // ---- thumbnails (Icons view) --------------------------------------------

    private void OnTileContainerChanging(ListViewBase sender, ContainerContentChangingEventArgs args)
    {
        if (args.InRecycleQueue || args.Item is not FileItem item || item.ThumbnailRequested) return;
        item.ThumbnailRequested = true;
        _ = LoadThumbnailAsync(item);
    }

    private async Task LoadThumbnailAsync(FileItem item)
    {
        await ThumbnailGate.WaitAsync();
        try
        {
            IStorageItemProperties storage = item.IsDir
                ? await StorageFolder.GetFolderFromPathAsync(item.FullPath)
                : await StorageFile.GetFileFromPathAsync(item.FullPath);
            using var thumb = await storage.GetThumbnailAsync(Windows.Storage.FileProperties.ThumbnailMode.SingleItem, 96,
                Windows.Storage.FileProperties.ThumbnailOptions.UseCurrentScale);
            if (thumb == null || thumb.Size == 0) return;
            var bmp = new BitmapImage();
            await bmp.SetSourceAsync(thumb);
            item.Thumbnail = bmp;
        }
        catch
        {
            // Keep the glyph.
        }
        finally
        {
            ThumbnailGate.Release();
        }
    }

    // ---- drag & drop -------------------------------------------------------

    private void OnDragItemsStarting(object sender, DragItemsStartingEventArgs e)
    {
        var paths = e.Items.OfType<FileItem>().Select(i => i.FullPath).ToList();
        if (paths.Count == 0)
        {
            e.Cancel = true;
            return;
        }
        e.Data.RequestedOperation = DataPackageOperation.Copy | DataPackageOperation.Move;
        e.Data.SetData(InternalPathsFormat, string.Join("\n", paths));
        // Lets the user drop onto File Explorer, the desktop or other apps.
        e.Data.SetDataProvider(StandardDataFormats.StorageItems, async request =>
        {
            var deferral = request.GetDeferral();
            try
            {
                var items = new List<IStorageItem>();
                foreach (var p in paths)
                {
                    try
                    {
                        items.Add(Directory.Exists(p) ? await StorageFolder.GetFolderFromPathAsync(p) : await StorageFile.GetFileFromPathAsync(p));
                    }
                    catch { }
                }
                request.SetData(items);
            }
            finally
            {
                deferral.Complete();
            }
        });
    }

    private string? DropTargetFolder(object originalSource)
    {
        if ((originalSource as FrameworkElement)?.DataContext is FileItem { IsDir: true } folder) return folder.FullPath;
        return _vm?.CurrentPath;
    }

    private void OnDragOver(object sender, DragEventArgs e)
    {
        if (_vm == null || _vm.HasError) return;
        var hasInternal = e.DataView.Contains(InternalPathsFormat);
        if (!hasInternal && !e.DataView.Contains(StandardDataFormats.StorageItems)) return;
        var target = DropTargetFolder(e.OriginalSource);
        bool move;
        if (e.Modifiers.HasFlag(Windows.ApplicationModel.DataTransfer.DragDrop.DragDropModifiers.Shift)) move = true;
        else if (e.Modifiers.HasFlag(Windows.ApplicationModel.DataTransfer.DragDrop.DragDropModifiers.Control)) move = false;
        else move = hasInternal; // same app: move like Explorer within a drive; refined on drop
        e.AcceptedOperation = move ? DataPackageOperation.Move : DataPackageOperation.Copy;
        e.DragUIOverride.Caption = (move ? "Di chuyển vào " : "Copy vào ") + System.IO.Path.GetFileName(target?.TrimEnd('\\') ?? "");
        e.Handled = true;
    }

    private async void OnDrop(object sender, DragEventArgs e)
    {
        if (_vm == null) return;
        e.Handled = true;
        var target = DropTargetFolder(e.OriginalSource);
        if (target == null) return;
        var deferral = e.GetDeferral();
        try
        {
            List<string> sources;
            if (e.DataView.Contains(InternalPathsFormat))
            {
                var text = (string)await e.DataView.GetDataAsync(InternalPathsFormat);
                sources = text.Split('\n', StringSplitOptions.RemoveEmptyEntries).ToList();
            }
            else
            {
                var items = await e.DataView.GetStorageItemsAsync();
                sources = items.Select(i => i.Path).Where(p => !string.IsNullOrEmpty(p)).ToList();
            }
            sources.RemoveAll(s => string.Equals(s, target, StringComparison.OrdinalIgnoreCase));
            if (sources.Count == 0) return;
            bool move;
            if (e.Modifiers.HasFlag(Windows.ApplicationModel.DataTransfer.DragDrop.DragDropModifiers.Shift)) move = true;
            else if (e.Modifiers.HasFlag(Windows.ApplicationModel.DataTransfer.DragDrop.DragDropModifiers.Control)) move = false;
            else move = string.Equals(System.IO.Path.GetPathRoot(sources[0]), System.IO.Path.GetPathRoot(target), StringComparison.OrdinalIgnoreCase);
            DropRequested?.Invoke(this, new DropRequest { Sources = sources, TargetFolder = target, Move = move });
        }
        catch (Exception ex)
        {
            App.ShowError(ex.Message);
        }
        finally
        {
            deferral.Complete();
        }
    }
}
