using System.Collections.ObjectModel;
using Microsoft.UI.Dispatching;
using Microsoft.UI.Windowing;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Input;
using Microsoft.UI.Xaml.Media;
using Nexus.Core;
using NexusFiles.Controls;
using NexusFiles.Services;
using NexusFiles.ViewModels;
using Windows.ApplicationModel.DataTransfer;
using Windows.Graphics;
using Windows.System;

namespace NexusFiles;

public sealed partial class MainWindow : Window
{
    private readonly List<TabViewModel> _tabs = [];
    private readonly Stack<TabState> _closedTabs = new();
    private readonly ObservableCollection<SidebarItem> _pins = [];
    private TabViewModel? _current;
    private List<PathSegment> _crumbs = [];
    private readonly DispatcherQueueTimer _jobsTimer;
    private readonly DispatcherQueueTimer _toastTimer;
    private readonly DispatcherQueueTimer _errorTimer;
    private readonly HashSet<ulong> _reportedJobs = [];
    private SessionState _session;
    private double _sidebarWidth = 240, _inspectorWidth = 320;
    private RectInt32? _normalBounds;
    private bool _suppressTabEvents;
    private bool _settingsLoading;

    public MainWindow(string? startPath)
    {
        InitializeComponent();
        App.WindowHandle = WinRT.Interop.WindowNative.GetWindowHandle(this);
        SystemBackdrop = new MicaBackdrop();
        ExtendsContentIntoTitleBar = true;
        SetTitleBar(DragRegion);
        AppWindow.SetIcon(System.IO.Path.Combine(AppContext.BaseDirectory, "Assets", "NexusFiles.ico"));
        AppWindow.TitleBar.PreferredHeightOption = TitleBarHeightOption.Tall;

        _jobsTimer = DispatcherQueue.CreateTimer();
        _jobsTimer.Interval = TimeSpan.FromMilliseconds(250);
        _jobsTimer.Tick += (_, _) => PollJobs();
        _toastTimer = DispatcherQueue.CreateTimer();
        _toastTimer.Interval = TimeSpan.FromSeconds(8);
        _toastTimer.IsRepeating = false;
        _toastTimer.Tick += (_, _) => UndoBar.IsOpen = false;
        _errorTimer = DispatcherQueue.CreateTimer();
        _errorTimer.Interval = TimeSpan.FromSeconds(10);
        _errorTimer.IsRepeating = false;
        _errorTimer.Tick += (_, _) => ErrorBar.IsOpen = false;

        ApplyTheme(CoreHost.Settings.Theme);
        WirePanes();
        WireSplitters();
        RegisterAccelerators();
        OmnibarView.Activated += OnOmniActivated;
        OmnibarView.PathEntered += (_, e) => _ = OpenPathAsync(e.path, e.target);
        OmnibarView.Closed += (_, _) => FocusActivePane();
        InspectorView.OpenFolderRequested += (_, p) => _ = ActivePane?.NavigateAsync(p);
        PinnedList.ItemsSource = _pins;

        _session = CoreHost.Core.LoadSession();
        if (_session.SidebarWidth > 0) _sidebarWidth = _session.SidebarWidth;
        if (_session.InspectorWidth > 0) _inspectorWidth = _session.InspectorWidth;
        RestoreWindowPlacement();
        LoadSidebar();
        RestoreTabs(startPath);
        Closed += OnClosed;
        AppWindow.Changed += (_, _) =>
        {
            UpdateTitleBarInset();
            if (AppWindow.Presenter is OverlappedPresenter { State: OverlappedPresenterState.Restored })
                _normalBounds = new RectInt32(AppWindow.Position.X, AppWindow.Position.Y, AppWindow.Size.Width, AppWindow.Size.Height);
        };
        RootGrid.Loaded += (_, _) => UpdateTitleBarInset();
    }

    private PaneViewModel? ActivePane => _current?.ActivePane;
    private FilePane ActivePaneView => _current?.RightIsActive == true ? RightPane : LeftPane;

    // ---- theme & title bar -------------------------------------------------

    private void ApplyTheme(ThemePreference pref)
    {
        var theme = pref switch
        {
            ThemePreference.Light => ElementTheme.Light,
            ThemePreference.Dark => ElementTheme.Dark,
            _ => ElementTheme.Default,
        };
        RootGrid.RequestedTheme = theme;
        Res.CurrentTheme = theme == ElementTheme.Default ? RootGrid.ActualTheme : theme;
        var dark = Res.CurrentTheme == ElementTheme.Dark;
        var tb = AppWindow.TitleBar;
        tb.ButtonBackgroundColor = Microsoft.UI.Colors.Transparent;
        tb.ButtonInactiveBackgroundColor = Microsoft.UI.Colors.Transparent;
        tb.ButtonForegroundColor = dark ? Microsoft.UI.Colors.White : Microsoft.UI.Colors.Black;
        tb.ButtonHoverBackgroundColor = dark ? Windows.UI.Color.FromArgb(40, 255, 255, 255) : Windows.UI.Color.FromArgb(24, 0, 0, 0);
    }

    private void UpdateTitleBarInset()
    {
        if (RootGrid.XamlRoot == null) return;
        var scale = RootGrid.XamlRoot.RasterizationScale;
        DragRegion.MinWidth = Math.Max(120, AppWindow.TitleBar.RightInset / scale + 40);
    }

    // ---- tabs ----------------------------------------------------------------

    private void RestoreTabs(string? startPath)
    {
        var settings = CoreHost.Settings;
        if (settings.RestoreSession && _session.Tabs.Count > 0)
        {
            foreach (var t in _session.Tabs)
            {
                var left = new PaneViewModel(t.Left.Path, t.Left.ViewMode, t.Left.Sort);
                var right = t.Right is { } r ? new PaneViewModel(r.Path, r.ViewMode, r.Sort) : null;
                AddTab(new TabViewModel(left, right, t.RightIsActive), select: false);
            }
            SidebarColumn.Width = new GridLength(Math.Clamp(_sidebarWidth, 160, 480));
            InspectorColumn.Width = new GridLength(Math.Clamp(_inspectorWidth, 240, 640));
            SetSidebarVisible(_session.SidebarVisible);
            SetInspectorVisible(_session.InspectorVisible);
            SelectTab(_tabs[(int)Math.Min(_session.ActiveTab, (uint)_tabs.Count - 1)]);
        }
        if (startPath != null)
        {
            NewTab(startPath);
        }
        else if (_tabs.Count == 0)
        {
            NewTab(CoreHost.HomePath);
        }
        foreach (var t in _tabs)
        {
            _ = t.Left.RefreshAsync();
            if (t.Right != null) _ = t.Right.RefreshAsync();
        }
    }

