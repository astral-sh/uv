#!/usr/bin/env bash

set -euo pipefail

# A registry can throttle a manifest push after accepting its blobs.
# Retry the same saved build with backoff and jitter.
retry_delay=5
for attempt in {1..5}; do
    if depot push "$@"; then
        exit 0
    else
        push_status=$?
    fi
    if [ "$attempt" -eq 5 ]; then
        exit "$push_status"
    fi
    sleep_seconds=$((retry_delay + RANDOM % 5))
    echo "::warning::Depot push failed (attempt $attempt/5); retrying in ${sleep_seconds}s" >&2
    sleep "$sleep_seconds"
    retry_delay=$((retry_delay * 2))
done
