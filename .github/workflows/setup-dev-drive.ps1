# This uses `D:` as the workspace instead of `C:`, as the performance is much
# better. Previously, we created a ReFS Dev Drive, but this is actually faster.

$Drive = "D:"
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

