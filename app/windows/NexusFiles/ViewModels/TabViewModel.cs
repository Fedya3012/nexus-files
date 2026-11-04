using CommunityToolkit.Mvvm.ComponentModel;
using Nexus.Core;

namespace NexusFiles.ViewModels;

/// <summary>A tab holds one or two panes (dual-pane).</summary>
public sealed class TabViewModel : ObservableObject, IDisposable
{
    private PaneViewModel? _right;
    private bool _rightIsActive;

    public TabViewModel(PaneViewModel left, PaneViewModel? right = null, bool rightIsActive = false)
    {
        Left = left;
        _right = right;
        _rightIsActive = rightIsActive && right != null;
        Left.PropertyChanged += OnPanePropertyChanged;
        if (right != null) right.PropertyChanged += OnPanePropertyChanged;
        UpdateActiveFlags();
    }

    public PaneViewModel Left { get; }

    public PaneViewModel? Right
    {
        get => _right;
        set
        {
            if (_right == value) return;
            if (_right != null)
            {
                _right.PropertyChanged -= OnPanePropertyChanged;
                _right.Dispose();
            }
            _right = value;
            if (_right != null) _right.PropertyChanged += OnPanePropertyChanged;
            if (_right == null) _rightIsActive = false;
            OnPropertyChanged();
            OnPropertyChanged(nameof(IsDual));
            UpdateActiveFlags();
        }
    }

    public bool IsDual => _right != null;

    public PaneViewModel ActivePane => _rightIsActive && _right != null ? _right : Left;
    public PaneViewModel? OtherPane => _right == null ? null : _rightIsActive ? Left : _right;
    public bool RightIsActive => _rightIsActive;

    public string Header => ActivePane.Title;

    public void Activate(PaneViewModel pane)
    {
        var right = pane == _right;
        if (right == _rightIsActive) return;
        _rightIsActive = right;
        UpdateActiveFlags();
        OnPropertyChanged(nameof(ActivePane));
        OnPropertyChanged(nameof(Header));
    }

    private void UpdateActiveFlags()
    {
        Left.IsActive = !_rightIsActive || _right == null;
        if (_right != null) _right.IsActive = _rightIsActive;
    }

    private void OnPanePropertyChanged(object? sender, System.ComponentModel.PropertyChangedEventArgs e)
    {
        if (e.PropertyName == nameof(PaneViewModel.Title) && sender == ActivePane)
            OnPropertyChanged(nameof(Header));
    }

    public TabState ToState() => new(Left.ToState(), _right?.ToState(), _rightIsActive);

    public void Dispose()
    {
        Left.Dispose();
        _right?.Dispose();
    }
}
