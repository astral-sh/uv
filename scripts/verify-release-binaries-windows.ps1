# Verify packaged Windows executables against the signing job's output.
#
# The signing job already checked the expected publisher. Require the
# packaged executables to contain exactly the signed bytes, then check that
# Windows trusts their timestamped Authenticode signatures.

param(
    [Parameter(Mandatory)]
    [string] $Signed,
    [Parameter(Mandatory)]
    [string] $BinaryDirectory,
    [Parameter(Mandatory)]
    [string[]] $Binaries
)

$ErrorActionPreference = 'Stop'

$signtool = Get-ChildItem "${env:ProgramFiles(x86)}\Windows Kits\10\bin\*\x64\signtool.exe" |
    Sort-Object FullName -Descending | Select-Object -First 1 -ExpandProperty FullName
if (-not $signtool) { throw 'signtool.exe not found in Windows SDK' }

foreach ($binary in $Binaries) {
    $binaryPath = (Get-Item "$BinaryDirectory/$binary").FullName
    if ((Get-FileHash "$Signed/$binary").Hash -ne (Get-FileHash $binaryPath).Hash) {
        throw "Packaged executable differs from signing output: $binary"
    }
    $signature = Get-AuthenticodeSignature $binaryPath
    if ($signature.Status -ne 'Valid' -or $null -eq $signature.TimeStamperCertificate) {
        throw "Expected a publicly trusted, timestamped signature: $binary"
    }
    & $signtool verify /pa /all /v $binaryPath
    if ($LASTEXITCODE -ne 0) { throw "Signature verification failed: $binary" }
}