    private TabViewModel NewTab(string path, bool select = true)
    {
        var view = CoreHost.Settings.DefaultView;
        var tab = new TabViewModel(new PaneViewModel(path, view, CoreHost.DefaultSort));
        AddTab(tab, select);
        _ = tab.Left.NavigateAsync(path, addToHistory: false);
        return tab;
    }

    private void AddTab(TabViewModel tab, bool select)
    {
        _tabs.Add(tab);
        var item = new TabViewItem
        {
            Header = tab.Header,
            Tag = tab,
            IconSource = new FontIconSource { Glyph = Icons.Folder },
        };
        tab.PropertyChanged += (_, e) =>
        {
            if (e.PropertyName == nameof(TabViewModel.Header))
            {
                item.Header = tab.Header;
                ToolTipService.SetToolTip(item, tab.ActivePane.CurrentPath);
                if (tab == _current) UpdateChrome();
            }
        };
        _suppressTabEvents = true;
        TabStrip.TabItems.Add(item);
        _suppressTabEvents = false;
        if (select) SelectTab(tab);
    }

    private void SelectTab(TabViewModel tab)
    {
        var item = TabStrip.TabItems.OfType<TabViewItem>().FirstOrDefault(i => i.Tag == tab);
        if (item != null && TabStrip.SelectedItem != item) TabStrip.SelectedItem = item;
        else ShowTab(tab);
    }

    private void ShowTab(TabViewModel tab)
    {
        if (_current != null) _current.PropertyChanged -= OnCurrentTabChanged;
        _current = tab;
        _current.PropertyChanged += OnCurrentTabChanged;
        LeftPane.ViewModel = tab.Left;
        RightPane.ViewModel = tab.Right;
        ApplyDualLayout();
        UpdateChrome();
        FocusActivePane();
    }

    private void OnCurrentTabChanged(object? sender, System.ComponentModel.PropertyChangedEventArgs e)
    {
        if (e.PropertyName is nameof(TabViewModel.Right) or nameof(TabViewModel.ActivePane))
        {
            RightPane.ViewModel = _current?.Right;
            ApplyDualLayout();
            UpdateChrome();
        }
    }

    private void OnTabSelectionChanged(object sender, SelectionChangedEventArgs e)
    {
        if (_suppressTabEvents) return;
        if (TabStrip.SelectedItem is TabViewItem { Tag: TabViewModel tab } && tab != _current) ShowTab(tab);
    }

    private void OnAddTab(TabView sender, object args) => NewTab(ActivePane?.CurrentPath ?? CoreHost.HomePath);

    private void OnTabCloseRequested(TabView sender, TabViewTabCloseRequestedEventArgs args)
    {
        if (args.Tab.Tag is TabViewModel tab) CloseTab(tab);
    }

    private void OnTabItemsChanged(TabView sender, Windows.Foundation.Collections.IVectorChangedEventArgs args)
    {
        if (_suppressTabEvents) return;
        // Keep our list in the user's drag-reordered order.
        var ordered = TabStrip.TabItems.OfType<TabViewItem>().Select(i => i.Tag).OfType<TabViewModel>().ToList();
        if (ordered.Count == _tabs.Count)
        {
            _tabs.Clear();
            _tabs.AddRange(ordered);
        }
    }

    private void CloseTab(TabViewModel tab)
    {
        if (_tabs.Count == 1)
        {
            Close();
            return;
        }
        var index = _tabs.IndexOf(tab);
        _closedTabs.Push(tab.ToState());
        var item = TabStrip.TabItems.OfType<TabViewItem>().First(i => i.Tag == tab);
        _suppressTabEvents = true;
        TabStrip.TabItems.Remove(item);
        _suppressTabEvents = false;
        _tabs.Remove(tab);
        if (tab == _current) SelectTab(_tabs[Math.Min(index, _tabs.Count - 1)]);
        tab.Dispose();
    }

    private void CycleTab(int delta)
    {
        if (_current == null || _tabs.Count < 2) return;
        var i = (_tabs.IndexOf(_current) + delta + _tabs.Count) % _tabs.Count;
        SelectTab(_tabs[i]);
    }

    private void ReopenClosedTab()
    {
        if (_closedTabs.Count == 0) return;
        var t = _closedTabs.Pop();
        var left = new PaneViewModel(t.Left.Path, t.Left.ViewMode, t.Left.Sort);
        var right = t.Right is { } r ? new PaneViewModel(r.Path, r.ViewMode, r.Sort) : null;
        var tab = new TabViewModel(left, right, t.RightIsActive);
        AddTab(tab, select: true);
        _ = left.RefreshAsync();
        if (right != null) _ = right.RefreshAsync();
    }

    // ---- panes ---------------------------------------------------------------

    private void WirePanes()
    {
        foreach (var pane in new[] { LeftPane, RightPane })
        {
            pane.Activated += (s, _) =>
            {
                if (_current == null || ((FilePane)s!).ViewModel is not { } vm) return;
                _current.Activate(vm);
                UpdateChrome();
            };
            pane.SelectionChanged += (s, _) =>
            {
                if (s == ActivePaneView) UpdateSelectionInfo();
            };
            pane.OpenRequested += (_, e) => _ = OpenItemAsync(e.item, e.target);
            pane.CommandInvoked += (_, id) => _ = ExecuteAsync(id);
            pane.DropRequested += (_, d) => StartTransfer(d.Sources, d.TargetFolder, d.Move);
        }
    }

