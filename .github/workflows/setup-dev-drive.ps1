# Configures a drive for testing in CI.
#
# When using standard GitHub Actions runners, a `D:` drive is present and has
# similar or better performance characteristics than a ReFS dev drive. Sometimes
# using a larger runner is still more performant (e.g., when running the test
# suite) and we need to create a dev drive. This script automatically configures
# the appropriate drive.
#
# When using GitHub Actions' "larger runners", the `D:` drive is not present and
# we create a DevDrive mount on `C:`. This is purported to be more performant
# than an ReFS drive, though we did not see a change when we switched over.
#
# When using Depot runners, the underling infrastructure is EC2, which does not
# support Hyper-V. The `New-VHD` commandlet only works with Hyper-V, but we can
# create a ReFS drive using `diskpart` and `format` directory. We cannot use a
# DevDrive, as that also requires Hyper-V. The Depot runners use `D:` already,
# so we must check if it's a Depot runner first, and we use `V:` as the target
# instead.


if ($env:DEPOT_RUNNER -eq "1") {
    Write-Output "DEPOT_RUNNER detected, setting up custom dev drive..."

    # Create VHD and configure drive using diskpart
    $vhdPath = "C:\uv_dev_drive.vhdx"
    @"
create vdisk file="$vhdPath" maximum=25600 type=expandable
attach vdisk
create partition primary
active
assign letter=V
"@ | diskpart

    # Format the drive as ReFS
    format V: /fs:ReFS /q /y
    $Drive = "V:"

    Write-Output "Custom dev drive created at $Drive"
} elseif (Test-Path "D:\") {
	# Note `Get-PSDrive` is not sufficient because the drive letter is assigned.
    Write-Output "Using existing drive at D:"
    $Drive = "D:"
} else {
	# The size (25 GB) is chosen empirically to be large enough for our
	# workflows; larger drives can take longer to set up.
	$Volume = New-VHD -Path C:/uv_dev_drive.vhdx -SizeBytes 25GB |
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
	"RUSTUP_HOME=$($Drive)/.rustup" `
	"CARGO_HOME=$($Drive)/.cargo" `
	"UV_WORKSPACE=$($Drive)/uv" `
	"PATH=$($Drive)/.cargo/bin;$env:PATH" `
	>> $env:GITHUB_ENV
