# Configures a drive for testing in CI.

# When not using a GitHub Actions "larger runner", the `D:` drive is present and
# has similar or better performance characteristics than a ReFS dev drive.
# Sometimes using a larger runner is still more performant (e.g., when running
# the test suite) and we need to create a dev drive. This script automatically
# configures the appropriate drive.

# Note we use `Get-PSDrive` is not sufficient because the drive letter is assigned.
if (Test-Path "D:\") {
    Write-Output "Using existing drive at D:"
    $Drive = "D:"
} else {
	# The size (20 GB) is chosen empirically to be large enough for our
	# workflows; larger drives can take longer to set up.
	$Volume = New-VHD -Path C:/uv_dev_drive.vhdx -SizeBytes 20GB |
						Mount-VHD -Passthru |
						Initialize-Disk -Passthru |
						New-Partition -AssignDriveLetter -UseMaximumSize |
						Format-Volume -DevDrive -Confirm:$false -Force

	$Drive = "$($Volume.DriveLetter):"

	# Set the drive as trusted
	# See https://learn.microsoft.com/en-us/windows/dev-drive/#how-do-i-designate-a-dev-drive-as-trusted
	fsutil devdrv trust $Drive

	# Disable antivirus filtering on dev drives
	# See https://learn.microsoft.com/en-us/windows/dev-drive/#how-do-i-configure-additional-filters-on-dev-drive
	fsutil devdrv enable /disallowAv

	# Remount so the changes take effect
	Dismount-VHD -Path C:/uv_dev_drive.vhdx
	Mount-VHD -Path C:/uv_dev_drive.vhdx

	# Show some debug information
	Write-Output $Volume
	fsutil devdrv query $Drive

    Write-Output "Using Dev Drive at $Volume"
}

$Tmp = "$($Drive)\uv-tmp"

# Create the directory ahead of time in an attempt to avoid race-conditions
New-Item $Tmp -ItemType Directory

# Move Cargo to the dev drive
New-Item -Path "$($Drive)/.cargo/bin" -ItemType Directory -Force
if (Test-Path "C:/Users/runneradmin/.cargo") {
    Copy-Item -Path "C:/Users/runneradmin/.cargo/*" -Destination "$($Drive)/.cargo/" -Recurse -Force
}

Write-Output `
	"DEV_DRIVE=$($Drive)" `
	"TMP=$($Tmp)" `
	"TEMP=$($Tmp)" `
	"UV_INTERNAL__TEST_DIR=$($Tmp)" `
	"RUSTUP_HOME=$($Drive)/.rustup" `
	"CARGO_HOME=$($Drive)/.cargo" `
	"UV_WORKSPACE=$($Drive)/uv" `
	"PATH=$($Drive)/.cargo/bin;$env:PATH" `
	>> $env:GITHUB_ENV

