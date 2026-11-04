using Microsoft.UI.Dispatching;
using Microsoft.UI.Input;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Input;
using Nexus.Core;
using NexusFiles.Services;
using NexusFiles.ViewModels;
using Windows.System;
using Windows.UI.Core;

namespace NexusFiles.Controls;

public sealed class OmniTemplateSelector : DataTemplateSelector
{
    public DataTemplate? Header { get; set; }
    public DataTemplate? Row { get; set; }
    protected override DataTemplate? SelectTemplateCore(object item) => item is OmniItem { IsHeader: true } ? Header : Row;
    protected override DataTemplate? SelectTemplateCore(object item, DependencyObject container) => SelectTemplateCore(item);
}

public sealed class OmniActivation
{
    public required OmniItem Item { get; init; }
    public required OpenTarget Target { get; init; }
    /// <summary>Alt+Enter: show the file in its folder instead of opening it.</summary>
    public bool Reveal { get; init; }
}

/// <summary>One box for search, GoTo, commands and paths.</summary>
public sealed partial class Omnibar : UserControl
{
    private readonly DispatcherQueueTimer _debounce;
    private CancelToken? _searchToken;
    private string _currentDir = "";
    private int _generation;

    public Omnibar()
    {
        InitializeComponent();
        _debounce = DispatcherQueue.CreateTimer();
        _debounce.Interval = TimeSpan.FromMilliseconds(90);
        _debounce.IsRepeating = false;
        _debounce.Tick += (_, _) => _ = UpdateResultsAsync();
    }

    public event EventHandler<OmniActivation>? Activated;
    /// <summary>Raised when Enter is pressed on a typed path with no result selected.</summary>
    public event EventHandler<(string path, OpenTarget target)>? PathEntered;
    public event EventHandler? Closed;

    public bool IsOpen => Visibility == Visibility.Visible;

    public void Show(string text, string currentDir, bool selectAll)
    {
        _currentDir = currentDir;
        Visibility = Visibility.Visible;
        Input.Text = text;
        if (selectAll) Input.SelectAll();
        else Input.Select(text.Length, 0);
        Input.Focus(FocusState.Programmatic);
        _ = UpdateResultsAsync();
    }

    public void Hide()
    {
        if (!IsOpen) return;
        _searchToken?.Cancel();
        Visibility = Visibility.Collapsed;
        Closed?.Invoke(this, EventArgs.Empty);
    }

    private void OnScrimPressed(object sender, PointerRoutedEventArgs e) => Hide();
    private void OnCardPressed(object sender, PointerRoutedEventArgs e) => e.Handled = true;

    private void OnTextChanged(object sender, TextChangedEventArgs e)
    {
        _debounce.Stop();
        _debounce.Start();
    }

    private async Task UpdateResultsAsync()
    {
        var gen = ++_generation;
        var text = Input.Text;
        var dir = _currentDir;
        _searchToken?.Cancel();
        var token = new CancelToken();
        _searchToken = token;

        OmnibarResults quick;
        try
        {
            quick = await Task.Run(() => CoreHost.Core.Omnibar(text, dir, 8));
        }
        catch (Exception ex)
        {
            ShowStatus(Format.Error(ex));
            return;
        }
        if (gen != _generation) return;
        RenderChips(quick.Parsed.Chips);

        var items = new List<OmniItem>();
        void AddPlaces(string header)
        {
            if (quick.Places.Count == 0) return;
            items.Add(OmniItem.Header(header));
            items.AddRange(quick.Places.Select(OmniItem.FromPlace));
        }
        void AddCommands()
        {
            if (quick.Commands.Count == 0) return;
            items.Add(OmniItem.Header("LỆNH"));
            items.AddRange(quick.Commands.Select(OmniItem.FromCommand));
        }

        switch (quick.Parsed.Mode)
        {
            case OmnibarMode.Command:
                AddCommands();
                break;
            case OmnibarMode.GoTo:
                AddPlaces("ĐI TỚI THƯ MỤC");
                break;
            case OmnibarMode.Path:
                AddPlaces("THƯ MỤC CON");
                break;
            case OmnibarMode.Tag:
                break;
            case OmnibarMode.Search:
                if (string.IsNullOrWhiteSpace(text))
                {
                    var recent = await Task.Run(() => CoreHost.Core.RecentFolders(8));
                    if (gen != _generation) return;
                    if (recent.Count > 0)
                    {
                        items.Add(OmniItem.Header("GẦN ĐÂY"));
                        items.AddRange(recent.Select(p => OmniItem.FromPlace(new PlaceMatch(p, [], 0))));
                    }
                    break;
                }
                // Show instant results first, then fill in files when the walk returns.
                Render(items, quick.Hint);
                ScopeText.Text = "Thư mục hiện tại · đang tìm…";
                SearchResults found;
                try
                {
                    found = await Task.Run(() => CoreHost.Core.Search(dir, text, 40, token));
                }
                catch (Exception ex)
                {
                    if (gen == _generation) ShowStatus(Format.Error(ex));
                    return;
                }
                if (gen != _generation) return;
                items.Clear();
                if (found.Files.Count > 0)
                {
                    var more = found.Incomplete ? "+" : "";
                    items.Add(OmniItem.Header($"FILE · {found.Files.Count}{more} KẾT QUẢ · {found.ElapsedMs} ms"));
                    items.AddRange(found.Files.Select(OmniItem.FromHit));
                }
                if (found.Folders.Count > 0)
                {
                    items.Add(OmniItem.Header("THƯ MỤC"));
                    items.AddRange(found.Folders.Select(OmniItem.FromHit));
                }
                AddPlaces("ĐI TỚI");
                AddCommands();
                ScopeText.Text = "Thư mục hiện tại";
                if (items.Count == 0)
                {
                    ShowStatus($"Không tìm thấy gì trong {dir} (đã xem {found.Scanned:N0} mục).");
                    return;
                }
                break;
        }
        Render(items, quick.Hint);
    }

