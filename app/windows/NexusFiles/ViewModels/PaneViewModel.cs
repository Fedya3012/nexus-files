using System.Collections.ObjectModel;
using CommunityToolkit.Mvvm.ComponentModel;
using Microsoft.UI.Dispatching;
using Nexus.Core;
using NexusFiles.Services;

namespace NexusFiles.ViewModels;

/// <summary>State of one file pane: folder, items, history, view and sort.</summary>
public sealed class PaneViewModel : ObservableObject, IDisposable
{
    private readonly DispatcherQueue _dispatcher;
    private readonly Stack<string> _back = new();
    private readonly Stack<string> _forward = new();
    private ObservableCollection<FileItem> _items = [];
    private List<FileEntry> _entries = [];
    private string _currentPath = "";
    private ViewMode _viewMode;
    private SortOptions _sort;
    private bool _isActive;
    private bool _isLoading;
    private string? _errorText;
    private int _generation;
    private FileSystemWatcher? _watcher;
    private DispatcherQueueTimer? _refreshTimer;

    public PaneViewModel(string path, ViewMode viewMode, SortOptions sort)
    {
        _dispatcher = DispatcherQueue.GetForCurrentThread();
        _currentPath = path;
        _viewMode = viewMode;
        _sort = sort;
    }

    /// <summary>Raised after a folder finished loading (UI restores selection/focus).</summary>
    public event EventHandler? Loaded;

    public ObservableCollection<FileItem> Items
    {
        get => _items;
        private set => SetProperty(ref _items, value);
    }

    public string CurrentPath
    {
        get => _currentPath;
        private set
        {
            if (SetProperty(ref _currentPath, value)) OnPropertyChanged(nameof(Title));
        }
    }

    public string Title
    {
        get
        {
            var name = System.IO.Path.GetFileName(_currentPath.TrimEnd('\\', '/'));
            return string.IsNullOrEmpty(name) ? _currentPath : name;
        }
    }

    public string Summary => Format.Items(_items.Count);

    public ViewMode ViewMode
    {
        get => _viewMode;
        set => SetProperty(ref _viewMode, value);
    }

    public SortOptions Sort
    {
        get => _sort;
        private set => SetProperty(ref _sort, value);
    }

    public bool IsActive
    {
        get => _isActive;
        set => SetProperty(ref _isActive, value);
    }

    public bool IsLoading
    {
        get => _isLoading;
        private set => SetProperty(ref _isLoading, value);
    }

    public string? ErrorText
    {
        get => _errorText;
        private set
        {
            if (SetProperty(ref _errorText, value)) OnPropertyChanged(nameof(HasError));
        }
    }

    public bool HasError => _errorText != null;
    public bool CanGoBack => _back.Count > 0;
    public bool CanGoForward => _forward.Count > 0;

    /// <summary>Paths to select once the next load completes.</summary>
    public HashSet<string> PendingSelection { get; } = new(StringComparer.OrdinalIgnoreCase);

    /// <summary>Selection remembered while the tab is in the background.</summary>
    public List<string> RememberedSelection { get; set; } = [];

    public double ScrollOffset { get; set; }

    public PaneState ToState() => new(CurrentPath, ViewMode, Sort);

    public Task NavigateAsync(string path, bool addToHistory = true)
    {
        if (addToHistory && !string.IsNullOrEmpty(_currentPath) &&
            !string.Equals(_currentPath, path, StringComparison.OrdinalIgnoreCase))
        {
            _back.Push(_currentPath);
            _forward.Clear();
        }
        return LoadAsync(path, rememberVisit: true);
    }

    public Task RefreshAsync() => LoadAsync(_currentPath, rememberVisit: false);

    public async Task GoBackAsync()
    {
        if (_back.Count == 0) return;
        _forward.Push(_currentPath);
        var target = _back.Pop();
        PendingSelection.Add(_currentPath);
        await LoadAsync(target, rememberVisit: true);
    }

    public async Task GoForwardAsync()
    {
        if (_forward.Count == 0) return;
        _back.Push(_currentPath);
        await LoadAsync(_forward.Pop(), rememberVisit: true);
    }

    public async Task GoUpAsync()
    {
        var parent = NexusNative.ParentPath(_currentPath);
        if (parent == null) return;
        PendingSelection.Add(_currentPath);
        await NavigateAsync(parent);
    }

