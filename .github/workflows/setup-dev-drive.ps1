# Configures a drive for testing in CI.

# When not using a GitHub Actions "larger runner", the `D:` drive is present and
# has similar or better performance characteristics than a ReFS dev drive.
# Sometimes using a larger runner is still more performant (e.g., when running
# the test suite) and we need to create a dev drive. This script automatically
# configures the appropriate drive.

# Note we use `Get-PSDrive` is not sufficient because the drive letter is assigned.
if (Test-Path "D:\") {
    Write-Output "Using `D:` drive"
    $Drive = "D:"
} else {
    $Volume = New-VHD -Path C:/uv_dev_drive.vhdx -SizeBytes 20GB |
        Mount-VHD -Passthru |
        Initialize-Disk -Passthru |
        New-Partition -AssignDriveLetter -UseMaximumSize |
        Format-Volume -FileSystem ReFS -Confirm:$false -Force

    Write-Output "Using ReFS drive at $Volume"
    $Drive = "$($Volume.DriveLetter):"
}

$Tmp = "$($Drive)\uv-tmp"

# Create the directory ahead of time in an attempt to avoid race-conditions
New-Item $Tmp -ItemType Directory

Write-Output `
	"DEV_DRIVE=$($Drive)" `
	"TMP=$($Tmp)" `
	"TEMP=$($Tmp)" `
	"UV_INTERNAL__TEST_DIR=$($Tmp)" `
	"RUSTUP_HOME=$($Drive)/.rustup" `
	"CARGO_HOME=$($Drive)/.cargo" `
	"UV_WORKSPACE=$($Drive)/uv" `
	>> $env:GITHUB_ENV

