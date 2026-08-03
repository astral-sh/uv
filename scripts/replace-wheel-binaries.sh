#!/usr/bin/env bash
# Replace executable wheel members with signed binaries and update their RECORD entries.
#
# Usage: scripts/replace-wheel-binaries.sh <signed-directory> <output-directory> <wheel>...

set -euo pipefail
shopt -s nullglob

if [[ $# -lt 3 ]]; then
  echo "Usage: $0 <signed-directory> <output-directory> <wheel>..." >&2
  exit 1
fi

signed_directory="$1"
output_directory="$2"
shift 2

mkdir -p "$output_directory"
output_directory="$(cd "$output_directory" && pwd -P)"
work_directory="$(mktemp -d "$RUNNER_TEMP/uv-wheel-replace.XXXXXXXX")"
trap 'rm -rf "$work_directory"' EXIT

for wheel in "$@"; do
  wheel_name="$(basename "$wheel")"
  unpacked="$work_directory/${wheel_name%.whl}"
  unzip -q "$wheel" -d "$unpacked"

  records=("$unpacked"/*.dist-info/RECORD)
  binaries=("$unpacked"/*.data/scripts/*)
  if [[ ${#records[@]} -ne 1 || ${#binaries[@]} -eq 0 ]]; then
    echo "Expected one RECORD and at least one executable in '$wheel'." >&2
    exit 1
  fi

  record="${records[0]}"
  for binary_path in "${binaries[@]}"; do
    binary="$(basename "$binary_path")"
    case "$binary" in
      uv | uvx | uv-build) ;;
      *)
        echo "Unexpected executable '$binary' in '$wheel'." >&2
        exit 1
        ;;
    esac

    cp "$signed_directory/$binary" "$binary_path"
    chmod 0755 "$binary_path"

    digest="$(openssl dgst -sha256 -binary "$binary_path" | openssl base64 -A | tr '+/' '-_' | tr -d '=')"
    size="$(wc -c < "$binary_path" | tr -d '[:space:]')"
    member="${binary_path#"$unpacked"/}"
    pattern="^[^/]+\\.data/scripts/${binary},"
    matches="$(grep -Ec "$pattern" "$record" || true)"
    if [[ "$matches" -ne 1 ]]; then
      echo "Expected one RECORD entry for '$member' in '$wheel'." >&2
      exit 1
    fi

    sed -E "s|${pattern}.*$|${member},sha256=${digest},${size}|" "$record" > "$record.updated"
    mv "$record.updated" "$record"
  done

  output="$output_directory/$wheel_name"
  if [[ -e "$output" ]]; then
    echo "Output wheel '$output' already exists." >&2
    exit 1
  fi
  (cd "$unpacked" && find . -type f -print | sed 's|^\./||' | LC_ALL=C sort | zip -q -X "$output" -@)
  unzip -tq "$output" >/dev/null
done
