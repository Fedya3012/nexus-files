using Windows.ApplicationModel.DataTransfer;
using Windows.Storage;

namespace NexusFiles.Services;

/// <summary>
/// Cut/Copy/Paste through the Windows clipboard, so files can be pasted into
/// File Explorer and vice versa.
/// </summary>
public static class FileClipboard
{
    public static async Task SetAsync(IReadOnlyList<string> paths, bool cut)
    {
        var package = new DataPackage { RequestedOperation = cut ? DataPackageOperation.Move : DataPackageOperation.Copy };
        var items = new List<IStorageItem>();
        foreach (var p in paths)
        {
            try
            {
                items.Add(Directory.Exists(p)
                    ? await StorageFolder.GetFolderFromPathAsync(p)
                    : await StorageFile.GetFileFromPathAsync(p));
            }
            catch
            {
                // Items Windows cannot wrap (e.g. some system files) are still
                // carried in the text form below.
            }
        }
        if (items.Count > 0) package.SetStorageItems(items);
        package.SetText(string.Join(Environment.NewLine, paths));
        Clipboard.SetContent(package);
        Clipboard.Flush();
    }

    /// <summary>Returns the paths on the clipboard and whether they were cut.</summary>
    public static async Task<(List<string> paths, bool cut)> GetAsync()
    {
        var content = Clipboard.GetContent();
        var cut = content.RequestedOperation.HasFlag(DataPackageOperation.Move);
        if (content.Contains(StandardDataFormats.StorageItems))
        {
            var items = await content.GetStorageItemsAsync();
            var paths = items.Select(i => i.Path).Where(p => !string.IsNullOrEmpty(p)).ToList();
            if (paths.Count > 0) return (paths, cut);
        }
        if (content.Contains(StandardDataFormats.Text))
        {
            var text = await content.GetTextAsync();
            var paths = text.Split(['\r', '\n'], StringSplitOptions.RemoveEmptyEntries)
                .Select(t => t.Trim().Trim('"'))
                .Where(t => System.IO.Path.IsPathFullyQualified(t) && (File.Exists(t) || Directory.Exists(t)))
                .ToList();
            return (paths, cut);
        }
        return ([], false);
    }

    public static bool HasFiles()
    {
        try
        {
            var c = Clipboard.GetContent();
            return c.Contains(StandardDataFormats.StorageItems) || c.Contains(StandardDataFormats.Text);
        }
        catch
        {
            return false;
        }
    }

    public static void Clear() => Clipboard.Clear();
}
