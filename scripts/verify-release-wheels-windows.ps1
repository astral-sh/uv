# Verify the signed executables packaged in uv's Windows release wheels.

param(
    [Parameter(Mandatory)]
    [string] $Signed,
    [Parameter(Mandatory)]
    [string] $WheelDirectory
)

$ErrorActionPreference = 'Stop'

$wheelBinaries = Join-Path ([System.IO.Path]::GetTempPath()) ([System.IO.Path]::GetRandomFileName())
New-Item $wheelBinaries -ItemType Directory | Out-Null
try {
    $wheels = Get-ChildItem "$WheelDirectory/*.whl"
    uv run "$PSScriptRoot/extract-wheel-binaries.py" --output $wheelBinaries $wheels.FullName
    if ($LASTEXITCODE -ne 0) { throw 'Wheel extraction failed' }

    & "$PSScriptRoot/verify-release-binaries-windows.ps1" -Signed $Signed `
        -BinaryDirectory $wheelBinaries -Binaries @('uv.exe', 'uvx.exe', 'uvw.exe', 'uv-build.exe')
}
finally {
    Remove-Item $wheelBinaries -Recurse -Force
}
