#!/usr/bin/env sh
## Verify that release executables were linked with mold.

set -eu

for artifact in "$@"; do
    case "$artifact" in
        *.whl)
            if ! unzip -p "$artifact" '*.data/scripts/uv-build' | grep -aFq 'mold '; then
                echo "Expected $artifact to contain an executable linked with mold" >&2
                exit 1
            fi
            ;;
        *)
            if ! grep -aFq 'mold ' "$artifact"; then
                echo "Expected $artifact to be linked with mold" >&2
                exit 1
            fi
            ;;
    esac
done