    private void Render(List<OmniItem> items, string? hint)
    {
        Results.ItemsSource = items;
        var first = items.FindIndex(i => !i.IsHeader);
        Results.SelectedIndex = first;
        if (hint != null) ShowStatus(hint);
        else StatusText.Visibility = Visibility.Collapsed;
        Results.Visibility = items.Count > 0 ? Visibility.Visible : Visibility.Collapsed;
    }

    private void ShowStatus(string text)
    {
        StatusText.Text = text;
        StatusText.Visibility = Visibility.Visible;
        Results.Visibility = Visibility.Collapsed;
    }

    private void RenderChips(List<FilterChip> chips)
    {
        Chips.Items.Clear();
        foreach (var chip in chips)
        {
            var b = new Button
            {
                Content = $"{chip.Label}  ✕",
                FontSize = 12,
                Padding = new Thickness(8, 2, 8, 2),
                CornerRadius = new CornerRadius(12),
                Background = Res.Brush("NxAccentSoft"),
                Foreground = Res.Brush("NxAccentText"),
                BorderThickness = new Thickness(0),
            };
            ToolTipService.SetToolTip(b, "Bỏ bộ lọc này");
            var token = chip.Token;
            b.Click += (_, _) =>
            {
                Input.Text = string.Join(' ', Input.Text.Split(' ').Where(t => t != token));
                Input.Focus(FocusState.Programmatic);
                Input.Select(Input.Text.Length, 0);
            };
            Chips.Items.Add(b);
        }
    }

    private void OnContainerChanging(ListViewBase sender, ContainerContentChangingEventArgs args)
    {
        if (args.ItemContainer is ListViewItem c && args.Item is OmniItem item)
        {
            c.IsHitTestVisible = !item.IsHeader;
            c.IsTabStop = !item.IsHeader;
            c.MinHeight = item.IsHeader ? 24 : 44;
        }
    }

    private static bool IsDown(VirtualKey key) =>
        InputKeyboardSource.GetKeyStateForCurrentThread(key).HasFlag(CoreVirtualKeyStates.Down);

    private void MoveSelection(int delta)
    {
        if (Results.ItemsSource is not List<OmniItem> items || items.Count == 0) return;
        var i = Results.SelectedIndex;
        for (int step = 0; step < items.Count; step++)
        {
            i = (i + delta + items.Count) % items.Count;
            if (!items[i].IsHeader) break;
        }
        Results.SelectedIndex = i;
        Results.ScrollIntoView(items[i]);
    }

    private void OnInputKeyDown(object sender, KeyRoutedEventArgs e)
    {
        switch (e.Key)
        {
            case VirtualKey.Escape:
                Hide();
                e.Handled = true;
                break;
            case VirtualKey.Down:
                MoveSelection(1);
                e.Handled = true;
                break;
            case VirtualKey.Up:
                MoveSelection(-1);
                e.Handled = true;
                break;
            case VirtualKey.Enter:
                e.Handled = true;
                var target = IsDown(VirtualKey.Control) ? OpenTarget.NewTab : IsDown(VirtualKey.Shift) ? OpenTarget.OtherPane : OpenTarget.Here;
                var reveal = IsDown(VirtualKey.Menu);
                var text = Input.Text.Trim();
                var parsed = NexusNative.ParseQuery(text);
                // A typed path wins over its first completion when it exists.
                if (parsed.Mode == OmnibarMode.Path && (Directory.Exists(SafeNormalize(text)) || File.Exists(SafeNormalize(text))))
                {
                    PathEntered?.Invoke(this, (SafeNormalize(text), target));
                    Hide();
                    return;
                }
                if (Results.SelectedItem is OmniItem item && !item.IsHeader)
                {
                    Activate(item, target, reveal);
                }
                break;
        }
    }

    private static string SafeNormalize(string p)
    {
        try { return NexusNative.NormalizePath(p); } catch { return p; }
    }

    private void OnItemClick(object sender, ItemClickEventArgs e)
    {
        if (e.ClickedItem is OmniItem { IsHeader: false } item)
            Activate(item, IsDown(VirtualKey.Control) ? OpenTarget.NewTab : OpenTarget.Here, false);
    }

    private void Activate(OmniItem item, OpenTarget target, bool reveal)
    {
        Hide();
        Activated?.Invoke(this, new OmniActivation { Item = item, Target = target, Reveal = reveal });
    }
}
