using System.Diagnostics;
using System.Runtime.InteropServices;

namespace NexusFiles.Services;

/// <summary>Hand-offs to Windows: open files, Properties dialog, Explorer, terminal.</summary>
public static partial class Shell
{
    public static void Open(string path)
    {
        try
        {
            Process.Start(new ProcessStartInfo(path) { UseShellExecute = true });
        }
        catch (Exception e)
        {
            App.ShowError($"Không mở được \"{System.IO.Path.GetFileName(path)}\": {e.Message}");
        }
    }

    public static void OpenWith(string path)
    {
        try
        {
            Process.Start(new ProcessStartInfo("rundll32.exe", $"shell32.dll,OpenAs_RunDLL {path}") { UseShellExecute = false });
        }
        catch (Exception e)
        {
            App.ShowError(e.Message);
        }
    }

    public static void ShowInExplorer(string path)
    {
        try
        {
            Process.Start(new ProcessStartInfo("explorer.exe", $"/select,\"{path}\"") { UseShellExecute = true });
        }
        catch (Exception e)
        {
            App.ShowError(e.Message);
        }
    }

    public static void OpenTerminal(string folder)
    {
        try
        {
            Process.Start(new ProcessStartInfo("wt.exe", $"-d \"{folder}\"") { UseShellExecute = true });
        }
        catch
        {
            try
            {
                Process.Start(new ProcessStartInfo("powershell.exe") { UseShellExecute = true, WorkingDirectory = folder });
            }
            catch (Exception e)
            {
                App.ShowError(e.Message);
            }
        }
    }

    [LibraryImport("shell32.dll", StringMarshalling = StringMarshalling.Utf16)]
    [return: MarshalAs(UnmanagedType.Bool)]
    private static partial bool SHObjectProperties(nint hwnd, uint shopObjectType, string pszObjectName, string? pszPropertyPage);

    private const uint SHOP_FILEPATH = 0x2;

    /// <summary>Opens the standard Windows Properties dialog.</summary>
    public static void Properties(string path) => SHObjectProperties(App.WindowHandle, SHOP_FILEPATH, path, null);
}
