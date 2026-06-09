$ErrorActionPreference = "Stop"

$repo = (Get-Location).Path
$replica = [int]$env:BENCH_REPLICA
$root = Join-Path $env:RUNNER_TEMP "mimalloc-rss"
$results = Join-Path $root "results"
$binaryRoot = Join-Path $env:RUNNER_TEMP "mimalloc-binaries"
$v2 = Join-Path $binaryRoot "uv-v2.exe"
$v3 = Join-Path $binaryRoot "uv-v3.exe"
$v3NoLargePages = Join-Path $binaryRoot "uv-v3-no-large-pages.exe"
$smoke = $env:BENCH_SMOKE -eq "1"

New-Item -ItemType Directory -Force $results | Out-Null

$treatments = @(
    [pscustomobject]@{
        Code = "A"
        Name = "v2-v2-policy"
        Variant = "v2"
        Policy = "v2"
        Binary = $v2
        LargePages = "v2"
        PageReclaimOnFree = "native"
        PurgeDelay = "10"
        ArenaPurgeMult = "10"
        EnvironmentOverride = $false
    }
    [pscustomobject]@{
        Code = "B"
        Name = "v3-default"
        Variant = "v3"
        Policy = "v2"
        Binary = $v3
        LargePages = "enabled"
        PageReclaimOnFree = "native"
        PurgeDelay = "10"
        ArenaPurgeMult = "10"
        EnvironmentOverride = $true
    }
    [pscustomobject]@{
        Code = "C"
        Name = "v3-reclaim-disabled"
        Variant = "v3"
        Policy = "v2"
        Binary = $v3
        LargePages = "enabled"
        PageReclaimOnFree = "-1"
        PurgeDelay = "10"
        ArenaPurgeMult = "10"
        EnvironmentOverride = $true
    }
    [pscustomobject]@{
        Code = "D"
        Name = "v3-large-pages-disabled"
        Variant = "v3"
        Policy = "v2"
        Binary = $v3NoLargePages
        LargePages = "disabled"
        PageReclaimOnFree = "native"
        PurgeDelay = "10"
        ArenaPurgeMult = "10"
        EnvironmentOverride = $true
    }
)

function Set-MimallocTreatmentEnvironment {
    param([pscustomobject]$Treatment)

    Remove-Item Env:MIMALLOC_PURGE_DELAY -ErrorAction SilentlyContinue
    Remove-Item Env:MIMALLOC_ARENA_PURGE_MULT -ErrorAction SilentlyContinue
    Remove-Item Env:MIMALLOC_PAGE_RECLAIM_ON_FREE -ErrorAction SilentlyContinue
    if ($Treatment.EnvironmentOverride) {
        $env:MIMALLOC_PURGE_DELAY = $Treatment.PurgeDelay
        $env:MIMALLOC_ARENA_PURGE_MULT = $Treatment.ArenaPurgeMult
    }
    if ($Treatment.PageReclaimOnFree -ne "native") {
        $env:MIMALLOC_PAGE_RECLAIM_ON_FREE = $Treatment.PageReclaimOnFree
    }
    $env:MIMALLOC_PURGE_DECOMMITS = "1"
}

foreach ($binary in @($v2, $v3, $v3NoLargePages)) {
    if (-not (Test-Path $binary)) {
        throw "Missing benchmark binary: $binary"
    }
    & $binary --version
    if ($LASTEXITCODE -ne 0) {
        throw "The benchmark binary failed to start: $binary"
    }
}

foreach ($treatment in $treatments) {
    Set-MimallocTreatmentEnvironment $treatment
    $env:MIMALLOC_VERBOSE = "1"
    $binary = $treatment.Binary
    & $binary --version 2>&1 |
        Set-Content (Join-Path $results "mimalloc-options-$($treatment.Code).txt")
    if ($LASTEXITCODE -ne 0) {
        throw "The policy probe failed for $($treatment.Name)"
    }
}
Remove-Item Env:MIMALLOC_VERBOSE

function Get-CommandOutput {
    param([scriptblock]$Command)

    try {
        return (& $Command 2>&1 | ForEach-Object ToString)
    } catch {
        return @("ERROR: $($_.Exception.Message)")
    }
}

