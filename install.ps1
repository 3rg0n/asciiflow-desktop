[CmdletBinding()]
param(
    [ValidatePattern('^\d+\.\d+\.\d+$')]
    [string]$Version = '0.1.0',
    [string]$InstallDir = (Join-Path ([Environment]::GetFolderPath('LocalApplicationData')) 'Programs\ASCIIFlow Desktop'),
    [switch]$NoPath
)

$ErrorActionPreference = 'Stop'
$platform = switch ([Runtime.InteropServices.RuntimeInformation]::OSArchitecture) {
    'X64' { 'windows-x86_64' }
    'Arm64' { 'windows-aarch64' }
    default { throw 'No release package for this Windows architecture.' }
}

$installDir = $ExecutionContext.SessionState.Path.GetUnresolvedProviderPathFromPSPath($InstallDir)
$releaseDir = Join-Path (Join-Path $PSScriptRoot 'dist') $Version
$package = "asciiflow-desktop-$Version-$platform.zip"
$archive = Join-Path $releaseDir $package
if (!(Test-Path -LiteralPath $archive -PathType Leaf)) {
    throw "Missing package $archive. Check out the release or download the archive and SHA256SUMS into dist/$Version."
}
$line = Get-Content -LiteralPath (Join-Path $releaseDir 'SHA256SUMS') | Where-Object {
    $_ -match ('^[0-9a-fA-F]{64}\s+\*?' + [regex]::Escape($package) + '$')
} | Select-Object -First 1
if (!$line -or (Get-FileHash -LiteralPath $archive -Algorithm SHA256).Hash -ne ($line -split '\s+')[0]) {
    throw "SHA-256 verification failed for $package."
}

Add-Type -AssemblyName System.IO.Compression.FileSystem
$zip = [IO.Compression.ZipFile]::OpenRead($archive)
try {
    $entry = $zip.GetEntry('asciiflow-desktop.exe')
    if (!$entry) { throw 'The package is missing asciiflow-desktop.exe.' }
    $null = New-Item -ItemType Directory -Path $installDir -Force
    $destination = Join-Path $installDir 'asciiflow-desktop.exe'
    $pending = Join-Path $installDir ('.asciiflow-' + [Guid]::NewGuid().ToString('N') + '.tmp')
    try {
        [IO.Compression.ZipFileExtensions]::ExtractToFile($entry, $pending)
        if (Test-Path -LiteralPath $destination) {
            [IO.File]::Replace($pending, $destination, [NullString]::Value)
        } else {
            [IO.File]::Move($pending, $destination)
        }
    } finally {
        if (Test-Path -LiteralPath $pending) { Remove-Item -LiteralPath $pending -Force }
    }
} finally {
    $zip.Dispose()
}

if (!$NoPath) {
    function Add-PathEntry([string]$CurrentPath, [string]$Directory) {
        $entries = @($CurrentPath -split ';' | Where-Object { $_ })
        $existing = $entries | Where-Object {
            [Environment]::ExpandEnvironmentVariables($_.Trim('"')).TrimEnd('\', '/') -eq $Directory.TrimEnd('\', '/')
        }
        if ($existing) { return $CurrentPath }
        return (($entries + $Directory) -join ';')
    }
    $userPath = [Environment]::GetEnvironmentVariable('Path', 'User')
    $updatedPath = Add-PathEntry $userPath $installDir
    if ($updatedPath -ne $userPath) { [Environment]::SetEnvironmentVariable('Path', $updatedPath, 'User') }
    $env:Path = Add-PathEntry $env:Path $installDir
}
Write-Output "Installed $destination"