    private void ApplyDualLayout()
    {
        var dual = _current?.IsDual == true;
        RightPane.Visibility = dual ? Visibility.Visible : Visibility.Collapsed;
        PaneSplitter.Visibility = dual ? Visibility.Visible : Visibility.Collapsed;
        RightColumn.Width = dual ? new GridLength(1, GridUnitType.Star) : new GridLength(0);
        LeftColumn.Width = new GridLength(1, GridUnitType.Star);
        DualToggle.IsChecked = dual;
    }

    private void ToggleDualPane()
    {
        if (_current == null) return;
        if (_current.IsDual)
        {
            _current.Activate(_current.Left);
            _current.Right = null;
        }
        else
        {
            var src = _current.Left;
            var right = new PaneViewModel(src.CurrentPath, src.ViewMode, src.Sort);
            _current.Right = right;
            _ = right.NavigateAsync(src.CurrentPath, addToHistory: false);
        }
        FocusActivePane();
    }

    private void SwitchPane()
    {
        if (_current?.Right == null) return;
        _current.Activate(_current.RightIsActive ? _current.Left : _current.Right);
        UpdateChrome();
        FocusActivePane();
    }

    private void FocusActivePane() => DispatcherQueue.TryEnqueue(() => ActivePaneView.FocusItems());

    private async Task OpenItemAsync(FileItem item, OpenTarget target)
    {
        if (!item.IsDir && !item.Entry.IsSymlink)
        {
            Shell.Open(item.FullPath);
            return;
        }
        if (item.IsDir) await OpenPathAsync(item.FullPath, target);
        else Shell.Open(item.FullPath);
    }

    private async Task OpenPathAsync(string path, OpenTarget target, string? selectAfter = null)
    {
        if (File.Exists(path) && !Directory.Exists(path))
        {
            Shell.Open(path);
            return;
        }
        switch (target)
        {
            case OpenTarget.NewTab:
                var tab = NewTab(path);
                if (selectAfter != null) tab.Left.PendingSelection.Add(selectAfter);
                break;
            case OpenTarget.OtherPane when _current != null:
                if (_current.Right == null) ToggleDualPane();
                var other = _current.OtherPane!;
                _current.Activate(other);
                if (selectAfter != null) other.PendingSelection.Add(selectAfter);
                await other.NavigateAsync(path);
                break;
            default:
                if (ActivePane == null) return;
                if (selectAfter != null) ActivePane.PendingSelection.Add(selectAfter);
                await ActivePane.NavigateAsync(path);
                break;
        }
    }

    // ---- chrome: address bar, status, inspector -------------------------------

    private void UpdateChrome()
    {
        var pane = ActivePane;
        if (pane == null) return;
        Title = $"{pane.Title} - Nexus Files";
        BackButton.IsEnabled = pane.CanGoBack;
        ForwardButton.IsEnabled = pane.CanGoForward;
        try
        {
            _crumbs = NexusNative.PathSegments(pane.CurrentPath);
            var drives = DrivesList.ItemsSource as IEnumerable<SidebarItem>;
            Crumbs.ItemsSource = _crumbs.Select((c, i) =>
            {
                if (i == 0 && drives?.FirstOrDefault(d => string.Equals(d.Path, c.Path, StringComparison.OrdinalIgnoreCase)) is { } drive)
                    return drive.Name;
                return c.Name;
            }).ToList();
        }
        catch
        {
            Crumbs.ItemsSource = new List<string> { pane.CurrentPath };
        }
        PaneText.Text = _current?.IsDual == true ? (_current.RightIsActive ? "Pane phải" : "Pane trái") : "";
        UpdateFreeSpace(pane.CurrentPath);
        UpdateSelectionInfo();
    }

    private void UpdateSelectionInfo()
    {
        var pane = ActivePane;
        if (pane == null) return;
        var selected = ActivePaneView.SelectedItems;
        CountText.Text = pane.Summary;
        if (selected.Count > 0)
        {
            var bytes = (ulong)selected.Where(s => !s.IsDir).Sum(s => (long)s.Entry.Size);
            SelectionText.Text = bytes > 0 ? $"{selected.Count} đang chọn · {Format.Size(bytes)}" : $"{selected.Count} đang chọn";
        }
        else
        {
            SelectionText.Text = "";
        }
        BackButton.IsEnabled = pane.CanGoBack;
        ForwardButton.IsEnabled = pane.CanGoForward;
        if (InspectorView.Visibility == Visibility.Visible) InspectorView.Show(selected, pane.CurrentPath);
    }

    private void UpdateFreeSpace(string path)
    {
        try
        {
            var root = System.IO.Path.GetPathRoot(path);
            if (string.IsNullOrEmpty(root)) return;
            var d = new System.IO.DriveInfo(root);
            FreeText.Text = $"{root.TrimEnd('\\')} còn {Format.Size((ulong)d.AvailableFreeSpace)}";
        }
        catch
        {
            FreeText.Text = "";
        }
    }

    private void OnCrumbClicked(BreadcrumbBar sender, BreadcrumbBarItemClickedEventArgs args)
    {
        if (args.Index >= 0 && args.Index < _crumbs.Count) _ = ActivePane?.NavigateAsync(_crumbs[args.Index].Path);
    }

    private void OnAddressBarTapped(object sender, TappedRoutedEventArgs e)
    {
        // Clicking a crumb navigates; clicking the empty part edits the path.
        if (e.OriginalSource is FrameworkElement fe && FindParent<BreadcrumbBarItem>(fe) != null) return;
        OpenOmnibar(ActivePane?.CurrentPath ?? "", selectAll: true);
    }

    private static T? FindParent<T>(DependencyObject? o) where T : DependencyObject
    {
        while (o != null)
        {
            if (o is T t) return t;
            o = VisualTreeHelper.GetParent(o);
        }
        return null;
    }

    // ---- sidebar ---------------------------------------------------------------

    private void LoadSidebar()
    {
        _pins.Clear();
        foreach (var p in CoreHost.Core.Pinned()) _pins.Add(SidebarItem.FromPlace(p));
        PinHint.Visibility = _pins.Count == 0 ? Visibility.Visible : Visibility.Collapsed;
        LibraryList.ItemsSource = NexusNative.KnownFolders().Select(SidebarItem.FromPlace).ToList();
        DrivesList.ItemsSource = NexusNative.Drives().Select(SidebarItem.FromDrive).ToList();
    }

