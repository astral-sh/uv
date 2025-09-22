#!/bin/bash

set -ex

script_dir=$( cd -- "$( dirname -- "${BASH_SOURCE[0]}" )" &> /dev/null && pwd )
limit=50000

uv run $script_dir/ecosystem_testing.py --uv $1 --mode compile --output $script_dir/base-compile --limit $limit
uv run $script_dir/ecosystem_testing.py --uv $2 --mode compile --output $script_dir/branch-compile --limit $limit
uv run $script_dir/ecosystem_testing.py --uv $1 --mode lock --output $script_dir/base-lock --limit $limit
uv run $script_dir/ecosystem_testing.py --uv $2 --mode lock --output $script_dir/branch-lock --limit $limit
uv run $script_dir/ecosystem_testing.py --uv $1 --mode pyproject-toml --input $script_dir/pyproject_toml --output $script_dir/base-pyproject-toml --limit $limit
uv run $script_dir/ecosystem_testing.py --uv $2 --mode pyproject-toml --input $script_dir/pyproject_toml --output $script_dir/branch-pyproject-toml --limit $limit

rm $script_dir/report.md
uv run $script_dir/create_report.py $script_dir/base-compile $script_dir/branch-compile --mode compile --markdown >> $script_dir/report.md
uv run $script_dir/create_report.py $script_dir/base-lock $script_dir/branch-lock --mode lock --markdown >> $script_dir/report.md
uv run $script_dir/create_report.py $script_dir/base-pyproject-toml $script_dir/branch-pyproject-toml --mode pyproject-toml --markdown >> $script_dir/report.md