[pscustomobject]@{
    replica = $replica
    runnerName = $env:RUNNER_NAME
    machineName = $env:COMPUTERNAME
    commit = git rev-parse HEAD
    timestamp = (Get-Date).ToUniversalTime().ToString("O")
    operatingSystem = Get-CimInstance Win32_OperatingSystem |
        Select-Object Caption, Version, BuildNumber, OSArchitecture, LastBootUpTime
    computerSystem = Get-CimInstance Win32_ComputerSystem |
        Select-Object Manufacturer, Model, NumberOfLogicalProcessors, TotalPhysicalMemory
    processors = @(Get-CimInstance Win32_Processor |
        Select-Object Name, NumberOfCores, NumberOfLogicalProcessors, MaxClockSpeed)
    powerScheme = Get-CommandOutput { powercfg /getactivescheme }
    defender = Get-CommandOutput {
        Get-MpComputerStatus |
            Select-Object AntivirusEnabled, RealTimeProtectionEnabled, `
                AntivirusSignatureVersion, AntivirusSignatureLastUpdated |
            ConvertTo-Json
    }
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
    }
} | ConvertTo-Json -Depth 6 | Set-Content (Join-Path $results "machine-metadata.json")

try {
    Get-Counter '\Processor(_Total)\% Processor Time' `
        -SampleInterval 1 `
        -MaxSamples 10 |
        ForEach-Object {
            $timestamp = $_.Timestamp.ToUniversalTime().ToString("O")
            foreach ($sample in $_.CounterSamples) {
                [pscustomobject]@{
                    timestampUtc = $timestamp
                    path = $sample.Path
                    value = $sample.CookedValue
                }
            }
        } |
        Export-Csv -Path (Join-Path $results "idle-cpu.csv") -NoTypeInformation
} catch {
    $_ | Out-String | Set-Content (Join-Path $results "idle-cpu-error.txt")
}

function New-PipCase {
    param(
        [string]$Name,
        [string]$Source,
        [string]$PythonVersion,
        [string]$Constraint = $null
    )

    return [pscustomobject]@{
        Name = "pip-$Name"
        Kind = "pip-compile"
        Source = $Source
        Constraint = $Constraint
        PythonVersion = $PythonVersion
        Cutoff = "2024-10-14"
    }
}

function New-LockCase {
    param(
        [string]$Name,
        [string]$Source
    )

    return [pscustomobject]@{
        Name = "lock-$Name"
        Kind = "lock"
        Source = $Source
        Constraint = $null
        PythonVersion = $null
        Cutoff = "2024-08-08T00:00:00Z"
    }
}

$corpus = @(
    New-PipCase "black" "test/requirements/black.in" "3.12"
    New-PipCase "trio" "test/requirements/trio.in" "3.12"
    New-PipCase "boto3" "test/requirements/boto3.in" "3.12"
    New-PipCase "scispacy" "test/requirements/scispacy.in" "3.12"
    New-PipCase "flyte" "test/requirements/flyte.in" "3.11"
    New-PipCase "jupyter" "test/requirements/jupyter.in" "3.12"
    New-PipCase "airflow" "test/requirements/airflow.in" "3.12"
    New-PipCase `
        "airflow2" `
        "test/requirements/airflow2-req.in" `
        "3.8" `
        "test/requirements/airflow2-constraints.txt"
    New-PipCase "slow" "test/requirements/slow.in" "3.11"
    New-PipCase "bio-embeddings" "test/requirements/bio_embeddings.in" "3.12"
    New-PipCase `
        "backtrack-numpy-numba" `
        "test/requirements/backtracking/numpy-numba.in" `
        "3.12"
    New-PipCase `
        "backtrack-numpy-sparse" `
        "test/requirements/backtracking/numpy-sparse.in" `
        "3.12"
    New-PipCase `
        "backtrack-sentry" `
        "test/requirements/backtracking/sentry.in" `
        "3.12"
    New-PipCase `
        "backtrack-starlette-fastapi" `
        "test/requirements/backtracking/starlette-fastapi.in" `
        "3.12"
    New-PipCase `
        "meine-stadt" `
        "test/requirements/meine_stadt_transparent.in" `
        "3.12"
    New-PipCase "pdm-2193" "test/requirements/pdm_2193.in" "3.11"
    New-LockCase `
        "github-wikidata-bot" `
        "test/ecosystem/github-wikidata-bot"
    New-LockCase "black" "test/ecosystem/black"
    New-LockCase "packse" "test/ecosystem/packse"
    New-LockCase "home-assistant-core" "test/ecosystem/home-assistant-core"
    New-LockCase "saleor" "test/ecosystem/saleor"
)

$selectedFixtures = @(
    "pip-black"
    "pip-boto3"
    "pip-jupyter"
    "pip-airflow"
    "pip-bio-embeddings"
    "pip-backtrack-numpy-numba"
    "lock-packse"
    "lock-saleor"
)
$corpus = @($corpus | Where-Object Name -in $selectedFixtures)

$coldOnlineWarmupBlocks = 4
$coldOnlineMeasuredBlocks = 8
if ($smoke) {
    $corpus = @($corpus | Select-Object -First 1)
    $coldOnlineWarmupBlocks = 4
    $coldOnlineMeasuredBlocks = 4
}

[pscustomobject]@{
    design = "four-treatment residual RSS Williams crossover: ABDC, BCAD, CDBA, DACB"
    smoke = $smoke
    metric = "per-process PeakWorkingSetSize from GetProcessMemoryInfo"
    launcher = "CreateProcessW suspended, then ResumeThread and retain process handle"
    coldOnline = "empty per-observation uv cache, fresh output or project, online"
    coldOnlineWarmupBlocks = $coldOnlineWarmupBlocks
    coldOnlineMeasuredBlocks = $coldOnlineMeasuredBlocks
    purgeDecommits = 1
    treatments = @($treatments |
        Select-Object Code, Name, Variant, Policy, LargePages,
            PageReclaimOnFree, PurgeDelay, ArenaPurgeMult, EnvironmentOverride)
    corpus = $corpus
} | ConvertTo-Json -Depth 6 | Set-Content (Join-Path $results "protocol.json")

function Shuffle-Array {
    param(
        [object[]]$Values,
        [int]$Seed
    )

    $random = [System.Random]::new($Seed)
    $copy = @($Values)
    for ($index = $copy.Count - 1; $index -gt 0; $index--) {
        $swapIndex = $random.Next($index + 1)
        $temporary = $copy[$index]
        $copy[$index] = $copy[$swapIndex]
        $copy[$swapIndex] = $temporary
    }
    return $copy
}

function New-BlockSchedule {
    param(
        [int]$Count,
        [int]$Seed
    )

    if ($Count % 4 -ne 0) {
        throw "Williams crossover block counts must be divisible by four"
    }
    $sequences = @()
    for ($index = 0; $index -lt ($Count / 4); $index++) {
        $sequences += "ABDC"
        $sequences += "BCAD"
        $sequences += "CDBA"
        $sequences += "DACB"
    }
    return (Shuffle-Array $sequences $Seed)
}

function New-UvArguments {
    param(
        [pscustomobject]$Case,
        [string]$CacheDir,
        [string]$OutputPath,
        [string]$ProjectDir
    )

    $arguments = @("--no-config", "--quiet", "--cache-dir", $CacheDir)
    if ($Case.Kind -eq "pip-compile") {
        $arguments += @(
            "pip", "compile", (Join-Path $repo $Case.Source),
            "--universal",
            "--python-version", $Case.PythonVersion,
            "--exclude-newer", $Case.Cutoff,
            "--no-header",
            "--output-file", $OutputPath
        )
        if ($Case.Constraint) {
            $arguments += @("--constraints", (Join-Path $repo $Case.Constraint))
        }
    } else {
        $arguments += @(
            "lock",
            "--project", $ProjectDir,
            "--exclude-newer", $Case.Cutoff
        )
    }
    return $arguments
}

function Invoke-Uv {
    param(
        [string]$Name,
        [pscustomobject]$Treatment,
        [pscustomobject]$Case,
        [string]$CacheDir,
        [string]$OutputPath,
        [string]$ProjectDir
    )

    $arguments = New-UvArguments `
        -Case $Case `
        -CacheDir $CacheDir `
        -OutputPath $OutputPath `
        -ProjectDir $ProjectDir
    Set-MimallocTreatmentEnvironment $Treatment
    $binary = $Treatment.Binary
    & $binary @arguments | Out-Null
    if ($LASTEXITCODE -ne 0) {
        throw "$Name failed with exit code $LASTEXITCODE"
    }
}

function New-ProjectCopy {
    param(
        [pscustomobject]$Case,
        [string]$Destination
    )

    Copy-Item (Join-Path $repo $Case.Source) $Destination -Recurse
    Remove-Item (Join-Path $Destination "uv.lock") -Force -ErrorAction SilentlyContinue
}

function Prime-Case {
    param(
        [pscustomobject]$Case,
        [string]$CaseRoot,
        [string]$WarmCache
    )

    Remove-Item $WarmCache -Recurse -Force -ErrorAction SilentlyContinue
    New-Item -ItemType Directory -Force $WarmCache | Out-Null

    $primeRoot = Join-Path $CaseRoot "prime"
    Remove-Item $primeRoot -Recurse -Force -ErrorAction SilentlyContinue
    New-Item -ItemType Directory -Force $primeRoot | Out-Null

    if ($Case.Kind -eq "pip-compile") {
        $primeOutput = Join-Path $primeRoot "prime.txt"
        Invoke-Uv "$($Case.Name) cache prime" `
            $treatments[0] $Case $WarmCache $primeOutput $null
        $validationOutputs = @()
        foreach ($treatment in $treatments) {
            $output = Join-Path $primeRoot "$($treatment.Code).txt"
            Invoke-Uv "$($Case.Name) $($treatment.Name) warm validation" `
                $treatment $Case $WarmCache $output $null
            $validationOutputs += $output
        }
    } else {
        $primeProject = Join-Path $primeRoot "prime"
        New-ProjectCopy $Case $primeProject
        Invoke-Uv "$($Case.Name) cache prime" `
            $treatments[0] $Case $WarmCache $null $primeProject
        $validationOutputs = @()
        foreach ($treatment in $treatments) {
            $project = Join-Path $primeRoot $treatment.Code
            New-ProjectCopy $Case $project
            Invoke-Uv "$($Case.Name) $($treatment.Name) warm validation" `
                $treatment $Case $WarmCache $null $project
            $validationOutputs += (Join-Path $project "uv.lock")
        }
    }

    $hashes = @($validationOutputs | ForEach-Object {
        (Get-FileHash -Algorithm SHA256 $_).Hash
    } | Sort-Object -Unique)
    if ($hashes.Count -ne 1) {
        throw "$($Case.Name) produced different treatment outputs during validation"
    }
    return $hashes[0]
}