    private void OnSidebarClick(object sender, ItemClickEventArgs e)
    {
        if (e.ClickedItem is SidebarItem item)
        {
            var ctrl = Microsoft.UI.Input.InputKeyboardSource.GetKeyStateForCurrentThread(VirtualKey.Control)
                .HasFlag(Windows.UI.Core.CoreVirtualKeyStates.Down);
            _ = OpenPathAsync(item.Path, ctrl ? OpenTarget.NewTab : OpenTarget.Here);
        }
    }

    private void OnPinsReordered(ListViewBase sender, DragItemsCompletedEventArgs args)
    {
        for (int i = 0; i < _pins.Count; i++) CoreHost.Core.MovePin(_pins[i].Path, (uint)i);
    }

    private void OnSidebarContext(UIElement sender, ContextRequestedEventArgs e)
    {
        if ((e.OriginalSource as FrameworkElement)?.DataContext is not SidebarItem item) return;
        var menu = new MenuFlyout();
        void Add(string text, string glyph, Action action)
        {
            var mi = new MenuFlyoutItem { Text = text, Icon = new FontIcon { Glyph = glyph } };
            mi.Click += (_, _) => action();
            menu.Items.Add(mi);
        }
        Add("Mở trong tab mới", "\uECCD", () => _ = OpenPathAsync(item.Path, OpenTarget.NewTab));
        Add("Mở ở pane bên kia", "\uE89F", () => _ = OpenPathAsync(item.Path, OpenTarget.OtherPane));
        if (item.IsPinned) Add("Bỏ ghim", "\uE77A", () => TogglePin(item.Path));
        else if (!CoreHost.Core.IsPinned(item.Path)) Add("Ghim", "\uE718", () => TogglePin(item.Path));
        Add("Thuộc tính", "\uE946", () => Shell.Properties(item.Path));
        if (e.TryGetPosition(sender, out var pt)) menu.ShowAt(sender, pt);
        e.Handled = true;
    }

    private void TogglePin(string path)
    {
        try
        {
            if (CoreHost.Core.IsPinned(path))
            {
                CoreHost.Core.Unpin(path);
                ShowToast($"Đã bỏ ghim \"{System.IO.Path.GetFileName(path.TrimEnd('\\'))}\"", undoable: false);
            }
            else
            {
                CoreHost.Core.Pin(path);
                ShowToast($"Đã ghim \"{System.IO.Path.GetFileName(path.TrimEnd('\\'))}\"", undoable: false);
            }
        }
        catch (Exception e)
        {
            ShowError(Format.Error(e));
        }
        LoadSidebar();
    }

    private void SetSidebarVisible(bool visible)
    {
        Sidebar.Visibility = visible ? Visibility.Visible : Visibility.Collapsed;
        SidebarSplitter.Visibility = Sidebar.Visibility;
        SidebarColumn.Width = visible ? new GridLength(Math.Max(160, _sidebarWidth)) : new GridLength(0);
    }

    private void SetInspectorVisible(bool visible)
    {
        InspectorView.Visibility = visible ? Visibility.Visible : Visibility.Collapsed;
        InspectorSplitter.Visibility = InspectorView.Visibility;
        InspectorColumn.Width = visible ? new GridLength(Math.Max(240, _inspectorWidth)) : new GridLength(0);
        InspectorToggle.IsChecked = visible;
        if (!visible) InspectorView.StopMedia();
        else UpdateSelectionInfo();
    }

    private void WireSplitters()
    {
        SidebarSplitter.Dragged += (_, dx) =>
        {
            _sidebarWidth = Math.Clamp(SidebarColumn.ActualWidth + dx, 160, 480);
            SidebarColumn.Width = new GridLength(_sidebarWidth);
        };
        InspectorSplitter.Dragged += (_, dx) =>
        {
            _inspectorWidth = Math.Clamp(InspectorColumn.ActualWidth - dx, 240, 640);
            InspectorColumn.Width = new GridLength(_inspectorWidth);
        };
        PaneSplitter.Dragged += (_, dx) =>
        {
            var total = LeftColumn.ActualWidth + RightColumn.ActualWidth;
            if (total <= 0) return;
            var left = Math.Clamp(LeftColumn.ActualWidth + dx, 220, total - 220);
            LeftColumn.Width = new GridLength(left / total, GridUnitType.Star);
            RightColumn.Width = new GridLength((total - left) / total, GridUnitType.Star);
        };
    }

    // ---- keyboard --------------------------------------------------------------

    private void RegisterAccelerators()
    {
        void Add(VirtualKey key, VirtualKeyModifiers mods, string command, bool allowInTextBox = false)
        {
            var acc = new KeyboardAccelerator { Key = key, Modifiers = mods };
            acc.Invoked += (_, args) =>
            {
                if (OmnibarView.IsOpen && command != "app.palette") return;
                if (!allowInTextBox && FocusManager.GetFocusedElement(RootGrid.XamlRoot) is TextBox) return;
                args.Handled = true;
                _ = ExecuteAsync(command);
            };
            RootGrid.KeyboardAccelerators.Add(acc);
        }
        const VirtualKeyModifiers C = VirtualKeyModifiers.Control, S = VirtualKeyModifiers.Shift, A = VirtualKeyModifiers.Menu, N = VirtualKeyModifiers.None;
        Add(VirtualKey.T, C, "tab.new", true);
        Add(VirtualKey.W, C, "tab.close", true);
        Add(VirtualKey.T, C | S, "tab.reopen", true);
        Add(VirtualKey.Tab, C, "tab.next", true);
        Add(VirtualKey.Tab, C | S, "tab.previous", true);
        Add(VirtualKey.L, C, "nav.omnibar", true);
        Add(VirtualKey.F, C, "nav.search", true);
        Add(VirtualKey.E, C, "nav.search", true);
        Add(VirtualKey.G, C, "nav.goto", true);
        Add(VirtualKey.P, C | S, "app.palette", true);
        Add(VirtualKey.F5, N, "nav.refresh", true);
        Add(VirtualKey.Left, A, "nav.back");
        Add(VirtualKey.Right, A, "nav.forward");
        Add(VirtualKey.Up, A, "nav.up");
        Add(VirtualKey.Number1, C, "view.details");
        Add(VirtualKey.Number2, C, "view.list");
        Add(VirtualKey.Number3, C, "view.icons");
        Add(VirtualKey.D, C | S, "view.dual_pane");
        Add(VirtualKey.I, C, "view.inspector");
        Add(VirtualKey.B, C, "view.sidebar");
        Add(VirtualKey.H, C, "view.hidden");
        Add(VirtualKey.D, C, "nav.pin");
        Add(VirtualKey.Z, C, "edit.undo");
        Add(VirtualKey.N, C | S, "file.new_folder");
        Add((VirtualKey)192, C, "file.open_terminal"); // Ctrl+`
    }

