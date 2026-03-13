#!/usr/bin/env bash
## Verify that all release artifacts contain cargo-auditable SBOM data.
##
## Requires:
##   cargo install rust-audit-info --locked
##
## Usage:
##   scripts/check-release-artifact-sboms.sh <run-id>

set -euo pipefail

if [ $# -ne 1 ]; then
    echo "Usage: $0 <github-actions-run-id>" >&2
    exit 1
fi

missing=""
command -v gh >/dev/null 2>&1 || missing="$missing gh"
command -v rust-audit-info >/dev/null 2>&1 || missing="$missing rust-audit-info"

if [ -n "$missing" ]; then
    echo "error: missing required tools:$missing" >&2
    exit 1
fi

RUN_ID="$1"
WORKDIR="$(mktemp -d)"
trap 'rm -rf "$WORKDIR"' EXIT

PASS=0
FAIL=0

pass() { echo "PASS $1"; PASS=$((PASS + 1)); }
fail() { echo "FAIL $1"; FAIL=$((FAIL + 1)); }

check() {
    local binary="$1"
    local label="$2"
    if rust-audit-info "$binary" >/dev/null 2>&1; then
        pass "$label"
    else
        fail "$label"
    fi
}

echo "Fetching artifacts for run $RUN_ID..."
ALL_ARTIFACTS=$(gh api "repos/{owner}/{repo}/actions/runs/$RUN_ID/artifacts" \
    --paginate --jq '.artifacts[].name')

echo ""

for artifact in $ALL_ARTIFACTS; do
    case "$artifact" in
        artifacts-*) ;;
        *) continue ;;
    esac

    dest="$WORKDIR/$artifact"
    gh run download "$RUN_ID" -n "$artifact" -D "$dest"

    # Extract the archive.
    for tarball in "$dest"/*.tar.gz; do
        [ -f "$tarball" ] || continue
        tar xzf "$tarball" -C "$dest"
    done
    for zip in "$dest"/*.zip; do
        [ -f "$zip" ] || continue
        unzip -qo "$zip" -d "$dest"
    done

    # Find the archive name for labeling.
    archive=""
    for f in "$dest"/*.tar.gz "$dest"/*.zip; do
        [ -f "$f" ] && archive=$(basename "$f") && break
    done

    # Check uv and uvx binaries.
    for bin in uv uvx; do
        binary=$(find "$dest" \( -name "$bin" -o -name "$bin.exe" \) -type f | head -1)
        if [ -n "$binary" ]; then
            check "$binary" "${archive:-$artifact} / $(basename "$binary")"
        fi
    done
done

echo ""
echo "PASS $PASS / FAIL $FAIL"

if [ "$FAIL" -gt 0 ]; then
    exit 1
fi
