$ErrorActionPreference = "Stop"

$repo = (Get-Location).Path
$root = Join-Path $env:RUNNER_TEMP "mimalloc-build"
$artifact = Join-Path $root "artifact"
$v2Target = Join-Path $root "target-v2"
$v3Target = Join-Path $root "target-v3"
$v3NoLargePagesTarget = Join-Path $root "target-v3-no-large-pages"

New-Item -ItemType Directory -Force $artifact | Out-Null

$manifest = Join-Path $repo "crates/uv-performance-memory-allocator/Cargo.toml"
$original = Get-Content $manifest -Raw
$v2Manifest = $original.Replace(
    'mimalloc = { version = "0.1.52" }',
    'mimalloc = { version = "0.1.52", features = ["v2"] }'
)
if ($original -eq $v2Manifest) {
    throw "Failed to enable mimalloc v2 for the baseline build"
}

try {
    $v2Manifest | Set-Content $manifest -NoNewline
    cargo build `
        --profile profiling `
        --locked `
        --package uv `
        --bin uv `
        --target-dir $v2Target
    if ($LASTEXITCODE -ne 0) {
        throw "The mimalloc v2 build failed with exit code $LASTEXITCODE"
    }
} finally {
    $original | Set-Content $manifest -NoNewline
}

cargo build `
    --profile profiling `
    --locked `
    --package uv `
    --bin uv `
    --target-dir $v3Target
if ($LASTEXITCODE -ne 0) {
    throw "The mimalloc v3 build failed with exit code $LASTEXITCODE"
}

$originalCxxFlags = $env:CXXFLAGS
try {
    $env:CXXFLAGS = "/DMI_ENABLE_LARGE_PAGES=0"
    cargo build `
        --profile profiling `
        --locked `
        --package uv `
        --bin uv `
        --target-dir $v3NoLargePagesTarget
    if ($LASTEXITCODE -ne 0) {
        throw "The mimalloc v3 no-large-pages build failed with exit code $LASTEXITCODE"
    }
} finally {
    if ($null -eq $originalCxxFlags) {
        Remove-Item Env:CXXFLAGS -ErrorAction SilentlyContinue
    } else {
        $env:CXXFLAGS = $originalCxxFlags
    }
}

$v2 = Join-Path $artifact "uv-v2.exe"
$v3 = Join-Path $artifact "uv-v3.exe"
$v3NoLargePages = Join-Path $artifact "uv-v3-no-large-pages.exe"
Copy-Item (Join-Path $v2Target "profiling/uv.exe") $v2
Copy-Item (Join-Path $v3Target "profiling/uv.exe") $v3
Copy-Item `
    (Join-Path $v3NoLargePagesTarget "profiling/uv.exe") `
    $v3NoLargePages

foreach ($binary in @($v2, $v3, $v3NoLargePages)) {
    & $binary --version
    if ($LASTEXITCODE -ne 0) {
        throw "The benchmark binary failed to start: $binary"
    }
}

[pscustomobject]@{
    commit = git rev-parse HEAD
    rustc = rustc --version --verbose
    cargo = cargo --version
    v2 = [pscustomobject]@{
        bytes = (Get-Item $v2).Length
        sha256 = (Get-FileHash -Algorithm SHA256 $v2).Hash
    }
    v3 = [pscustomobject]@{
        bytes = (Get-Item $v3).Length
        sha256 = (Get-FileHash -Algorithm SHA256 $v3).Hash
    }
    v3NoLargePages = [pscustomobject]@{
        bytes = (Get-Item $v3NoLargePages).Length
        sha256 = (Get-FileHash -Algorithm SHA256 $v3NoLargePages).Hash
        cxxflags = "/DMI_ENABLE_LARGE_PAGES=0"
    }
} | ConvertTo-Json -Depth 4 | Set-Content (Join-Path $artifact "build-metadata.json")