    private void OpenOmnibar(string text, bool selectAll) =>
        OmnibarView.Show(text, ActivePane?.CurrentPath ?? CoreHost.HomePath, selectAll);

    private void OnOmniActivated(object? sender, OmniActivation a)
    {
        var item = a.Item;
        switch (item.Kind)
        {
            case OmniItemKind.Command when item.CommandId != null:
                _ = ExecuteAsync(item.CommandId);
                break;
            case OmniItemKind.File when item.Path != null:
                if (a.Reveal || a.Target != OpenTarget.Here)
                    _ = OpenPathAsync(System.IO.Path.GetDirectoryName(item.Path)!, a.Target == OpenTarget.Here ? OpenTarget.Here : a.Target, item.Path);
                else
                    Shell.Open(item.Path);
                break;
            case OmniItemKind.Folder or OmniItemKind.Place when item.Path != null:
                if (a.Reveal && System.IO.Path.GetDirectoryName(item.Path) is { } parent)
                    _ = OpenPathAsync(parent, a.Target, item.Path);
                else
                    _ = OpenPathAsync(item.Path, a.Target);
                break;
        }
    }

    // ---- commands ----------------------------------------------------------------

    private List<string> SelectedPaths() => ActivePaneView.SelectedItems.Select(i => i.FullPath).ToList();

    private void OnNavButton(object sender, RoutedEventArgs e)
    {
        if (sender is FrameworkElement { Tag: string id }) _ = ExecuteAsync(id);
    }

    private void OnMenuCommand(object sender, RoutedEventArgs e) => OnNavButton(sender, e);

    private void OnToggleCommand(object sender, RoutedEventArgs e) => OnNavButton(sender, e);

    /// <summary>Runs a command by its id (the same ids the core lists in the palette).</summary>
    private async Task ExecuteAsync(string id)
    {
        var pane = ActivePane;
        if (pane == null) return;
        try
        {
            switch (id)
            {
                case "nav.back": await pane.GoBackAsync(); break;
                case "nav.forward": await pane.GoForwardAsync(); break;
                case "nav.up": await pane.GoUpAsync(); break;
                case "nav.refresh": LoadSidebar(); await pane.RefreshAsync(); break;
                case "nav.omnibar": OpenOmnibar(pane.CurrentPath, selectAll: true); break;
                case "nav.search": OpenOmnibar("", selectAll: false); break;
                case "nav.goto": OpenOmnibar("@", selectAll: false); break;
                case "app.palette": OpenOmnibar(">", selectAll: false); break;
                case "nav.pin": TogglePin(pane.CurrentPath); break;
                case "nav.pin_selected":
                    foreach (var p in ActivePaneView.SelectedItems.Where(i => i.IsDir)) TogglePin(p.FullPath);
                    break;

                case "tab.new": NewTab(pane.CurrentPath); break;
                case "tab.close": if (_current != null) CloseTab(_current); break;
                case "tab.reopen": ReopenClosedTab(); break;
                case "tab.next": CycleTab(1); break;
                case "tab.previous": CycleTab(-1); break;
                case "tab.duplicate": NewTab(pane.CurrentPath); break;

                case "view.details": pane.ViewMode = ViewMode.Details; break;
                case "view.list": pane.ViewMode = ViewMode.List; break;
                case "view.icons": pane.ViewMode = ViewMode.Icons; break;
                case "view.dual_pane": ToggleDualPane(); break;
                case "view.switch_pane": SwitchPane(); break;
                case "view.inspector": SetInspectorVisible(InspectorView.Visibility != Visibility.Visible); break;
                case "view.sidebar": SetSidebarVisible(Sidebar.Visibility != Visibility.Visible); break;
                case "view.hidden":
                    SaveSettings(CoreHost.Settings with { ShowHidden = !CoreHost.Settings.ShowHidden });
                    await RefreshAllPanesAsync();
                    break;

                case "sort.name": pane.SetSort(SortKey.Name); break;
                case "sort.modified": pane.SetSort(SortKey.Modified); break;
                case "sort.size": pane.SetSort(SortKey.Size); break;
                case "sort.type": pane.SetSort(SortKey.Type); break;

                case "app.theme_light": SetTheme(ThemePreference.Light); break;
                case "app.theme_dark": SetTheme(ThemePreference.Dark); break;
                case "app.theme_system": SetTheme(ThemePreference.System); break;
                case "app.jobs": JobsFlyout.ShowAt(JobsButton); break;
                case "app.history": await ShowHistoryAsync(); break;

                case "edit.select_all": ActivePaneView.SelectAll(); break;
                case "edit.invert_selection": ActivePaneView.InvertSelection(); break;
                case "edit.copy": await CopyToClipboardAsync(cut: false); break;
                case "edit.cut": await CopyToClipboardAsync(cut: true); break;
                case "edit.paste": await PasteAsync(); break;
                case "edit.undo": Undo(); break;

                case "file.open":
                    foreach (var item in ActivePaneView.SelectedItems.Take(10)) await OpenItemAsync(item, OpenTarget.Here);
                    break;
                case "file.new_folder": await CreateAsync(folder: true); break;
                case "file.new_text": await CreateAsync(folder: false); break;
                case "file.rename": await RenameAsync(); break;
                case "file.delete": await DeleteAsync(permanent: false); break;
                case "file.delete_permanent": await DeleteAsync(permanent: true); break;
                case "file.copy_path":
                    var paths = SelectedPaths();
                    var text = paths.Count > 0 ? string.Join(Environment.NewLine, paths.Select(p => $"\"{p}\"")) : pane.CurrentPath;
                    var pkg = new DataPackage();
                    pkg.SetText(text);
                    Clipboard.SetContent(pkg);
                    ShowToast(paths.Count > 1 ? $"Đã sao chép {paths.Count} đường dẫn" : "Đã sao chép đường dẫn", undoable: false);
                    break;
                case "file.properties": Shell.Properties(SelectedPaths().FirstOrDefault() ?? pane.CurrentPath); break;
                case "file.open_with":
                    if (SelectedPaths().FirstOrDefault() is { } f) Shell.OpenWith(f);
                    break;
                case "file.show_in_explorer": Shell.ShowInExplorer(SelectedPaths().FirstOrDefault() ?? pane.CurrentPath); break;
                case "file.open_terminal": Shell.OpenTerminal(pane.CurrentPath); break;
            }
        }
        catch (Exception e)
        {
            ShowError(Format.Error(e));
        }
        if (id.StartsWith("view.") || id.StartsWith("sort.")) UpdateChrome();
    }

