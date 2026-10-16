#!/usr/bin/env bash
set -euo pipefail

sample="$1"
results="$RUNNER_TEMP/codegen-results"
if [[ "$sample" != seed ]]; then
  python3 - "$RUNNER_TEMP/workspace.json" > "$results/workspace-packages.txt" <<'PYTHON'
import json
import sys
with open(sys.argv[1]) as file:
    metadata = json.load(file)
workspace_members = set(metadata["workspace_members"])
for package in metadata["packages"]:
    if package["id"] in workspace_members:
        print(package["name"])
PYTHON
  mapfile -t packages < "$results/workspace-packages.txt"
  if [[ ${#packages[@]} -eq 0 ]]; then
    echo "No workspace packages found" >&2
    exit 1
  fi
  clean_args=()
  for package in "${packages[@]}"; do
    clean_args+=(-p "$package")
  done
  cargo clean --locked --profile profiling "${clean_args[@]}" 2>&1 | tee "$results/$sample-clean.log"
fi

build_args=(--locked -m "$BENCHMARK_MODE" --profile profiling -p uv-bench)
if [[ "$BENCHMARK_MODE" == walltime ]]; then
  build_args+=(--bench uv --bench workspace_discovery --bench git_fetch --bench uv_pep508)
fi
printf 'sample=%s mode=%s codegen_units=%s\n' "$sample" "$BENCHMARK_MODE" "$CARGO_PROFILE_PROFILING_CODEGEN_UNITS"
/usr/bin/time -v -o "$results/$sample-time.txt" \
  uv run --only-dev cargo codspeed build "${build_args[@]}" \
  2>&1 | tee "$results/$sample-build.log"
find target/codspeed -type f -printf '%p %s\n' | sort > "$results/$sample-artifact-sizes.txt"
