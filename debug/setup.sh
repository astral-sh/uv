#!/usr/bin/env bash
set -euo pipefail

if [[ $# -ne 1 ]]; then
  echo "usage: $0 <exfat-volume>" >&2
  exit 1
fi

script_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
run_id="$(date +%s)-$$"
source_dir="$script_dir/generated/glob-appledouble-source-$run_id"
destination_dir="$1/glob-appledouble-destination-$run_id"
source="$source_dir/plain.txt"
destination="$destination_dir/plain.txt"

mkdir -p "$source_dir"
mkdir "$destination_dir"
printf 'hello\n' > "$source"
xattr -w com.apple.quarantine '0081;deadbeef;glob-appledouble-repro;0' "$source"
cp "$source" "$destination"

echo "Created APFS source file with xattr: $source" >&2
echo "Copied source file into destination: $destination" >&2
printf '%s\n' "$destination_dir"