Add-Type -TypeDefinition @'
using System;
using System.ComponentModel;
using System.Runtime.InteropServices;
using System.Text;

public sealed class PeakWorkingSetMeasurement
{
    public uint ExitCode { get; set; }
    public ulong PeakWorkingSetBytes { get; set; }
    public int MemoryQueryCount { get; set; }
    public bool FinalMemoryQuerySucceeded { get; set; }
    public int FinalMemoryQueryError { get; set; }
}

public static class SuspendedProcessMemory
{
    private const uint CREATE_SUSPENDED = 0x00000004;
    private const uint CREATE_UNICODE_ENVIRONMENT = 0x00000400;
    private const uint WAIT_OBJECT_0 = 0x00000000;
    private const uint WAIT_TIMEOUT = 0x00000102;
    private const uint WAIT_FAILED = 0xffffffff;

    [StructLayout(LayoutKind.Sequential, CharSet = CharSet.Unicode)]
    private struct STARTUPINFO
    {
        public uint cb;
        public string lpReserved;
        public string lpDesktop;
        public string lpTitle;
        public uint dwX;
        public uint dwY;
        public uint dwXSize;
        public uint dwYSize;
        public uint dwXCountChars;
        public uint dwYCountChars;
        public uint dwFillAttribute;
        public uint dwFlags;
        public ushort wShowWindow;
        public ushort cbReserved2;
        public IntPtr lpReserved2;
        public IntPtr hStdInput;
        public IntPtr hStdOutput;
        public IntPtr hStdError;
    }