    private async Task LoadAsync(string path, bool rememberVisit)
    {
        var gen = ++_generation;
        IsLoading = true;
        var options = new ListOptions(CoreHost.Settings.ShowHidden, Sort);
        DirListing? listing = null;
        Exception? error = null;
        try
        {
            listing = await Task.Run(() => NexusNative.ListDirectory(path, options));
        }
        catch (Exception e)
        {
            error = e;
        }
        if (gen != _generation) return; // a newer navigation won
        IsLoading = false;
        if (listing == null)
        {
            // Stay in the folder the user asked for, but show why it is empty.
            CurrentPath = path;
            _entries = [];
            Items = [];
            ErrorText = Format.Error(error!);
            OnPropertyChanged(nameof(Summary));
            NotifyHistory();
            Loaded?.Invoke(this, EventArgs.Empty);
            return;
        }
        ErrorText = null;
        var samePlace = string.Equals(CurrentPath, listing.Path, StringComparison.OrdinalIgnoreCase);
        CurrentPath = listing.Path;
        _entries = listing.Entries;
        Items = new ObservableCollection<FileItem>(listing.Entries.Select(e => new FileItem(listing.Path, e)));
        OnPropertyChanged(nameof(Summary));
        NotifyHistory();
        if (!samePlace) Watch(listing.Path);
        if (rememberVisit) _ = Task.Run(() => CoreHost.Core.RecordVisit(listing.Path));
        Loaded?.Invoke(this, EventArgs.Empty);
        _ = FillFolderCountsAsync(gen);
    }

    private void NotifyHistory()
    {
        OnPropertyChanged(nameof(CanGoBack));
        OnPropertyChanged(nameof(CanGoForward));
    }

    /// <summary>Fills "24 mục" for folders in the background, like the mockup.</summary>
    private async Task FillFolderCountsAsync(int gen)
    {
        var folders = Items.Where(i => i.IsDir).Take(400).ToList();
        if (folders.Count == 0) return;
        var dir = CurrentPath;
        var counts = await Task.Run(() => NexusNative.ChildCounts(dir, folders.Select(f => f.Name).ToList()));
        if (gen != _generation) return;
        for (int i = 0; i < folders.Count && i < counts.Count; i++)
        {
            if (counts[i] is uint n) folders[i].SizeText = Format.Items((int)n);
        }
    }

    public void SetSort(SortKey key)
    {
        var desc = Sort.Key == key ? !Sort.Descending : key is SortKey.Modified or SortKey.Size;
        ApplySort(new SortOptions(key, desc, Sort.FoldersFirst));
    }

    public void ApplySort(SortOptions sort)
    {
        Sort = sort;
        if (_entries.Count == 0) return;
        _entries = NexusNative.SortEntries(_entries, sort);
        var keep = Items.ToDictionary(i => i.Name, StringComparer.Ordinal);
        Items = new ObservableCollection<FileItem>(_entries.Select(e =>
            keep.TryGetValue(e.Name, out var old) ? old : new FileItem(CurrentPath, e)));
        Loaded?.Invoke(this, EventArgs.Empty);
    }

    /// <summary>Auto-refresh when something changes in the folder (debounced).</summary>
    private void Watch(string path)
    {
        _watcher?.Dispose();
        _watcher = null;
        try
        {
            _watcher = new FileSystemWatcher(path)
            {
                IncludeSubdirectories = false,
                NotifyFilter = NotifyFilters.FileName | NotifyFilters.DirectoryName | NotifyFilters.Size | NotifyFilters.LastWrite,
                InternalBufferSize = 64 * 1024,
            };
            FileSystemEventHandler onChange = (_, _) => ScheduleRefresh();
            _watcher.Created += onChange;
            _watcher.Deleted += onChange;
            _watcher.Changed += onChange;
            _watcher.Renamed += (_, _) => ScheduleRefresh();
            _watcher.Error += (_, _) => ScheduleRefresh();
            _watcher.EnableRaisingEvents = true;
        }
        catch
        {
            // Some locations (network, virtual folders) cannot be watched; F5 still works.
        }
    }

    private void ScheduleRefresh()
    {
        _dispatcher.TryEnqueue(() =>
        {
            if (_refreshTimer == null)
            {
                _refreshTimer = _dispatcher.CreateTimer();
                _refreshTimer.Interval = TimeSpan.FromMilliseconds(350);
                _refreshTimer.IsRepeating = false;
                _refreshTimer.Tick += async (_, _) => await RefreshAsync();
            }
            _refreshTimer.Stop();
            _refreshTimer.Start();
        });
    }

    public void Dispose()
    {
        _watcher?.Dispose();
        _refreshTimer?.Stop();
    }
}
