param(
    [Parameter(Mandatory = $true)]
    [ValidateSet("prepare", "measure")]
    [string]$Stage,
    [Parameter(Mandatory = $true)]
    [ValidateSet("c-cold", "d-cold", "d-warm")]
    [string]$Mode,
    [Parameter(Mandatory = $true)]
    [int]$Phase,
    [string]$CacheHit = ""
)

$ErrorActionPreference = "Stop"
$candidateDir = "$env:DEV_DRIVE/uv-python-bootstrap-benchmark"
$baselineDir = "$env:APPDATA/uv/python-bootstrap-benchmark"

if ($Stage -eq "prepare") {
    $installDir = if ($Mode -eq "c-cold") { $baselineDir } else { $candidateDir }
    # These directories belong only to this benchmark on its ephemeral runner.
    if (Test-Path $installDir) {
        Remove-Item -LiteralPath $installDir -Recurse -Force
    }
    $cacheDir = "$env:DEV_DRIVE/uv-python-bootstrap-benchmark-cache-$Phase"
    if (Test-Path $cacheDir) {
        throw "The per-phase download cache must be empty: $cacheDir"
    }
    "UV_PYTHON_INSTALL_DIR=$installDir" >> $env:GITHUB_ENV
    "UV_CACHE_DIR=$cacheDir" >> $env:GITHUB_ENV
    "UV_PYTHON_CACHE_DIR=$cacheDir/python" >> $env:GITHUB_ENV
    "PYTHON_BENCHMARK_START=$([DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds())" >> $env:GITHUB_ENV
    exit 0
}

if ($Mode -eq "d-warm" -and $CacheHit -ne "true") {
    throw "Warm-cache measurement requires an exact cache hit"
}

$stopwatch = [System.Diagnostics.Stopwatch]::StartNew()
uv python install
if ($LASTEXITCODE -ne 0) {
    throw "uv python install failed"
}
$stopwatch.Stop()
$combinedMilliseconds = [DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds() - [long]$env:PYTHON_BENCHMARK_START

# Verify every requested interpreter outside the measured setup interval.
$versions = @(Get-Content .python-versions | Where-Object { $_.Trim() -ne "" -and -not $_.Trim().StartsWith("#") })
$verified = @()
foreach ($version in $versions) {
    $executable = (uv python find --managed-python $version).Trim()
    if ($LASTEXITCODE -ne 0) {
        throw "Could not find Python $version"
    }
    $installPath = [System.IO.Path]::GetFullPath($env:UV_PYTHON_INSTALL_DIR).TrimEnd('\') + '\'
    if (-not [System.IO.Path]::GetFullPath($executable).StartsWith($installPath, [StringComparison]::OrdinalIgnoreCase)) {
        throw "Python $version resolved outside the benchmark installation: $executable"
    }
    $actual = (& $executable --version).Trim()
    if ($LASTEXITCODE -ne 0 -or $actual -ne "Python $version") {
        throw "Expected Python $version, got $actual"
    }
    $verified += @{ request = $version; executable = $executable; version = $actual }
}

$installations = @(Get-ChildItem -LiteralPath $env:UV_PYTHON_INSTALL_DIR -Directory -Filter "cpython-*")
if ($installations.Count -ne $versions.Count) {
    throw "Expected $($versions.Count) installations, got $($installations.Count)"
}
$files = @(Get-ChildItem -LiteralPath $env:UV_PYTHON_INSTALL_DIR -File -Recurse)
$result = [ordered]@{
    mode = $Mode
    phase = $Phase
    order = $env:BENCHMARK_ORDER
    runner = $env:RUNNER_NAME
    uv_version = (uv --version)
    cache_hit = $CacheHit
    install_directory = $env:UV_PYTHON_INSTALL_DIR
    install_seconds = $stopwatch.Elapsed.TotalSeconds
    combined_seconds = $combinedMilliseconds / 1000.0
    installation_count = $installations.Count
    installed_file_count = $files.Count
    installed_bytes = ($files | Measure-Object -Property Length -Sum).Sum
    interpreters = $verified
}
$result | ConvertTo-Json -Depth 5 | Set-Content "python-bootstrap-$Phase.json"
$result | ConvertTo-Json -Depth 5