    [StructLayout(LayoutKind.Sequential)]
    private struct PROCESS_INFORMATION
    {
        public IntPtr hProcess;
        public IntPtr hThread;
        public uint dwProcessId;
        public uint dwThreadId;
    }

    [StructLayout(LayoutKind.Sequential)]
    private struct PROCESS_MEMORY_COUNTERS
    {
        public uint cb;
        public uint PageFaultCount;
        public UIntPtr PeakWorkingSetSize;
        public UIntPtr WorkingSetSize;
        public UIntPtr QuotaPeakPagedPoolUsage;
        public UIntPtr QuotaPagedPoolUsage;
        public UIntPtr QuotaPeakNonPagedPoolUsage;
        public UIntPtr QuotaNonPagedPoolUsage;
        public UIntPtr PagefileUsage;
        public UIntPtr PeakPagefileUsage;
    }

    [DllImport("kernel32.dll", CharSet = CharSet.Unicode, SetLastError = true)]
    private static extern bool CreateProcessW(
        string lpApplicationName,
        StringBuilder lpCommandLine,
        IntPtr lpProcessAttributes,
        IntPtr lpThreadAttributes,
        bool bInheritHandles,
        uint dwCreationFlags,
        IntPtr lpEnvironment,
        string lpCurrentDirectory,
        ref STARTUPINFO lpStartupInfo,
        out PROCESS_INFORMATION lpProcessInformation);

