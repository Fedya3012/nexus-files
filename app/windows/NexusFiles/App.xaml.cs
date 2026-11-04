using System.Runtime.InteropServices;
using Microsoft.UI.Xaml;
using Nexus.Core;
using NexusFiles.Services;

namespace NexusFiles;

public partial class App : Application
{
    public static MainWindow? Main { get; private set; }
    public static nint WindowHandle { get; set; }

    public App()
    {
        InitializeComponent();
        UnhandledException += (__, e) =>
        {
            e.Handled = true;
            ShowError(e.Exception is NexusException ne ? Format.Error(ne) : e.Exception.Message);
        };
        try
        {
            CoreHost.Initialise();
            CoreHost.Settings = CoreHost.Core.LoadSettings();
            // The app-wide theme can only be set before the first window opens.
            if (CoreHost.Settings.Theme == ThemePreference.Light) RequestedTheme = ApplicationTheme.Light;
            else if (CoreHost.Settings.Theme == ThemePreference.Dark) RequestedTheme = ApplicationTheme.Dark;
        }
        catch (Exception e)
        {
            MessageBoxW(0, $"Không khởi động được lõi Nexus (nexus_core.dll).\n\n{e.Message}", "Nexus Files", 0x10);
            Environment.Exit(1);
        }
    }

    protected override void OnLaunched(LaunchActivatedEventArgs args)
    {
        var argv = Environment.GetCommandLineArgs();
        var startPath = argv.Length > 1 && Directory.Exists(argv[1]) ? argv[1] : null;
        Main = new MainWindow(startPath);
        Main.Activate();
    }

    public static void ShowError(string message) => Main?.ShowError(message);

    [DllImport("user32.dll", CharSet = CharSet.Unicode)]
    private static extern int MessageBoxW(nint hWnd, string text, string caption, uint type);
}
