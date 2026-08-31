"""Time the unchanged release PGO pipeline for a temporary runner comparison."""

# /// script
# requires-python = ">=3.11"
# dependencies = []
# ///

from __future__ import annotations

import argparse
import importlib.util
import json
import sys
import time
from datetime import UTC, datetime
from pathlib import Path


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    output = args.output.resolve()
    output.parent.mkdir(parents=True, exist_ok=True)
    root = Path.cwd().resolve()
    spec = importlib.util.spec_from_file_location(
        "build_uv_pgo", root / "scripts" / "build_uv_pgo.py"
    )
    if spec is None or spec.loader is None:
        raise RuntimeError("Could not load the release PGO script")
    pgo = importlib.util.module_from_spec(spec)
    sys.modules[spec.name] = pgo
    spec.loader.exec_module(pgo)
    original_run = pgo.run

    def timed_run(command: list[str], **kwargs: object) -> None:
        started_at = datetime.now(UTC).isoformat()
        start = time.perf_counter()
        success = False
        try:
            original_run(command, **kwargs)
            success = True
        finally:
            record = {
                "started_at": started_at,
                "elapsed_seconds": time.perf_counter() - start,
                "success": success,
                "command": command,
            }
            with output.open("a", encoding="utf-8") as stream:
                stream.write(json.dumps(record) + "\n")
            print("PGO_METRIC " + json.dumps(record), flush=True)

    pgo.run = timed_run
    sys.argv = [
        str(root / "scripts" / "build_uv_pgo.py"),
        "--target",
        "x86_64-pc-windows-msvc",
        "--target-dir",
        str(root / "target" / "uv-pgo"),
        "--train-only",
    ]
    pgo.main()


if __name__ == "__main__":
    main()
