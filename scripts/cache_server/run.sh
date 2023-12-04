#!/bin/bash

set -euo pipefail

# shellcheck disable=SC2164
cd "$(git rev-parse --show-toplevel)"

script_dir=scripts/cache_server
export VIRTUAL_ENV=$script_dir/.venv
rm -rf $script_dir/cache-dir
virtualenv --clear -p 3.12 -q $VIRTUAL_ENV

write_cache() {
  cat <<EOF > $script_dir/requirements.txt
pkg_source_dist @ http://0.0.0.0:8000/pkg_source_dist-0.1.0.tar.gz
pkg_whl @ http://0.0.0.0:8000/pkg_whl-0.1.0-py3-none-any.whl
pkg_path_source_dist @ file://$(pwd)/$script_dir/$cache_kind/pkg_path_source_dist-0.1.0.tar.gz
pkg_path_whl @ file://$(pwd)/$script_dir/$cache_kind/pkg_path_whl-0.1.0-py3-none-any.whl
EOF
}

check() {
  $VIRTUAL_ENV/bin/python -c "import pkg_source_dist; pkg_source_dist.whoami()"
  $VIRTUAL_ENV/bin/python -c "import pkg_whl; pkg_whl.whoami()"
  $VIRTUAL_ENV/bin/python -c "import pkg_path_source_dist; pkg_path_source_dist.whoami()"
  $VIRTUAL_ENV/bin/python -c "import pkg_path_whl; pkg_path_whl.whoami()"
}

printf "\n# Cache a\n"
cache_kind=cache_a
write_cache
python -m http.server --directory $script_dir/$cache_kind/ &
cargo run --bin puffin -q -- pip-sync --cache-dir $script_dir/cache-dir $script_dir/requirements.txt
# https://unix.stackexchange.com/a/340084
kill %1
check

printf "\n# Cache b\n"
cache_kind=cache_b
write_cache
python -m http.server --directory $script_dir/$cache_kind/ &
cargo run --bin puffin -q -- pip-sync --cache-dir $script_dir/cache-dir $script_dir/requirements.txt
kill %1
check

echo "Done"
