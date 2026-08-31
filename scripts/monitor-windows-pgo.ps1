param(
    [Parameter(Mandatory = $true)][string]$OutputPath,
    [Parameter(Mandatory = $true)][string]$StopPath
)

$ErrorActionPreference = 'Continue'
while (-not (Test-Path $StopPath)) {
    $operatingSystem = Get-CimInstance Win32_OperatingSystem
    $processes = @(Get-Process -ErrorAction SilentlyContinue |
        Where-Object { $_.ProcessName -match '^(rustc|cargo|cargo-auditable|uv|uvx|uvw|link|lld-link|python|MsMpEng)$' } |
        Select-Object Id, ProcessName, CPU, WorkingSet64, PeakWorkingSet64)
    $disks = @(Get-CimInstance Win32_PerfFormattedData_PerfDisk_PhysicalDisk -ErrorAction SilentlyContinue |
        Select-Object Name, DiskReadBytesPersec, DiskWriteBytesPersec, PercentDiskTime)
    @{
        timestamp = [DateTime]::UtcNow.ToString('o')
        freeMemoryKiB = $operatingSystem.FreePhysicalMemory
        processes = $processes
        disks = $disks
    } | ConvertTo-Json -Compress -Depth 5 | Add-Content -Path $OutputPath -Encoding utf8
    Start-Sleep -Seconds 10
}