    private async Task RefreshAllPanesAsync()
    {
        foreach (var t in _tabs)
        {
            await t.Left.RefreshAsync();
            if (t.Right != null) await t.Right.RefreshAsync();
        }
    }

    private async Task CopyToClipboardAsync(bool cut)
    {
        var items = ActivePaneView.SelectedItems;
        if (items.Count == 0) return;
        foreach (var t in _tabs.SelectMany(t => new[] { t.Left, t.Right }).OfType<PaneViewModel>())
            foreach (var i in t.Items.Where(i => i.IsCut)) i.IsCut = false;
        await FileClipboard.SetAsync(items.Select(i => i.FullPath).ToList(), cut);
        if (cut) foreach (var i in items) i.IsCut = true;
        ShowToast(cut ? $"Đã cắt {items.Count} mục, chọn nơi rồi nhấn Ctrl+V" : $"Đã copy {items.Count} mục", undoable: false);
    }

    private async Task PasteAsync()
    {
        var pane = ActivePane;
        if (pane == null) return;
        var (paths, cut) = await FileClipboard.GetAsync();
        if (paths.Count == 0) return;
        StartTransfer(paths, pane.CurrentPath, cut);
        if (cut) FileClipboard.Clear();
    }

    private void StartTransfer(List<string> sources, string target, bool move)
    {
        FileOperation op = move
            ? new FileOperation.Move(sources, target, ConflictPolicy.KeepBoth)
            : new FileOperation.Copy(sources, target, ConflictPolicy.KeepBoth);
        Enqueue(op);
    }

    private void Enqueue(FileOperation op)
    {
        try
        {
            CoreHost.Core.Enqueue(op);
            _jobsTimer.Start();
            PollJobs();
        }
        catch (Exception e)
        {
            ShowError(Format.Error(e));
        }
    }

    private async Task CreateAsync(bool folder)
    {
        var pane = ActivePane;
        if (pane == null || pane.HasError) return;
        var name = folder ? "Thư mục mới" : "Tệp mới.txt";
        FileOperation op = folder ? new FileOperation.NewFolder(pane.CurrentPath, name) : new FileOperation.NewFile(pane.CurrentPath, name);
        var status = await Task.Run(() => CoreHost.Core.Execute(op));
        var created = status.ResultPaths.FirstOrDefault();
        if (created == null) return;
        pane.PendingSelection.Add(created);
        await pane.RefreshAsync();
        UpdateUndo();
        // Like Explorer: let the user name it straight away.
        await RenamePathAsync(created, isDir: folder);
    }

    private async Task RenameAsync()
    {
        var item = ActivePaneView.SelectedItems.FirstOrDefault();
        if (item == null) return;
        await RenamePathAsync(item.FullPath, item.IsDir);
    }

    private async Task RenamePathAsync(string path, bool isDir)
    {
        var name = System.IO.Path.GetFileName(path);
        var box = new TextBox { Text = name, MinWidth = 380 };
        var error = new TextBlock { Foreground = Res.Brush("NxBad"), TextWrapping = TextWrapping.Wrap, Visibility = Visibility.Collapsed };
        var dialog = new ContentDialog
        {
            XamlRoot = RootGrid.XamlRoot,
            Title = "Đổi tên",
            Content = new StackPanel { Spacing = 8, Children = { box, error } },
            PrimaryButtonText = "Đổi tên",
            CloseButtonText = "Hủy",
            DefaultButton = ContentDialogButton.Primary,
            RequestedTheme = RootGrid.ActualTheme,
        };
        box.Loaded += (_, _) =>
        {
            box.Focus(FocusState.Programmatic);
            var dot = isDir ? -1 : name.LastIndexOf('.');
            box.Select(0, dot > 0 ? dot : name.Length);
        };
        string? newPath = null;
        dialog.PrimaryButtonClick += (_, args) =>
        {
            var newName = box.Text.Trim();
            if (newName == name) return;
            try
            {
                var st = CoreHost.Core.Execute(new FileOperation.Rename(path, newName));
                newPath = st.ResultPaths.FirstOrDefault();
            }
            catch (Exception ex)
            {
                error.Text = Format.Error(ex);
                error.Visibility = Visibility.Visible;
                args.Cancel = true;
            }
        };
        await dialog.ShowAsync();
        if (newPath != null && ActivePane is { } pane)
        {
            pane.PendingSelection.Add(newPath);
            await pane.RefreshAsync();
            UpdateUndo();
        }
        FocusActivePane();
    }

