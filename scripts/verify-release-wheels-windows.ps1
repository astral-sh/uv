# Verify uv's Windows release wheels against the signing job's output.
#
# The signing job already checked the expected publisher. Require the
# wheel executables to contain exactly the signed bytes, then check that
# Windows trusts the wheel executables' timestamped Authenticode signatures.

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

    $signtool = Get-ChildItem "${env:ProgramFiles(x86)}\Windows Kits\10\bin\*\x64\signtool.exe" |
        Sort-Object FullName -Descending | Select-Object -First 1 -ExpandProperty FullName
    if (-not $signtool) { throw 'signtool.exe not found in Windows SDK' }

    foreach ($binary in @('uv.exe', 'uvx.exe', 'uvw.exe', 'uv-build.exe')) {
        $binaryPath = (Get-Item "$wheelBinaries/$binary").FullName
        if ((Get-FileHash "$Signed/$binary").Hash -ne (Get-FileHash $binaryPath).Hash) {
            throw "Wheel executable differs from signing output: $binary"
        }
        $signature = Get-AuthenticodeSignature $binaryPath
        if ($signature.Status -ne 'Valid' -or $null -eq $signature.TimeStamperCertificate) {
            throw "Expected a publicly trusted, timestamped signature: $binary"
        }
        & $signtool verify /pa /all /v $binaryPath
        if ($LASTEXITCODE -ne 0) { throw "Signature verification failed: $binary" }
    }
}
finally {
    Remove-Item $wheelBinaries -Recurse -Force
}
