& {
    $ErrorActionPreference = 'Stop'
    $platform = switch ([Runtime.InteropServices.RuntimeInformation]::OSArchitecture) {
        'X64' { 'windows-x86_64' }
        'Arm64' { 'windows-aarch64' }
        default { throw 'No release package for this Windows architecture.' }
    }

    $version = '0.1.0'
    $repository = 'https://raw.githubusercontent.com/3rg0n/asciiflow-desktop/main'
    $release = "$repository/dist/$version"
    $package = "asciiflow-desktop-$version-$platform.zip"
    $stage = New-Item -ItemType Directory -Path (Join-Path ([IO.Path]::GetTempPath()) ([guid]::NewGuid().ToString('N')))
    try {
        $dist = Join-Path $stage.FullName "dist/$version"
        $null = New-Item -ItemType Directory -Path $dist -Force
        $installer = Join-Path $stage.FullName 'install.ps1'
        Invoke-WebRequest -UseBasicParsing -Uri "$release/$package" -OutFile (Join-Path $dist $package)
        Invoke-WebRequest -UseBasicParsing -Uri "$release/SHA256SUMS" -OutFile (Join-Path $dist 'SHA256SUMS')
        Invoke-WebRequest -UseBasicParsing -Uri "$repository/install.ps1" -OutFile $installer
        & $installer -Version $version
    } finally {
        Remove-Item -LiteralPath $stage.FullName -Recurse -Force
    }
}
