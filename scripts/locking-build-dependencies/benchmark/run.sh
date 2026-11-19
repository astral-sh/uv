#!/bin/sh
# Use Linux binaries built for the container's architecture.
set -eu
if [ "$#" -lt 3 ] || [ "$#" -gt 4 ]; then
    echo "Usage: $0 BASE_UV CANDIDATE_UV NEW_RESULTS_DIR [TOP_15K_CSV]" >&2
    exit 2
fi
scripts=$(CDPATH='' cd "$(dirname "$0")" && pwd)
root=$(CDPATH='' cd "$scripts/../../.." && pwd)
baseline=$(realpath "$1")
candidate=$(realpath "$2")
mkdir -m 700 "$3"
output=$(realpath "$3")
# Permit remapped container users to write inside the otherwise private results directory.
mkdir -m 1777 "$output/.runner"
if [ "$#" = 4 ]; then
    set -- -v "$(realpath "$4"):/input.csv:ro" --env INVENTORY=/input.csv
else
    set --
fi
docker run --rm --read-only --cap-drop ALL --security-opt no-new-privileges \
    -v "$baseline:/baseline:ro" -v "$candidate:/candidate:ro" \
    -v "$root/test/ecosystem/warehouse/pyproject.toml:/warehouse.toml:ro" \
    -v "$scripts:/bench:ro" -v "$output/.runner:/results" -v /scratch \
    --env HOME=/scratch --env TMPDIR=/scratch --env IN_SCAN_CONTAINER=1 \
    "$@" python:3.11-slim-trixie sh -eu -c '
    python /bench/bench.py /baseline /candidate /results/data \
        --python /usr/local/bin/python3.11 --warehouse /warehouse.toml \
        ${INVENTORY:+--inventory "$INVENTORY"}
    python /bench/report.py /results/data
'
mv "$output/.runner/data" "$output/data"
rmdir "$output/.runner"
echo "Open $output/data/index.html"
