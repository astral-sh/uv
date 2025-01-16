# This creates a 20GB dev drive, and exports all required environment
# variables so that rustup, uv and others all use the dev drive as much
# as possible.
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

# Configure a temporary directory
$Tmp = "$($Drive)/uv-tmp"

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

