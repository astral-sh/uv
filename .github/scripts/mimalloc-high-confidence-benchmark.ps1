$ErrorActionPreference = "Stop"

$repo = (Get-Location).Path
$replica = [int]$env:BENCH_REPLICA
$root = Join-Path $env:RUNNER_TEMP "mimalloc-bench"
$results = Join-Path $root "results"
$rawResults = Join-Path $results "hyperfine"
$binaryRoot = Join-Path $env:RUNNER_TEMP "mimalloc-binaries"
$v2 = Join-Path $binaryRoot "uv-v2.exe"
$v3 = Join-Path $binaryRoot "uv-v3.exe"

New-Item -ItemType Directory -Force $results, $rawResults | Out-Null

foreach ($binary in @($v2, $v3)) {
    if (-not (Test-Path $binary)) {
        throw "Missing benchmark binary: $binary"
    }
    & $binary --version
    if ($LASTEXITCODE -ne 0) {
        throw "The benchmark binary failed to start: $binary"
    }
}

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
    hyperfine = hyperfine --version
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
} | ConvertTo-Json -Depth 6 | Set-Content (Join-Path $results "machine-metadata.json")

try {
    Get-Counter '\Processor(_Total)\% Processor Time' `
        -SampleInterval 1 `
        -MaxSamples 10 |
        Export-Counter -Path (Join-Path $results "idle-cpu.blg") -FileFormat BLG -Force
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

$warmOnlineWarmupBlocks = 4
$warmOnlineMeasuredBlocks = 16
$warmOfflineWarmupBlocks = 4
$warmOfflineMeasuredBlocks = 16
$coldWarmupBlocks = 2
$coldMeasuredBlocks = 4

[pscustomobject]@{
    design = "balanced ABBA/BAAB four-observation blocks"
    timer = "hyperfine 1 run per command with shell disabled"
    warmOnline = "populated per-fixture uv cache, fresh output or project, online"
    warmOffline = "populated per-fixture uv cache, fresh output or project, offline"
    coldOnline = "empty per-observation uv cache, fresh output or project, online"
    warmOnlineWarmupBlocks = $warmOnlineWarmupBlocks
    warmOnlineMeasuredBlocks = $warmOnlineMeasuredBlocks
    warmOfflineWarmupBlocks = $warmOfflineWarmupBlocks
    warmOfflineMeasuredBlocks = $warmOfflineMeasuredBlocks
    coldWarmupBlocks = $coldWarmupBlocks
    coldMeasuredBlocks = $coldMeasuredBlocks
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

    if ($Count % 2 -ne 0) {
        throw "Balanced block counts must be even"
    }
    $sequences = @()
    for ($index = 0; $index -lt ($Count / 2); $index++) {
        $sequences += "ABBA"
        $sequences += "BAAB"
    }
    return (Shuffle-Array $sequences $Seed)
}

function New-UvArguments {
    param(
        [pscustomobject]$Case,
        [string]$CacheDir,
        [string]$OutputPath,
        [string]$ProjectDir,
        [switch]$Offline
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
    if ($Offline) {
        $arguments += "--offline"
    }
    return $arguments
}

function Join-Command {
    param(
        [string]$Binary,
        [string[]]$Arguments
    )

    $tokens = @(@($Binary) + $Arguments) | ForEach-Object {
        $normalized = $_ -replace '\\', '/'
        if ($normalized -match '\s') {
            throw "The shell-free benchmark requires paths without spaces: $normalized"
        }
        $normalized
    }
    return $tokens -join " "
}

function Invoke-Uv {
    param(
        [string]$Name,
        [string]$Binary,
        [pscustomobject]$Case,
        [string]$CacheDir,
        [string]$OutputPath,
        [string]$ProjectDir,
        [switch]$Offline
    )

    $arguments = New-UvArguments `
        -Case $Case `
        -CacheDir $CacheDir `
        -OutputPath $OutputPath `
        -ProjectDir $ProjectDir `
        -Offline:$Offline
    & $Binary @arguments | Out-Null
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
        $onlineOutput = Join-Path $primeRoot "online.txt"
        $v2Output = Join-Path $primeRoot "v2.txt"
        $v3Output = Join-Path $primeRoot "v3.txt"
        Invoke-Uv "$($Case.Name) online prime" $v2 $Case $WarmCache $onlineOutput $null
        Invoke-Uv "$($Case.Name) v2 offline check" `
            $v2 $Case $WarmCache $v2Output $null -Offline
        Invoke-Uv "$($Case.Name) v3 offline check" `
            $v3 $Case $WarmCache $v3Output $null -Offline
    } else {
        $onlineProject = Join-Path $primeRoot "online"
        $v2Project = Join-Path $primeRoot "v2"
        $v3Project = Join-Path $primeRoot "v3"
        New-ProjectCopy $Case $onlineProject
        New-ProjectCopy $Case $v2Project
        New-ProjectCopy $Case $v3Project
        Invoke-Uv "$($Case.Name) online prime" `
            $v2 $Case $WarmCache $null $onlineProject
        Invoke-Uv "$($Case.Name) v2 offline check" `
            $v2 $Case $WarmCache $null $v2Project -Offline
        Invoke-Uv "$($Case.Name) v3 offline check" `
            $v3 $Case $WarmCache $null $v3Project -Offline
        $v2Output = Join-Path $v2Project "uv.lock"
        $v3Output = Join-Path $v3Project "uv.lock"
    }

    $v2Hash = (Get-FileHash -Algorithm SHA256 $v2Output).Hash
    $v3Hash = (Get-FileHash -Algorithm SHA256 $v3Output).Hash
    if ($v2Hash -ne $v3Hash) {
        throw "$($Case.Name) produced different v2 and v3 outputs during validation"
    }
    return $v2Hash
}

$rows = [System.Collections.Generic.List[object]]::new()
$scheduleRows = [System.Collections.Generic.List[object]]::new()

function Invoke-Blocks {
    param(
        [pscustomobject]$Case,
        [int]$CaseIndex,
        [string]$CaseRoot,
        [string]$WarmCache,
        [string]$CacheState,
        [string]$Phase,
        [int]$BlockCount,
        [int]$Seed
    )

    $schedules = New-BlockSchedule $BlockCount $Seed
    for ($blockIndex = 0; $blockIndex -lt $schedules.Count; $blockIndex++) {
        $sequenceName = $schedules[$blockIndex]
        $variants = if ($sequenceName -eq "ABBA") {
            @("v2", "v3", "v3", "v2")
        } else {
            @("v3", "v2", "v2", "v3")
        }

        $blockNumber = $blockIndex + 1
        $blockRoot = Join-Path `
            $CaseRoot `
            "$CacheState-$Phase-$($blockNumber.ToString('D3'))"
        Remove-Item $blockRoot -Recurse -Force -ErrorAction SilentlyContinue
        New-Item -ItemType Directory -Force $blockRoot | Out-Null

        $commands = @()
        $observations = @()
        for ($positionIndex = 0; $positionIndex -lt 4; $positionIndex++) {
            $position = $positionIndex + 1
            $variant = $variants[$positionIndex]
            $binary = if ($variant -eq "v2") { $v2 } else { $v3 }
            $cacheDir = if ($CacheState -like "warm-*") {
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
                -ProjectDir $projectDir `
                -Offline:($CacheState -eq "warm-offline")
            $commands += Join-Command $binary $arguments
            $observations += [pscustomobject]@{
                position = $position
                variant = $variant
                outputPath = $outputPath
            }
        }

        $jsonDir = Join-Path $rawResults $Case.Name
        New-Item -ItemType Directory -Force $jsonDir | Out-Null
        $jsonPath = Join-Path `
            $jsonDir `
            "$CacheState-$Phase-$($blockNumber.ToString('D3')).json"
        $hyperfineArguments = @(
            "--shell=none",
            "--runs", "1",
            "--style", "none",
            "--export-json", $jsonPath
        )
        for ($positionIndex = 0; $positionIndex -lt 4; $positionIndex++) {
            $observation = $observations[$positionIndex]
            $hyperfineArguments += @(
                "--command-name",
                "$($observation.variant)-p$($observation.position)",
                $commands[$positionIndex]
            )
        }

        $blockStartedAtUtc = (Get-Date).ToUniversalTime().ToString("O")
        & hyperfine @hyperfineArguments 2>&1 | Out-Null
        $blockCompletedAtUtc = (Get-Date).ToUniversalTime().ToString("O")
        if ($LASTEXITCODE -ne 0) {
            throw "Hyperfine failed for $($Case.Name) $CacheState $Phase block $blockNumber"
        }

        $hyperfine = Get-Content $jsonPath -Raw | ConvertFrom-Json
        $hashes = @()
        for ($positionIndex = 0; $positionIndex -lt 4; $positionIndex++) {
            $observation = $observations[$positionIndex]
            $hash = (Get-FileHash -Algorithm SHA256 $observation.outputPath).Hash
            $hashes += $hash
            $seconds = [double]$hyperfine.results[$positionIndex].times[0]
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
                position = $observation.position
                variant = $observation.variant
                seconds = $seconds
                blockStartedAtUtc = $blockStartedAtUtc
                blockCompletedAtUtc = $blockCompletedAtUtc
                outputSha256 = $hash
            })
        }
        if (@($hashes | Sort-Object -Unique).Count -ne 1) {
            throw "$($Case.Name) produced different outputs in $CacheState $Phase block $blockNumber"
        }
        $scheduleRows.Add([pscustomobject]@{
            replica = $replica
            fixture = $Case.Name
            cacheState = $CacheState
            phase = $Phase
            block = $blockNumber
            sequence = $sequenceName
            seed = $Seed
        })

        $rows | Export-Csv -NoTypeInformation (Join-Path $results "observations.csv")
        $scheduleRows |
            Export-Csv -NoTypeInformation (Join-Path $results "schedule.csv")
        Remove-Item $blockRoot -Recurse -Force
    }
}

$orderedCorpus = Shuffle-Array $corpus (910000 + $replica)
$orderedCorpus |
    Select-Object Name, Kind, Source, Constraint, PythonVersion, Cutoff |
    ConvertTo-Json |
    Set-Content (Join-Path $results "fixture-order.json")

$requestedCounters = @(
    '\Processor Information(_Total)\% Processor Time',
    '\Processor Information(_Total)\Processor Frequency',
    '\Memory\Available MBytes',
    '\PhysicalDisk(_Total)\% Disk Time'
)
$availableCounters = @()
$counterErrors = @()
foreach ($counter in $requestedCounters) {
    try {
        Get-Counter -Counter $counter -MaxSamples 1 -ErrorAction Stop | Out-Null
        $availableCounters += $counter
    } catch {
        $counterErrors += [pscustomobject]@{
            counter = $counter
            error = $_.Exception.Message
        }
    }
}
if ($availableCounters -notcontains '\Processor Information(_Total)\% Processor Time') {
    throw "The required processor utilization counter is unavailable"
}

$counterPath = Join-Path $results "machine-counters.blg"
[pscustomobject]@{
    requested = $requestedCounters
    available = $availableCounters
    errors = $counterErrors
} | ConvertTo-Json -Depth 4 | Set-Content (Join-Path $results "counter-selection.json")

$counterConfiguration = [pscustomobject]@{
    Path = $counterPath
    Counters = $availableCounters
}
$counterJob = Start-Job -ScriptBlock {
    param([pscustomobject]$Configuration)

    Get-Counter `
        -Counter $Configuration.Counters `
        -SampleInterval 5 `
        -Continuous `
        -ErrorAction Stop |
        Export-Counter `
            -Path $Configuration.Path `
            -FileFormat BLG `
            -Force `
            -ErrorAction Stop
} -ArgumentList $counterConfiguration

Start-Sleep -Seconds 10
if ($counterJob.State -ne "Running" -or -not (Test-Path $counterPath)) {
    $counterFailure = Receive-Job $counterJob 2>&1 | ForEach-Object ToString
    $counterFailure | Set-Content (Join-Path $results "machine-counters-job.txt")
    throw "The machine telemetry collector failed during startup"
}

try {
    Start-Sleep -Seconds 20

    for ($caseIndex = 0; $caseIndex -lt $orderedCorpus.Count; $caseIndex++) {
        $case = $orderedCorpus[$caseIndex]
        Write-Host "::group::Benchmark $($case.Name)"
        $caseRoot = Join-Path $root $case.Name
        $warmCache = Join-Path $caseRoot "warm-cache"
        New-Item -ItemType Directory -Force $caseRoot | Out-Null

        $validatedHash = Prime-Case $case $caseRoot $warmCache
        [pscustomobject]@{
            fixture = $case.Name
            outputSha256 = $validatedHash
        } | ConvertTo-Json |
            Set-Content (Join-Path $results "validated-$($case.Name).json")

        Invoke-Blocks `
            -Case $case `
            -CaseIndex $caseIndex `
            -CaseRoot $caseRoot `
            -WarmCache $warmCache `
            -CacheState "warm-online" `
            -Phase "warmup" `
            -BlockCount $warmOnlineWarmupBlocks `
            -Seed (920000 + ($replica * 1000) + $caseIndex)
        Invoke-Blocks `
            -Case $case `
            -CaseIndex $caseIndex `
            -CaseRoot $caseRoot `
            -WarmCache $warmCache `
            -CacheState "warm-online" `
            -Phase "measured" `
            -BlockCount $warmOnlineMeasuredBlocks `
            -Seed (930000 + ($replica * 1000) + $caseIndex)
        Invoke-Blocks `
            -Case $case `
            -CaseIndex $caseIndex `
            -CaseRoot $caseRoot `
            -WarmCache $warmCache `
            -CacheState "warm-offline" `
            -Phase "warmup" `
            -BlockCount $warmOfflineWarmupBlocks `
            -Seed (940000 + ($replica * 1000) + $caseIndex)
        Invoke-Blocks `
            -Case $case `
            -CaseIndex $caseIndex `
            -CaseRoot $caseRoot `
            -WarmCache $warmCache `
            -CacheState "warm-offline" `
            -Phase "measured" `
            -BlockCount $warmOfflineMeasuredBlocks `
            -Seed (950000 + ($replica * 1000) + $caseIndex)
        Invoke-Blocks `
            -Case $case `
            -CaseIndex $caseIndex `
            -CaseRoot $caseRoot `
            -WarmCache $warmCache `
            -CacheState "cold-online" `
            -Phase "warmup" `
            -BlockCount $coldWarmupBlocks `
            -Seed (960000 + ($replica * 1000) + $caseIndex)
        Invoke-Blocks `
            -Case $case `
            -CaseIndex $caseIndex `
            -CaseRoot $caseRoot `
            -WarmCache $warmCache `
            -CacheState "cold-online" `
            -Phase "measured" `
            -BlockCount $coldMeasuredBlocks `
            -Seed (970000 + ($replica * 1000) + $caseIndex)
        Write-Host "::endgroup::"
    }
} finally {
    $counterWasRunning = $counterJob.State -eq "Running"
    Stop-Job $counterJob
    Receive-Job $counterJob 2>&1 |
        ForEach-Object ToString |
        Set-Content (Join-Path $results "machine-counters-job.txt")
    Remove-Job $counterJob
    if (
        -not $counterWasRunning -or
        -not (Test-Path $counterPath) -or
        (Get-Item $counterPath).Length -eq 0
    ) {
        throw "The machine telemetry collector did not cover the complete benchmark"
    }
}
