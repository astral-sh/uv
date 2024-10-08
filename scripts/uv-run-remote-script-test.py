# This file is used to test `uv run <url>` in ../crates/uv/tests/run.rs
# /// script
# requires-python = ">=3.11"
# dependencies = [
#   "rich==13.7.1",
# ]
# ///
import sys

import rich

who = sys.argv[1]
rich.print(f"Hello {who}, from uv!")