    private async Task DeleteAsync(bool permanent)
    {
        var paths = SelectedPaths();
        if (paths.Count == 0) return;
        var what = paths.Count == 1 ? $"\"{System.IO.Path.GetFileName(paths[0])}\"" : $"{paths.Count} mục";
        if (permanent || CoreHost.Settings.ConfirmTrash)
        {
            var dialog = new ContentDialog
            {
                XamlRoot = RootGrid.XamlRoot,
                Title = permanent ? "Xóa vĩnh viễn?" : "Xóa vào Thùng rác?",
                Content = permanent ? $"{what} sẽ bị xóa hẳn và không hoàn tác được." : $"{what} sẽ được chuyển vào Thùng rác. Bạn có thể hoàn tác bằng Ctrl+Z.",
                PrimaryButtonText = permanent ? "Xóa vĩnh viễn" : "Xóa",
                CloseButtonText = "Hủy",
                DefaultButton = permanent ? ContentDialogButton.Close : ContentDialogButton.Primary,
                RequestedTheme = RootGrid.ActualTheme,
            };
            if (await dialog.ShowAsync() != ContentDialogResult.Primary) return;
        }
        // Select the next item so the keyboard user can keep going.
        var pane = ActivePane!;
        var items = pane.Items;
        var lastIndex = items.Select((it, i) => (it, i)).Where(x => paths.Contains(x.it.FullPath)).Select(x => x.i).DefaultIfEmpty(-1).Max();
        var next = items.Skip(lastIndex + 1).FirstOrDefault(i => !paths.Contains(i.FullPath))
                   ?? items.Take(Math.Max(0, lastIndex)).LastOrDefault(i => !paths.Contains(i.FullPath));
        if (next != null) pane.PendingSelection.Add(next.FullPath);
        Enqueue(permanent ? new FileOperation.DeletePermanently(paths) : new FileOperation.Trash(paths));
    }

    private void Undo()
    {
        var title = CoreHost.Core.UndoTitle();
        var id = CoreHost.Core.Undo();
        if (id == null)
        {
            ShowToast("Không còn gì để hoàn tác", undoable: false);
            return;
        }
        _reportedJobs.Add(id.Value); // the undo itself reports through the poller below
        _jobsTimer.Start();
        ShowToast($"Đang hoàn tác: {title}", undoable: false);
        UpdateUndo();
    }

    private void OnUndoToast(object sender, RoutedEventArgs e)
    {
        UndoBar.IsOpen = false;
        Undo();
    }

    private void UpdateUndo()
    {
        var title = CoreHost.Core.UndoTitle();
        UndoButton.IsEnabled = title != null;
        ToolTipService.SetToolTip(UndoButton, title == null ? "Không có gì để hoàn tác" : $"Hoàn tác: {title} (Ctrl+Z)");
    }

    private async Task ShowHistoryAsync()
    {
        var list = new StackPanel { Spacing = 6 };
        foreach (var h in CoreHost.Core.History(50))
        {
            list.Children.Add(new TextBlock
            {
                Text = $"{Format.DateTimeFull(h.TimeMs)}  ·  {h.Title}{(h.Undone ? "  (đã hoàn tác)" : "")}",
                TextWrapping = TextWrapping.Wrap,
                FontSize = 13,
                Opacity = h.Undone ? 0.55 : 1,
            });
        }
        if (list.Children.Count == 0) list.Children.Add(new TextBlock { Text = "Chưa có thao tác nào." });
        var dialog = new ContentDialog
        {
            XamlRoot = RootGrid.XamlRoot,
            Title = "Lịch sử thao tác (30 ngày)",
            Content = new ScrollViewer { Content = list, MaxHeight = 420 },
            CloseButtonText = "Đóng",
            RequestedTheme = RootGrid.ActualTheme,
        };
        await dialog.ShowAsync();
    }

    // ---- jobs & notifications ---------------------------------------------------

    private void PollJobs()
    {
        var jobs = CoreHost.Core.Jobs();
        var running = jobs.Where(j => j.State is JobState.Running or JobState.Queued).ToList();
        if (running.Count > 0)
        {
            JobsButton.Visibility = Visibility.Visible;
            var j = running[0];
            var pct = j.BytesTotal > 0 ? 100.0 * j.BytesDone / j.BytesTotal : j.FilesTotal > 0 ? 100.0 * j.FilesDone / j.FilesTotal : 0;
            JobsText.Text = running.Count > 1 ? $"{running.Count} tác vụ · {j.Title} · {pct:0}%" : $"{j.Title} · {pct:0}%";
            JobsProgress.Value = pct;
        }
        else
        {
            JobsButton.Visibility = Visibility.Collapsed;
            _jobsTimer.Stop();
        }
        RenderJobsFlyout(jobs);
        foreach (var j in jobs.Where(j => j.State is not (JobState.Running or JobState.Queued)))
        {
            if (!_reportedJobs.Add(j.Id)) continue;
            ReportFinished(j);
        }
    }

    private void ReportFinished(JobStatus j)
    {
        UpdateUndo();
        switch (j.State)
        {
            case JobState.Completed:
                if (j.Undoable) ShowToast($"Xong: {j.Title}", undoable: true);
                break;
            case JobState.CompletedWithErrors:
                var first = j.Errors.FirstOrDefault();
                ShowError(first != null
                    ? $"{j.Title}: {j.Errors.Count} mục lỗi. {first.message} ({System.IO.Path.GetFileName(first.path)})"
                    : $"{j.Title}: bỏ qua {j.Skipped} mục đã có sẵn.");
                break;
            case JobState.Failed:
                var e = j.Errors.FirstOrDefault();
                ShowError(e != null ? $"{j.Title}: {e.message}" : $"{j.Title}: thất bại");
                break;
            case JobState.Cancelled:
                ShowToast($"Đã hủy: {j.Title}", undoable: j.Undoable);
                break;
        }
        // Select what was created/moved here (the watcher refreshes the folder).
        if (ActivePane is { } pane)
        {
            foreach (var p in j.ResultPaths.Where(p => string.Equals(System.IO.Path.GetDirectoryName(p), pane.CurrentPath, StringComparison.OrdinalIgnoreCase)))
                pane.PendingSelection.Add(p);
            if (pane.PendingSelection.Count > 0) _ = pane.RefreshAsync();
        }
    }