    [DllImport("kernel32.dll", SetLastError = true)]
    private static extern uint ResumeThread(IntPtr hThread);

    [DllImport("kernel32.dll", SetLastError = true)]
    private static extern uint WaitForSingleObject(IntPtr hHandle, uint dwMilliseconds);

    [DllImport("kernel32.dll", SetLastError = true)]
    private static extern bool GetExitCodeProcess(IntPtr hProcess, out uint lpExitCode);

    [DllImport("kernel32.dll", SetLastError = true)]
    private static extern bool CloseHandle(IntPtr hObject);

    [DllImport("psapi.dll", SetLastError = true)]
    private static extern bool GetProcessMemoryInfo(
        IntPtr Process,
        ref PROCESS_MEMORY_COUNTERS ppsmemCounters,
        uint cb);

    private static string QuoteArgument(string value)
    {
        StringBuilder quoted = new StringBuilder();
        quoted.Append('"');
        int backslashes = 0;
        foreach (char character in value)
        {
            if (character == '\\')
            {
                backslashes++;
                continue;
            }
            if (character == '"')
            {
                quoted.Append('\\', (backslashes * 2) + 1);
                quoted.Append('"');
                backslashes = 0;
                continue;
            }
            quoted.Append('\\', backslashes);
            quoted.Append(character);
            backslashes = 0;
        }
        quoted.Append('\\', backslashes * 2);
        quoted.Append('"');
        return quoted.ToString();
    }

    private static bool TryUpdatePeak(
        IntPtr process,
        ref ulong peakWorkingSetBytes,
        ref int queryCount,
        out int error)
    {
        PROCESS_MEMORY_COUNTERS counters = new PROCESS_MEMORY_COUNTERS();
        counters.cb = (uint)Marshal.SizeOf<PROCESS_MEMORY_COUNTERS>();
        if (!GetProcessMemoryInfo(process, ref counters, counters.cb))
        {
            error = Marshal.GetLastWin32Error();
            return false;
        }
        peakWorkingSetBytes = Math.Max(
            peakWorkingSetBytes,
            counters.PeakWorkingSetSize.ToUInt64());
        queryCount++;
        error = 0;
        return true;
    }

    public static PeakWorkingSetMeasurement Run(
        string applicationName,
        string[] arguments,
        string workingDirectory)
    {
        StringBuilder commandLine = new StringBuilder(QuoteArgument(applicationName));
        foreach (string argument in arguments)
        {
            commandLine.Append(' ');
            commandLine.Append(QuoteArgument(argument));
        }

        STARTUPINFO startupInfo = new STARTUPINFO();
        startupInfo.cb = (uint)Marshal.SizeOf<STARTUPINFO>();
        PROCESS_INFORMATION processInformation;
        if (!CreateProcessW(
            applicationName,
            commandLine,
            IntPtr.Zero,
            IntPtr.Zero,
            false,
            CREATE_SUSPENDED | CREATE_UNICODE_ENVIRONMENT,
            IntPtr.Zero,
            workingDirectory,
            ref startupInfo,
            out processInformation))
        {
            throw new Win32Exception(Marshal.GetLastWin32Error(), "CreateProcessW failed");
        }

        ulong peakWorkingSetBytes = 0;
        int queryCount = 0;
        int ignoredError;
        bool finalQuerySucceeded = false;
        int finalQueryError = 0;
        try
        {
            if (ResumeThread(processInformation.hThread) == uint.MaxValue)
            {
                throw new Win32Exception(Marshal.GetLastWin32Error(), "ResumeThread failed");
            }

            TryUpdatePeak(
                processInformation.hProcess,
                ref peakWorkingSetBytes,
                ref queryCount,
                out ignoredError);
            while (true)
            {
                uint waitResult = WaitForSingleObject(processInformation.hProcess, 1);
                if (waitResult == WAIT_OBJECT_0)
                {
                    break;
                }
                if (waitResult == WAIT_FAILED)
                {
                    throw new Win32Exception(
                        Marshal.GetLastWin32Error(),
                        "WaitForSingleObject failed");
                }
                if (waitResult != WAIT_TIMEOUT)
                {
                    throw new InvalidOperationException(
                        "Unexpected process wait result: " + waitResult);
                }
                TryUpdatePeak(
                    processInformation.hProcess,
                    ref peakWorkingSetBytes,
                    ref queryCount,
                    out ignoredError);
            }

            finalQuerySucceeded = TryUpdatePeak(
                processInformation.hProcess,
                ref peakWorkingSetBytes,
                ref queryCount,
                out finalQueryError);
            if (!finalQuerySucceeded)
            {
                throw new Win32Exception(
                    finalQueryError,
                    "The final process memory query failed");
            }
            uint exitCode;
            if (!GetExitCodeProcess(processInformation.hProcess, out exitCode))
            {
                throw new Win32Exception(
                    Marshal.GetLastWin32Error(),
                    "GetExitCodeProcess failed");
            }
            return new PeakWorkingSetMeasurement
            {
                ExitCode = exitCode,
                PeakWorkingSetBytes = peakWorkingSetBytes,
                MemoryQueryCount = queryCount,
                FinalMemoryQuerySucceeded = finalQuerySucceeded,
                FinalMemoryQueryError = finalQueryError,
            };
        }
        finally
        {
            CloseHandle(processInformation.hThread);
            CloseHandle(processInformation.hProcess);
        }
    }
}
'@

