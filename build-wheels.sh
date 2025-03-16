#!/usr/bin/env bash
# build-wheels.sh
#
# This script builds wheels for the project's dependencies.
#
# It uses uv to build wheels for the following packages:
# - `provider_fictional_hw`: A fictional hardware provider package
# - `variantlib`: A library for handling variant configurations
#
# The wheels are built and placed in the ./wheels directory.
#
# Any existing wheels are removed before building.

set -euxo pipefail

UV=./target/debug/uv

# Create the destination directory if it doesn't exist.
rm -rf wheels
mkdir wheels

# Build the wheels for the fictional hardware provider package.
$UV build --out-dir ./wheels --project ./vendor/provider_fictional_hw

# Build the wheels for the variant library.
$UV build --out-dir ./wheels --project ./vendor/variantlib
