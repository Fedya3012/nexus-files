using Nexus.Core;

namespace NexusFiles.Services;

/// <summary>Owns the single <see cref="NexusCore"/> instance for the app's lifetime.</summary>
public static class CoreHost
{
    private static NexusCore? _core;

    public static string DataDir { get; } =
        System.IO.Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData), "NexusFiles");

    public static NexusCore Core => _core ?? throw new InvalidOperationException("Core not initialised");

    public static void Initialise() => _core ??= new NexusCore(DataDir);

    public static Settings Settings { get; set; } = null!;

    public static SortOptions DefaultSort => new(SortKey.Name, false, true);

    public static string HomePath => Environment.GetFolderPath(Environment.SpecialFolder.UserProfile);
}
