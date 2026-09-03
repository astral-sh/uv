# Verify uv's packaged Windows executables against the signing job's output.
#
# The signing job already checked the expected publisher. Require the archive
# and wheel executables to contain exactly the signed bytes, then check that
# Windows trusts the wheel executables' timestamped Authenticode signatures.

param(
    [Parameter(Mandatory)]
    [string] $Signed,
    [Parameter(Mandatory)]
    [string] $Distribution
)

$ErrorActionPreference = 'Stop'

$temporary = Join-Path ([System.IO.Path]::GetTempPath()) ([System.IO.Path]::GetRandomFileName())
New-Item $temporary -ItemType Directory | Out-Null
try {
    $wheelBinaries = Join-Path $temporary 'wheels'
    $archiveBinaries = Join-Path $temporary 'archive'
    $wheels = Get-ChildItem "$Distribution/wheels/*.whl"
    uv run "$PSScriptRoot/extract-wheel-binaries.py" --output $wheelBinaries $wheels.FullName
    if ($LASTEXITCODE -ne 0) { throw 'Wheel extraction failed' }
    Expand-Archive "$Distribution/uv-x86_64-pc-windows-msvc.zip" $archiveBinaries

    foreach ($binary in @('uv.exe', 'uvx.exe', 'uvw.exe')) {
        if ((Get-FileHash "$Signed/$binary").Hash -ne (Get-FileHash "$archiveBinaries/$binary").Hash) {
            throw "Archive executable differs from signing output: $binary"
        }
    }

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
    Remove-Item $temporary -Recurse -Force
}
