using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Media;

namespace NexusFiles.Services;

/// <summary>
/// Looks up Nexus theme brushes for the theme the window actually shows
/// (the user can pick Light/Dark independently of Windows).
/// </summary>
public static class Res
{
    public static ElementTheme CurrentTheme { get; set; } = ElementTheme.Default;

    private static string ThemeKey()
    {
        var t = CurrentTheme;
        if (t == ElementTheme.Default)
            t = Application.Current.RequestedTheme == ApplicationTheme.Dark ? ElementTheme.Dark : ElementTheme.Light;
        return t == ElementTheme.Dark ? "Dark" : "Light";
    }

    public static Brush Brush(string key)
    {
        var theme = ThemeKey();
        foreach (var md in Application.Current.Resources.MergedDictionaries)
        {
            if (md.ThemeDictionaries.TryGetValue(theme, out var d) && d is ResourceDictionary rd && rd.TryGetValue(key, out var v) && v is Brush b)
                return b;
        }
        return (Brush)Application.Current.Resources[key];
    }

    public static T Get<T>(string key) => (T)Application.Current.Resources[key];
}
