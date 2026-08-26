[CmdletBinding()]
param(
    [string]$Version = $(if ($env:SCV_VERSION) { $env:SCV_VERSION } else { "latest" }),
    [string]$InstallDir = $(if ($env:SCV_INSTALL_DIR) { $env:SCV_INSTALL_DIR } else { Join-Path $env:LOCALAPPDATA "Programs\SCV\bin" }),
    [string]$BinaryPath,
    [switch]$NoModifyPath
)

$ErrorActionPreference = "Stop"
$releaseBaseUrl = if ($env:SCV_RELEASE_BASE_URL) { $env:SCV_RELEASE_BASE_URL.TrimEnd('/') } else { "https://github.com/elixir/scv/releases" }
$destination = Join-Path $InstallDir "scv.exe"
New-Item -ItemType Directory -Force -Path $InstallDir | Out-Null

if ($BinaryPath) {
    if (-not (Test-Path -LiteralPath $BinaryPath -PathType Leaf)) {
        throw "Binary not found: $BinaryPath"
    }
    Copy-Item -LiteralPath $BinaryPath -Destination $destination -Force
} else {
    $temporary = Join-Path ([System.IO.Path]::GetTempPath()) ("scv-install-" + [guid]::NewGuid().ToString("N"))
    New-Item -ItemType Directory -Path $temporary | Out-Null
    try {
        $artifact = "scv-x86_64-pc-windows-msvc.zip"
        if ($Version -eq "latest") {
            $downloadUrl = "$releaseBaseUrl/latest/download/$artifact"
            $checksumUrl = "$releaseBaseUrl/latest/download/SHA256SUMS"
        } else {
            $downloadUrl = "$releaseBaseUrl/download/$Version/$artifact"
            $checksumUrl = "$releaseBaseUrl/download/$Version/SHA256SUMS"
        }
        $archive = Join-Path $temporary $artifact
        $checksums = Join-Path $temporary "SHA256SUMS"
        Invoke-WebRequest -UseBasicParsing -Uri $downloadUrl -OutFile $archive
        Invoke-WebRequest -UseBasicParsing -Uri $checksumUrl -OutFile $checksums
        $expectedLine = Get-Content -LiteralPath $checksums | Where-Object { $_ -match "\s+$([regex]::Escape($artifact))$" } | Select-Object -First 1
        if (-not $expectedLine) { throw "Checksum missing for $artifact" }
        $expected = ($expectedLine -split "\s+")[0].ToLowerInvariant()
        $actual = (Get-FileHash -Algorithm SHA256 -LiteralPath $archive).Hash.ToLowerInvariant()
        if ($actual -ne $expected) { throw "Checksum verification failed" }
        Expand-Archive -LiteralPath $archive -DestinationPath $temporary -Force
        Copy-Item -LiteralPath (Join-Path $temporary "scv.exe") -Destination $destination -Force
    } finally {
        if (Test-Path -LiteralPath $temporary) {
            Remove-Item -LiteralPath $temporary -Recurse -Force
        }
    }
}

$noPathChange = $NoModifyPath -or $env:SCV_NO_MODIFY_PATH -eq "1"
$userPath = [Environment]::GetEnvironmentVariable("Path", "User")
$pathEntries = @($userPath -split ';' | Where-Object { $_ })
$alreadyPresent = $pathEntries | Where-Object { [string]::Equals($_.TrimEnd('\'), $InstallDir.TrimEnd('\'), [System.StringComparison]::OrdinalIgnoreCase) }
if (-not $noPathChange -and -not $alreadyPresent) {
    $newPath = (@($pathEntries) + $InstallDir) -join ';'
    [Environment]::SetEnvironmentVariable("Path", $newPath, "User")
    Write-Host "Updated the user PATH. Open a new terminal to use scv."
}

Write-Host "Installed SCV: $destination"
& $destination --version
