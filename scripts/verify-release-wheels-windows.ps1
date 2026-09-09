# Verify uv's Windows release wheels against the signing job's output.
#
# Extract the executables with `extract-wheel-binaries.py`, then delegate byte
# and signature checks to `verify-release-binaries-windows.ps1`. Both scripts
# are provided by `astral-sh/github-actions/setup-release-signing`.

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
    uv run "$env:RELEASE_SIGNING_SCRIPTS/extract-wheel-binaries.py" --output $wheelBinaries $wheels.FullName
    if ($LASTEXITCODE -ne 0) { throw 'Wheel extraction failed' }

    & "$env:RELEASE_SIGNING_SCRIPTS/verify-release-binaries-windows.ps1" -Signed $Signed `
        -BinaryDirectory $wheelBinaries -Binaries @('uv.exe', 'uvx.exe', 'uvw.exe', 'uv-build.exe')
}
finally {
    Remove-Item $wheelBinaries -Recurse -Force
}