$rows = [System.Collections.Generic.List[object]]::new()
$scheduleRows = [System.Collections.Generic.List[object]]::new()

function Invoke-RssBlocks {
    param(
        [pscustomobject]$Case,
        [int]$CaseIndex,
        [string]$CaseRoot,
        [string]$WarmCache,
        [string]$ExpectedOutputSha256,
        [string]$CacheState,
        [string]$Phase,
        [int]$BlockCount,
        [int]$Seed
    )

    $schedules = New-BlockSchedule $BlockCount $Seed
    for ($blockIndex = 0; $blockIndex -lt $schedules.Count; $blockIndex++) {
        $sequenceName = $schedules[$blockIndex]
        $blockNumber = $blockIndex + 1
        $blockRoot = Join-Path `
            $CaseRoot `
            "$CacheState-$Phase-$($blockNumber.ToString('D3'))"
        Remove-Item $blockRoot -Recurse -Force -ErrorAction SilentlyContinue
        New-Item -ItemType Directory -Force $blockRoot | Out-Null

        $blockStartedAtUtc = (Get-Date).ToUniversalTime().ToString("O")
        $hashes = @()
        for ($positionIndex = 0; $positionIndex -lt 4; $positionIndex++) {
            $position = $positionIndex + 1
            $treatmentCode = [string]$sequenceName[$positionIndex]
            $treatment = $treatments |
                Where-Object Code -eq $treatmentCode |
                Select-Object -First 1
            if (-not $treatment) {
                throw "Unknown treatment code: $treatmentCode"
            }
            $cacheDir = if ($CacheState -eq "warm-online") {
                $WarmCache
            } else {
                Join-Path $blockRoot "cache-p$position"
            }

            if ($Case.Kind -eq "pip-compile") {
                $projectDir = $null
                $outputPath = Join-Path $blockRoot "output-p$position.txt"
            } else {
                $projectDir = Join-Path $blockRoot "project-p$position"
                New-ProjectCopy $Case $projectDir
                $outputPath = Join-Path $projectDir "uv.lock"
            }

            $arguments = New-UvArguments `
                -Case $Case `
                -CacheDir $cacheDir `
                -OutputPath $outputPath `
                -ProjectDir $projectDir
            Set-MimallocTreatmentEnvironment $treatment
            $measurement = [SuspendedProcessMemory]::Run(
                $treatment.Binary,
                $arguments,
                $repo
            )
            if ($measurement.ExitCode -ne 0) {
                throw "$($treatment.Name) failed with exit code " +
                    "$($measurement.ExitCode) for " +
                    "$($Case.Name) $CacheState $Phase block $blockNumber position $position"
            }
            if (-not $measurement.FinalMemoryQuerySucceeded) {
                throw "The post-exit memory query failed with Windows error " +
                    "$($measurement.FinalMemoryQueryError) for $($Case.Name) " +
                    "$CacheState $Phase block $blockNumber position $position"
            }

            $hash = (Get-FileHash -Algorithm SHA256 $outputPath).Hash
            $hashes += $hash
            $rows.Add([pscustomobject]@{
                replica = $replica
                runnerName = $env:RUNNER_NAME
                machineName = $env:COMPUTERNAME
                fixture = $Case.Name
                fixtureIndex = $CaseIndex
                kind = $Case.Kind
                cacheState = $CacheState
                phase = $Phase
                block = $blockNumber
                sequence = $sequenceName
                position = $position
                treatment = $treatment.Name
                treatmentCode = $treatment.Code
                variant = $treatment.Variant
                policy = $treatment.Policy
                largePages = $treatment.LargePages
                pageReclaimOnFree = $treatment.PageReclaimOnFree
                purgeDelayMs = $treatment.PurgeDelay
                arenaPurgeMult = $treatment.ArenaPurgeMult
                purgeDecommits = 1
                environmentOverride = $treatment.EnvironmentOverride
                peakWorkingSetBytes = $measurement.PeakWorkingSetBytes
                peakWorkingSetMiB = $measurement.PeakWorkingSetBytes / 1MB
                memoryQueryCount = $measurement.MemoryQueryCount
                finalMemoryQuerySucceeded = $measurement.FinalMemoryQuerySucceeded
                finalMemoryQueryError = $measurement.FinalMemoryQueryError
                outputSha256 = $hash
            })
        }
        $blockCompletedAtUtc = (Get-Date).ToUniversalTime().ToString("O")
        if (@($hashes | Sort-Object -Unique).Count -ne 1) {
            throw "$($Case.Name) produced different outputs in " +
                "$CacheState $Phase block $blockNumber"
        }
        if ($hashes[0] -ne $ExpectedOutputSha256) {
            throw "$($Case.Name) output changed after validation in " +
                "$CacheState $Phase block $blockNumber"
        }
        $scheduleRows.Add([pscustomobject]@{
            replica = $replica
            fixture = $Case.Name
            cacheState = $CacheState
            phase = $Phase
            block = $blockNumber
            sequence = $sequenceName
            seed = $Seed
            blockStartedAtUtc = $blockStartedAtUtc
            blockCompletedAtUtc = $blockCompletedAtUtc
        })

        $rows | Export-Csv -NoTypeInformation (Join-Path $results "observations.csv")
        $scheduleRows |
            Export-Csv -NoTypeInformation (Join-Path $results "schedule.csv")
        Remove-Item $blockRoot -Recurse -Force
    }
}

