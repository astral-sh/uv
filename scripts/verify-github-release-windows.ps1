# Verify the signed executables in a Windows GitHub release archive.
# Check the archive's exact contents and checksum, then require the signer's
# output and publicly trusted, timestamped Authenticode signatures.

param(
    [Parameter(Mandatory)]
    [string] $Signed,
    [Parameter(Mandatory)]
    [string] $Archive
)

$ErrorActionPreference = 'Stop'

$archiveBinaries = Join-Path ([System.IO.Path]::GetTempPath()) ([System.IO.Path]::GetRandomFileName())
New-Item $archiveBinaries -ItemType Directory | Out-Null
try {
    uv run "$PSScriptRoot/extract-github-release-binaries.py" --output $archiveBinaries $Archive
    if ($LASTEXITCODE -ne 0) { throw 'GitHub release archive extraction failed' }

    & "$PSScriptRoot/verify-release-binaries-windows.ps1" -Signed $Signed `
        -BinaryDirectory $archiveBinaries -Binaries @('uv.exe', 'uvx.exe', 'uvw.exe')
}
finally {
    Remove-Item $archiveBinaries -Recurse -Force
}
