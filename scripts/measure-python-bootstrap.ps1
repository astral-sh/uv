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
$candidateDir = "$env:DEV_DRIVE/uv-python"
$baselineDir = "$env:APPDATA/uv/python"

if ($Stage -eq "prepare") {
    $installDir = if ($Mode -eq "c-cold") { $baselineDir } else { $candidateDir }
    if ($Mode -eq "c-cold" -and (Test-Path $baselineDir)) {
        throw "The production baseline path must be absent before measurement: $baselineDir"
    }
    # The candidate directory belongs only to this benchmark on its ephemeral runner.
    if (Test-Path $installDir) {
        Remove-Item -LiteralPath $installDir -Recurse -Force
    }
    "UV_PYTHON_INSTALL_DIR=$installDir" >> $env:GITHUB_ENV
    "PYTHON_BENCHMARK_START=$([System.Diagnostics.Stopwatch]::GetTimestamp())" >> $env:GITHUB_ENV
    exit 0
}

if ($Mode -eq "d-warm" -and $CacheHit -ne "true") {
    throw "Warm-cache measurement requires an exact cache hit"
}

$stopwatch = [System.Diagnostics.Stopwatch]::StartNew()
uv python install
$installExitCode = $LASTEXITCODE
$stopwatch.Stop()
$combinedSeconds = ([System.Diagnostics.Stopwatch]::GetTimestamp() - [long]$env:PYTHON_BENCHMARK_START) / [System.Diagnostics.Stopwatch]::Frequency

$result = [ordered]@{
    mode = $Mode
    phase = $Phase
    order = $env:BENCHMARK_ORDER
    runner = $env:RUNNER_NAME
    uv_version = (uv --version)
    cache_directory = (uv cache dir)
    cache_hit = $CacheHit
    install_directory = $env:UV_PYTHON_INSTALL_DIR
    install_exit_code = $installExitCode
    install_seconds = $stopwatch.Elapsed.TotalSeconds
    combined_seconds = $combinedSeconds
    verified = $false
}
if ($installExitCode -ne 0) {
    $result["entries"] = @(Get-ChildItem -LiteralPath $env:UV_PYTHON_INSTALL_DIR |
        Select-Object Name, Attributes, LinkType, Target)
    $result | ConvertTo-Json -Depth 5 | Set-Content "python-bootstrap-$Phase.json"
    $result | ConvertTo-Json -Depth 5
    if ($Mode -eq "d-warm") {
        # Record a failed cache-restored installation without losing the cold measurements.
        exit 0
    }
    throw "uv python install failed"
}

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

# Minor-version junctions are aliases, not additional installations.
$installations = @(Get-ChildItem -LiteralPath $env:UV_PYTHON_INSTALL_DIR -Directory -Filter "cpython-*" |
    Where-Object { ($_.Attributes -band [System.IO.FileAttributes]::ReparsePoint) -eq 0 })
if ($installations.Count -ne $versions.Count) {
    throw "Expected $($versions.Count) installations, got $($installations.Count)"
}
$files = @(Get-ChildItem -LiteralPath $env:UV_PYTHON_INSTALL_DIR -File -Recurse)
$result["verified"] = $true
$result["installation_count"] = $installations.Count
$result["installed_file_count"] = $files.Count
$result["installed_bytes"] = ($files | Measure-Object -Property Length -Sum).Sum
$result["interpreters"] = $verified
$result | ConvertTo-Json -Depth 5 | Set-Content "python-bootstrap-$Phase.json"
$result | ConvertTo-Json -Depth 5
