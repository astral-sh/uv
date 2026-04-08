<#
.SYNOPSIS
Install a package with winget with additional retries.

.DESCRIPTION
winget already performs internal retries for some network/download failures.
For the installer download path used by `winget install`, upstream currently
uses `MaxRetryCount = 2` in this loop [1].

That corresponds to 2 total attempts (initial attempt + 1 retry).
This script adds an outer retry loop for CI, where transient errors can still
cause winget to fail (for example: "InternetOpenUrl() failed").

[1]: https://github.com/microsoft/winget-cli/blob/87949cef103975859167c9122f8f3cfa84c5e56f/src/AppInstallerCLICore/Workflows/DownloadFlow.cpp#L430-L432
#>

param(
    [Parameter(Mandatory = $true)]
    [string]$Package
)

$maxAttempts = 5
$delaySeconds = 20

for ($attempt = 1; $attempt -le $maxAttempts; $attempt++) {
    Write-Host "Installing $Package with winget (attempt $attempt/$maxAttempts)..."

    try {
        winget install $Package --accept-package-agreements --accept-source-agreements --disable-interactivity
        $exitCode = $LASTEXITCODE
    }
    catch {
        $exitCode = 1
        Write-Warning "winget threw an exception: $($_.Exception.Message)"
    }

    if ($exitCode -eq 0) {
        exit 0
    }

    if ($attempt -eq $maxAttempts) {
        throw "Failed to install $Package after $maxAttempts attempts (last exit code: $exitCode)."
    }

    Write-Warning "Attempt $attempt failed with exit code $exitCode. Retrying in $delaySeconds seconds..."
    Start-Sleep -Seconds $delaySeconds
}
