# Install and run the Windows wheels in the given directory.
param(
    [Parameter(Mandatory)]
    [string] $WheelDirectory
)

$ErrorActionPreference = 'Stop'

$virtualEnvironment = Join-Path ([System.IO.Path]::GetTempPath()) ([System.IO.Path]::GetRandomFileName())
New-Item $virtualEnvironment -ItemType Directory | Out-Null
try {
    python -m venv $virtualEnvironment
    if ($LASTEXITCODE -ne 0) { throw 'Virtual environment creation failed' }
    $wheels = Get-ChildItem "$WheelDirectory/*.whl"
    & "$virtualEnvironment/Scripts/python.exe" -m pip install --no-index --no-deps $wheels.FullName
    if ($LASTEXITCODE -ne 0) { throw 'Signed wheel installation failed' }
    foreach ($binary in @('uv.exe', 'uvx.exe', 'uvw.exe', 'uv-build.exe')) {
        $binaryPath = Join-Path "$virtualEnvironment/Scripts" $binary
        $process = Start-Process -FilePath $binaryPath -ArgumentList '--version' -NoNewWindow -Wait -PassThru
        if ($process.ExitCode -ne 0) { throw "Signed executable failed: $binary" }
    }
}
finally {
    Remove-Item $virtualEnvironment -Recurse -Force
}
