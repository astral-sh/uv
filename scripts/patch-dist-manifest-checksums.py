#!/usr/bin/env python3
"""Patch cargo-dist local manifest JSON with sidecar SHA-256 checksums.

This is used by the release workflow when custom local artifact jobs build archives
outside of cargo-dist. cargo-dist's global installer generation can embed archive
checksums if they appear in dist-manifest.json, so we synthesize a local manifest
and then inject the checksums from the uploaded `*.sha256` sidecar files.
"""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--manifest", type=Path, required=True)
    parser.add_argument("--artifacts-dir", type=Path, required=True)
    return parser.parse_args()


def read_sha256(path: Path) -> str:
    line = path.read_text(encoding="utf-8").strip()
    if not line:
        raise ValueError(f"empty checksum file: {path}")
    checksum = line.split()[0]
    if len(checksum) != 64:
        raise ValueError(f"unexpected sha256 length in {path}: {checksum!r}")
    return checksum


def main() -> int:
    args = parse_args()

    manifest = json.loads(args.manifest.read_text(encoding="utf-8"))
    artifacts: dict[str, dict] = manifest["artifacts"]

    patched = 0
    skipped = 0
    for checksum_path in sorted(args.artifacts_dir.glob("*.sha256")):
        artifact_name = checksum_path.name[: -len(".sha256")]
        artifact = artifacts.get(artifact_name)
        if artifact is None:
            print(
                f"warning: checksum file {checksum_path.name} does not match any artifact in {args.manifest.name}",
                file=sys.stderr,
            )
            skipped += 1
            continue

        checksum = read_sha256(checksum_path)
        artifact.setdefault("checksums", {})["sha256"] = checksum
        patched += 1

    if patched == 0:
        print(
            f"error: no artifact checksums were patched from {args.artifacts_dir}",
            file=sys.stderr,
        )
        return 1

    args.manifest.write_text(json.dumps(manifest, indent=2) + "\n", encoding="utf-8")
    print(
        f"patched {patched} artifact checksum(s) in {args.manifest}"
        + (f" ({skipped} checksum file(s) skipped)" if skipped else ""),
        file=sys.stderr,
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
