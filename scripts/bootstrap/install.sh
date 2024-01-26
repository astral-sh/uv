#!/usr/bin/env bash
#
# Download required Python versions and install to `bin`
# Uses prebuilt Python distributions from indygreg/python-build-standalone
#
# Requirements
#
#   macOS
#
#       brew install zstd jq coreutils
#
#   Ubuntu
#
#       apt install zstd jq
#
#   Arch Linux
#
#       pacman -S zstd jq libxcrypt-compat
#
#   Windows
#
#       winget install jqlang.jq
#
# Usage
#
#   ./scripts/bootstrap/install.sh
#
# The Python versions are installed from `.python_versions`.
# Python versions are linked in-order such that the _last_ defined version will be the default.
#
# Version metadata can be updated with `fetch-version-metadata.py` which requires Python 3.12

set -euo pipefail

# Convenience function for displaying URLs
function urldecode() { : "${*//+/ }"; echo -e "${_//%/\\x}"; }

# Convenience function for checking that a command exists.
requires() {
  cmd="$1"
  if ! command -v "$cmd" > /dev/null 2>&1; then
    echo "DEPENDENCY MISSING: $(basename $0) requires $cmd to be installed" >&2
    exit 1
  fi
}

requires jq
requires zstd

# Setup some file paths
this_dir=$(realpath "$(dirname "$0")")
root_dir=$(dirname "$(dirname "$this_dir")")
bin_dir="$root_dir/bin"
install_dir="$bin_dir/versions"
versions_file="$root_dir/.python-versions"
versions_metadata="$this_dir/versions.json"

# Determine system metadata
os=$(uname -s | tr '[:upper:]' '[:lower:]')
arch=$(uname -m)
interpreter='cpython'

# On macOS, we need a newer version of `realpath` for `--relative-to` support
realpath="$(which grealpath || which realpath)"

# Read requested versions into an array
readarray -t versions < "$versions_file"

# Install each version
for version in "${versions[@]}"; do
    key="$interpreter-$version-$os-$arch"
    echo "Installing $key"

    url=$(jq --arg key "$key" '.[$key] | .url' -r < "$versions_metadata")

    if [ "$url" == 'null' ]; then
        echo "No matching download for $key"
        exit 1
    fi

    filename=$(basename "$url")
    echo "Downloading $(urldecode "$filename")"
    curl -L --progress-bar -o "$filename" "$url" --output-dir "$this_dir"

    expected_sha=$(jq --arg key "$key" '.[$key] | .sha256' -r < "$versions_metadata")
    if [ "$expected_sha" == 'null' ]; then
        echo "WARNING: no checksum for $key"
    else
        echo -n "Verifying checksum..."
        echo "$expected_sha $this_dir/$filename" | sha256sum -c --quiet
        echo " OK"
    fi

    install_key="$install_dir/$interpreter@$version"
    rm -rf "$install_key"
    echo "Extracting to $($realpath --relative-to="$root_dir" "$install_key")"
    mkdir -p "$install_key"
    zstd -d "$this_dir/$filename" --stdout | tar -x -C "$install_key"

    # Setup the installation
    mv "$install_key/python/"* "$install_key"
    # Use relative paths for links so if the bin is moved they don't break
    link=$($realpath --relative-to="$bin_dir" "$install_key/install/bin/python3")
    minor=$(jq --arg key "$key" '.[$key] | .minor' -r < "$versions_metadata")

    # Link as all version tuples, later versions in the file will take precedence
    ln -sf "./$link" "$bin_dir/python$version"
    ln -sf "./$link" "$bin_dir/python3.$minor"
    ln -sf "./$link" "$bin_dir/python3"
    ln -sf "./$link" "$bin_dir/python"
    echo "Installed as python$version"

    # Cleanup
    rmdir "$install_key/python/"
    rm "$this_dir/$filename"
done

echo "Done!"