    private void RenderJobsFlyout(List<JobStatus> jobs)
    {
        JobsPanel.Children.Clear();
        foreach (var j in jobs.AsEnumerable().Reverse().Take(8))
        {
            var pct = j.BytesTotal > 0 ? 100.0 * j.BytesDone / j.BytesTotal : j.FilesTotal > 0 ? 100.0 * j.FilesDone / j.FilesTotal : 100;
            var panel = new StackPanel { Spacing = 4 };
            var row = new Grid { ColumnSpacing = 8 };
            row.ColumnDefinitions.Add(new ColumnDefinition { Width = new GridLength(1, GridUnitType.Star) });
            row.ColumnDefinitions.Add(new ColumnDefinition { Width = GridLength.Auto });
            row.Children.Add(new TextBlock { Text = j.Title, TextTrimming = TextTrimming.CharacterEllipsis, FontSize = 13 });
            if (j.State is JobState.Running or JobState.Queued)
            {
                var cancel = new Button { Content = "Hủy", Padding = new Thickness(8, 2, 8, 2), FontSize = 12 };
                var id = j.Id;
                cancel.Click += (_, _) => CoreHost.Core.CancelJob(id);
                Grid.SetColumn(cancel, 1);
                row.Children.Add(cancel);
            }
            panel.Children.Add(row);
            panel.Children.Add(new ProgressBar { Value = pct, Maximum = 100 });
            var state = j.State switch
            {
                JobState.Running => $"{j.FilesDone}/{j.FilesTotal} mục · {Format.Size(j.BytesDone)} / {Format.Size(j.BytesTotal)}",
                JobState.Queued => "Đang chờ",
                JobState.Completed => "Xong",
                JobState.CompletedWithErrors => $"Xong, {j.Errors.Count} lỗi, bỏ qua {j.Skipped}",
                JobState.Failed => "Lỗi: " + (j.Errors.FirstOrDefault()?.message ?? ""),
                _ => "Đã hủy",
            };
            panel.Children.Add(new TextBlock { Text = state, FontSize = 12, Foreground = Res.Brush("NxText3"), TextWrapping = TextWrapping.Wrap });
            JobsPanel.Children.Add(panel);
        }
    }

    private void ShowToast(string message, bool undoable)
    {
        UndoBar.Message = message;
        UndoBar.ActionButton.Visibility = undoable ? Visibility.Visible : Visibility.Collapsed;
        UndoBar.IsOpen = true;
        _toastTimer.Stop();
        _toastTimer.Start();
    }

    public void ShowError(string message)
    {
        ErrorBar.Message = message;
        ErrorBar.IsOpen = true;
        _errorTimer.Stop();
        _errorTimer.Start();
    }

    // ---- settings -----------------------------------------------------------------

    private void OnSettingsOpening(object? sender, object e)
    {
        _settingsLoading = true;
        var s = CoreHost.Settings;
        ThemeChoice.SelectedIndex = s.Theme switch { ThemePreference.Light => 1, ThemePreference.Dark => 2, _ => 0 };
        HiddenSwitch.IsOn = s.ShowHidden;
        ConfirmSwitch.IsOn = s.ConfirmTrash;
        RestoreSwitch.IsOn = s.RestoreSession;
        AboutText.Text = $"Nexus Files {typeof(App).Assembly.GetName().Version?.ToString(3)} · lõi {NexusNative.CoreVersion()}\nDữ liệu: {CoreHost.DataDir}";
        _settingsLoading = false;
    }

    private void OnThemeChoice(object sender, SelectionChangedEventArgs e)
    {
        if (_settingsLoading) return;
        SetTheme(ThemeChoice.SelectedIndex switch { 1 => ThemePreference.Light, 2 => ThemePreference.Dark, _ => ThemePreference.System });
    }

    private void SetTheme(ThemePreference pref)
    {
        SaveSettings(CoreHost.Settings with { Theme = pref });
        ApplyTheme(pref);
        _ = RefreshAllPanesAsync(); // re-create rows so icon colours follow the theme
        LoadSidebar();
    }

    private async void OnSettingSwitch(object sender, RoutedEventArgs e)
    {
        if (_settingsLoading) return;
        var before = CoreHost.Settings;
        SaveSettings(before with { ShowHidden = HiddenSwitch.IsOn, ConfirmTrash = ConfirmSwitch.IsOn, RestoreSession = RestoreSwitch.IsOn });
        if (before.ShowHidden != HiddenSwitch.IsOn) await RefreshAllPanesAsync();
    }

    private static void SaveSettings(Settings s)
    {
        CoreHost.Settings = s;
        try
        {
            CoreHost.Core.SaveSettings(s);
        }
        catch (Exception e)
        {
            App.ShowError(Format.Error(e));
        }
    }

    // ---- session ------------------------------------------------------------------

    private void RestoreWindowPlacement()
    {
        if (_session.Window is not { } w) return;
        // Only restore onto a visible monitor.
        var area = DisplayArea.GetFromRect(new RectInt32(w.X, w.Y, w.Width, w.Height), DisplayAreaFallback.None);
        if (area != null && w.Width > 400 && w.Height > 300)
            AppWindow.MoveAndResize(new RectInt32(w.X, w.Y, w.Width, w.Height));
        if (w.Maximized && AppWindow.Presenter is OverlappedPresenter p) p.Maximize();
    }

    private void OnClosed(object sender, WindowEventArgs args)
    {
        try
        {
            var maximized = AppWindow.Presenter is OverlappedPresenter { State: OverlappedPresenterState.Maximized };
            var b = _normalBounds ?? new RectInt32(AppWindow.Position.X, AppWindow.Position.Y, AppWindow.Size.Width, AppWindow.Size.Height);
            var active = _current == null ? 0 : _tabs.IndexOf(_current);
            var session = new SessionState(
                _tabs.Select(t => t.ToState()).ToList(),
                (uint)Math.Max(0, active),
                Sidebar.Visibility == Visibility.Visible,
                InspectorView.Visibility == Visibility.Visible,
                _sidebarWidth,
                _inspectorWidth,
                new WindowPlacement(b.X, b.Y, b.Width, b.Height, maximized));
            CoreHost.Core.SaveSession(session);
        }
        catch
        {
            // Never block closing on a failed save.
        }
        foreach (var t in _tabs) t.Dispose();
    }
}
