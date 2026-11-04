# Builds Nexus Files for Windows: Rust core -> C# bindings -> WinUI 3 app.
# Usage:  ./build.ps1 [-Runtime win-x64|win-arm64] [-Configuration Release|Debug]
param(
    [string]$Runtime = "win-x64",
    [string]$Configuration = "Release"
)
$ErrorActionPreference = "Stop"
Set-Location $PSScriptRoot

$rustTarget = if ($Runtime -eq "win-arm64") { "aarch64-pc-windows-msvc" } else { "x86_64-pc-windows-msvc" }
$platform   = if ($Runtime -eq "win-arm64") { "ARM64" } else { "x64" }

Write-Host "==> Rust core ($rustTarget)"
cargo build --release -p nexus-core --target $rustTarget
if ($LASTEXITCODE) { throw "cargo build failed" }

$native = "target/nexus-native"
New-Item -ItemType Directory -Force $native | Out-Null
Copy-Item "target/$rustTarget/release/nexus_core.dll" $native -Force

Write-Host "==> C# bindings"
if (-not (Get-Command uniffi-bindgen-cs -ErrorAction SilentlyContinue)) {
    cargo install uniffi-bindgen-cs --git https://github.com/NordSecurity/uniffi-bindgen-cs --tag v0.11.1+v0.31.0
}
uniffi-bindgen-cs "$native/nexus_core.dll" --library --config core/uniffi.toml --out-dir $native
if ($LASTEXITCODE) { throw "uniffi-bindgen-cs failed" }

Write-Host "==> WinUI 3 app"
$out = "dist/NexusFiles-$Runtime"
dotnet publish app/windows/NexusFiles/NexusFiles.csproj -c $Configuration -r $Runtime -p:Platform=$platform -o $out
if ($LASTEXITCODE) { throw "dotnet publish failed" }
Write-Host "Done: $out/NexusFiles.exe"