$orderedCorpus = Shuffle-Array $corpus (981000 + $replica)
$orderedCorpus |
    Select-Object Name, Kind, Source, Constraint, PythonVersion, Cutoff |
    ConvertTo-Json |
    Set-Content (Join-Path $results "fixture-order.json")

for ($caseIndex = 0; $caseIndex -lt $orderedCorpus.Count; $caseIndex++) {
    $case = $orderedCorpus[$caseIndex]
    Write-Host "::group::RSS benchmark $($case.Name)"
    $caseRoot = Join-Path $root $case.Name
    $warmCache = Join-Path $caseRoot "warm-cache"
    New-Item -ItemType Directory -Force $caseRoot | Out-Null

    $validatedHash = Prime-Case $case $caseRoot $warmCache
    [pscustomobject]@{
        fixture = $case.Name
        outputSha256 = $validatedHash
    } | ConvertTo-Json |
        Set-Content (Join-Path $results "validated-$($case.Name).json")

    Invoke-RssBlocks `
        -Case $case `
        -CaseIndex $caseIndex `
        -CaseRoot $caseRoot `
        -WarmCache $warmCache `
        -ExpectedOutputSha256 $validatedHash `
        -CacheState "cold-online" `
        -Phase "warmup" `
        -BlockCount $coldOnlineWarmupBlocks `
        -Seed (982000 + ($replica * 1000) + $caseIndex)
    Invoke-RssBlocks `
        -Case $case `
        -CaseIndex $caseIndex `
        -CaseRoot $caseRoot `
        -WarmCache $warmCache `
        -ExpectedOutputSha256 $validatedHash `
        -CacheState "cold-online" `
        -Phase "measured" `
        -BlockCount $coldOnlineMeasuredBlocks `
        -Seed (983000 + ($replica * 1000) + $caseIndex)
    Write-Host "::endgroup::"
}
