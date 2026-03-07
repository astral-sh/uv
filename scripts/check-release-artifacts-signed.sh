#!/usr/bin/env bash
## Check that release artifacts from a CI run are code-signed.
##
## Downloads macOS and Windows artifacts and wheels from the given GitHub
## Actions run, extracts binaries, and verifies:
##   - macOS:   codesign identity signature (not ad-hoc)
##   - Windows: Authenticode signature present
##
## Usage:
##   scripts/check-release-artifacts-signed.sh <run-id>

set -euo pipefail

if [ $# -lt 1 ]; then
    echo "Usage: $0 <run-id>" >&2
    exit 1
fi

missing=()
command -v gh >/dev/null 2>&1 || missing+=(gh)
command -v codesign >/dev/null 2>&1 || missing+=(codesign)
command -v osslsigncode >/dev/null 2>&1 || missing+=(osslsigncode)

if [ ${#missing[@]} -gt 0 ]; then
    echo "error: missing required tools: ${missing[*]}" >&2
    echo "" >&2
    echo "Install with:" >&2
    for tool in "${missing[@]}"; do
        case "$tool" in
            gh)            echo "  brew install gh" >&2 ;;
            codesign)      echo "  (requires macOS)" >&2 ;;
            osslsigncode)  echo "  brew install osslsigncode" >&2 ;;
        esac
    done
    exit 1
fi

RUN_ID="$1"
WORK_DIR="$(mktemp -d)"
trap 'rm -rf "$WORK_DIR"' EXIT

PASS=0
FAIL=0

pass() { echo "PASS $1"; PASS=$((PASS + 1)); }
fail() { echo "FAIL $1"; FAIL=$((FAIL + 1)); }

check_macos() {
    local binary="$1"
    local label="$2"
    local info
    info=$(codesign -dv "$binary" 2>&1) || true
    if echo "$info" | grep -q "Signature=adhoc"; then
        fail "$label (ad-hoc, not identity-signed)"
    elif sig_size=$(echo "$info" | grep "Signature size=" | sed 's/.*Signature size=//'); then
        pass "$label (identity-signed, size=$sig_size)"
    else
        fail "$label (not signed)"
    fi
}

check_windows() {
    local binary="$1"
    local label="$2"
    local output
    output=$(osslsigncode verify -in "$binary" 2>&1) || true
    if echo "$output" | grep -q "Signer's certificate:"; then
        local subject
        subject=$(echo "$output" | grep "Subject:" | head -1 | sed 's/.*Subject: //')
        pass "$label (Authenticode, $subject)"
    else
        fail "$label (not Authenticode signed)"
    fi
}

echo "Fetching artifacts for run $RUN_ID..."
ALL_ARTIFACTS=$(gh api "repos/{owner}/{repo}/actions/runs/$RUN_ID/artifacts" \
    --paginate --jq '.artifacts[].name')

echo ""

for artifact in $ALL_ARTIFACTS; do
    # Only check macOS and Windows archives and wheels.
    case "$artifact" in
        artifacts-*apple-darwin*|artifacts-macos-*)  check=check_macos ;;
        artifacts-*windows*|artifacts-*win*)         check=check_windows ;;
        wheels_uv-*apple-darwin*|wheels_uv-macos-*)  check=check_macos ;;
        wheels_uv-*windows*|wheels_uv-*win*)         check=check_windows ;;
        *) continue ;;
    esac

    dest="$WORK_DIR/$artifact"
    mkdir -p "$dest"
    if ! gh run download "$RUN_ID" -n "$artifact" -D "$dest"; then
        fail "$artifact (download failed)"
        continue
    fi

    # Extract everything: tar.gz archives, zip archives, and wheels.
    for tarball in "$dest"/*.tar.gz; do
        [ -f "$tarball" ] || continue
        tar xzf "$tarball" -C "$dest"
    done
    for zip in "$dest"/*.zip "$dest"/*.whl; do
        [ -f "$zip" ] || continue
        unzip -qo "$zip" -d "$dest"
    done

    # Check each binary. The label shows the archive/wheel filename and binary name,
    # e.g. "uv-x86_64-apple-darwin.tar.gz uv" or "uv-0.10.8-py3-none-win_amd64.whl uv.exe".
    while IFS= read -r binary; do
        bin_name=$(basename "$binary")
        # Walk up to find the archive or wheel this binary came from.
        archive=""
        for f in "$dest"/*.tar.gz "$dest"/*.zip "$dest"/*.whl; do
            [ -f "$f" ] && archive=$(basename "$f") && break
        done
        $check "$binary" "${archive:-$artifact} / $bin_name"
    done < <(find "$dest" \( -name "uv" -o -name "uvx" -o -name "uv.exe" -o -name "uvx.exe" -o -name "uvw.exe" \) -type f ! -name "*.sha256")
done

echo ""
echo "PASS $PASS / FAIL $FAIL"

if [ "$FAIL" -gt 0 ]; then
    exit 1
fi
