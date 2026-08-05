$ErrorActionPreference = "Stop"

# Test that uv can install packages with entrypoints on Windows NanoServer.
#
# Windows NanoServer does not support the `BeginUpdateResourceW`,
# `UpdateResourceW`, and `EndUpdateResourceW` APIs that are used to embed
# metadata in trampoline executables. uv must fall back to the legacy
# trampoline format on this platform.
#
# See: https://github.com/astral-sh/uv/issues/18663

Write-Host "Testing uv on Windows NanoServer"

# Add the mounted binary directory to PATH
$env:PATH = "C:\uv;" + $env:PATH

# Verify uv is available
uv --version
if ($LASTEXITCODE -ne 0) { throw "uv --version failed" }

# Create a temporary project
uv init C:\test-project
if ($LASTEXITCODE -ne 0) { throw "uv init failed" }

# Install a package that has console_scripts entry points.
# `pytest` is a pure-Python package that registers a `pytest` console script,
# which exercises the trampoline creation code path.
uv --directory C:\test-project add pytest
if ($LASTEXITCODE -ne 0) { throw "uv add pytest failed" }

Write-Host "Successfully installed package with entrypoints on Windows NanoServer"
