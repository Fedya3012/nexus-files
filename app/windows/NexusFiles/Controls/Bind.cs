using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Documents;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Media;
using Nexus.Core;

using NexusFiles.Services;

namespace NexusFiles.Controls;

/// <summary>Small helpers for x:Bind function bindings.</summary>
public static class Bind
{
    public static Visibility Visible(bool value) => value ? Visibility.Visible : Visibility.Collapsed;
    public static Visibility Collapsed(bool value) => value ? Visibility.Collapsed : Visibility.Visible;
    public static Visibility VisibleIfText(string? value) => string.IsNullOrEmpty(value) ? Visibility.Collapsed : Visibility.Visible;
}

/// <summary>
/// Attached property that renders a title with the matched characters in
/// bold accent colour (Omnibar results, GoTo).
/// </summary>
public static class Highlight
{
    public static readonly DependencyProperty ItemProperty = DependencyProperty.RegisterAttached(
        "Item", typeof(ViewModels.OmniItem), typeof(Highlight), new PropertyMetadata(null, OnChanged));

    public static ViewModels.OmniItem? GetItem(DependencyObject o) => (ViewModels.OmniItem?)o.GetValue(ItemProperty);
    public static void SetItem(DependencyObject o, ViewModels.OmniItem? v) => o.SetValue(ItemProperty, v);

    private static void OnChanged(DependencyObject d, DependencyPropertyChangedEventArgs e)
    {
        if (d is not TextBlock tb) return;
        tb.Inlines.Clear();
        if (e.NewValue is not ViewModels.OmniItem item) return;
        Fill(tb, item.Title, item.Highlights);
    }

    public static void Fill(TextBlock tb, string text, IReadOnlyList<TextRange> ranges)
    {
        tb.Inlines.Clear();
        var accent = Res.Brush("NxAccentText");
        int pos = 0;
        foreach (var r in ranges.OrderBy(r => r.Start))
        {
            int start = (int)Math.Min(r.Start, (uint)text.Length);
            int end = (int)Math.Min(r.Start + r.Length, (uint)text.Length);
            if (start < pos) continue;
            if (start > pos) tb.Inlines.Add(new Run { Text = text[pos..start] });
            if (end > start)
                tb.Inlines.Add(new Run { Text = text[start..end], FontWeight = Microsoft.UI.Text.FontWeights.SemiBold, Foreground = accent });
            pos = end;
        }
        if (pos < text.Length) tb.Inlines.Add(new Run { Text = text[pos..] });
    }
}
