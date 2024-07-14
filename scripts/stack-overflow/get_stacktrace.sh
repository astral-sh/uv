#!/bin/bash

# Run e.g. as
# ```
# scripts/stack-overflow/get_stacktrace.sh pip compile a/pyproject.toml
# ```

set -e

cd "$(git rev-parse --show-toplevel)"

export UV_STACK_SIZE=1000000 # 1MB, the windows default

cargo build --bin uv

lldb -o "command script import scripts/stack-overflow/write_stacktrace.py" \
     -o "run $*" \
     -o "save_backtrace" \
     -o "quit" \
     -- "target/debug/uv" "$@"
