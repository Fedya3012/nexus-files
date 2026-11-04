using Microsoft.UI.Input;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Input;
using Microsoft.UI.Xaml.Media;

using NexusFiles.Services;

namespace NexusFiles.Controls;

/// <summary>
/// Thin vertical bar that resizes the grid column to its left (or right, for
/// the Inspector) by dragging. Raises <see cref="Dragged"/> with the delta.
/// </summary>
public sealed partial class SplitterBar : Grid
{
    private bool _dragging;
    private double _lastX;

    public SplitterBar()
    {
        Background = new SolidColorBrush(Microsoft.UI.Colors.Transparent);
        ProtectedCursor = InputSystemCursor.Create(InputSystemCursorShape.SizeWestEast);
        PointerPressed += OnPressed;
        PointerMoved += OnMoved;
        PointerReleased += OnReleased;
        PointerCaptureLost += (_, _) => _dragging = false;
    }

    public event EventHandler<double>? Dragged;
    public event EventHandler? DragCompleted;

    private void OnPressed(object sender, PointerRoutedEventArgs e)
    {
        _dragging = CapturePointer(e.Pointer);
        _lastX = e.GetCurrentPoint(XamlRoot.Content).Position.X;
        e.Handled = true;
    }

    private void OnMoved(object sender, PointerRoutedEventArgs e)
    {
        if (!_dragging) return;
        var x = e.GetCurrentPoint(XamlRoot.Content).Position.X;
        Dragged?.Invoke(this, x - _lastX);
        _lastX = x;
        e.Handled = true;
    }

    private void OnReleased(object sender, PointerRoutedEventArgs e)
    {
        if (!_dragging) return;
        _dragging = false;
        ReleasePointerCapture(e.Pointer);
        DragCompleted?.Invoke(this, EventArgs.Empty);
        e.Handled = true;
    }
}

/// <summary>x:Bind helpers used by MainWindow templates.</summary>
public static class MainWindowBind
{
    public static Brush UsageBrush(bool low) =>
        Res.Brush(low ? "NxBad" : "NxAccent");

    public static Brush UsageTextBrush(bool low) =>
        Res.Brush(low ? "NxBad" : "NxText3");
}
