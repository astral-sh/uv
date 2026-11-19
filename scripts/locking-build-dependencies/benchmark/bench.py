"""Compare two uv lock binaries on virtual projects, without building distributions."""

import argparse
import csv
import hashlib
import json
import os
import re
import subprocess
import tempfile
import time
import tomllib
from concurrent.futures import ThreadPoolExecutor, as_completed
from pathlib import Path

CUTOFF = "2026-11-01T00:00:00Z"
PREVIEW = "lock-build-dependencies"
NAMED = {
    "black": "black==26.3.1",
    "httpx": "httpx[http2]==0.28.1",
    "jupyter": "jupyter==1.1.1",
    "airflow": "apache-airflow==3.1.8",
    "transformers": "transformers[torch,sentencepiece]==5.3.0",
}


def workloads(inventory, warehouse):
    if inventory:
        with inventory.open() as stream:
            for row in csv.DictReader(stream):
                name, version = row["package_name"], row["latest_version"]
                if not re.fullmatch(r"[A-Za-z0-9._-]+", name) or not re.fullmatch(
                    r"[A-Za-z0-9.!+_-]*", version
                ):
                    raise ValueError(f"Expected an index package: {name!r} {version!r}")
                yield name, [f"{name}=={version}"] if version else []
        return

    yield from ((name, [requirement]) for name, requirement in NAMED.items())
    # The project on PyPI named "warehouse" is not the dependency list for pypi.org.
    yield "warehouse", tomllib.loads(warehouse.read_text())["project"]["dependencies"]


def package_ids(lock):
    return {
        (pkg["name"], pkg.get("version"), json.dumps(pkg["source"], sort_keys=True))
        for pkg in lock["package"]
        if "virtual" not in pkg["source"]
    }


def measure(binary, python, requirements, cache, cutoff, candidate, keep, timeout):
    with tempfile.TemporaryDirectory() as tmp:
        root = Path(tmp)
        (root / "pyproject.toml").write_text(
            '[project]\nname = "benchmark-input"\nversion = "0"\n'
            'requires-python = "==3.11.*"\n'
            f"dependencies = {json.dumps(requirements)}\n"
        )
        command = [
            str(binary),
            "lock",
            "--no-build",
            "--no-python-downloads",
            "--no-config",
            "--default-index",
            "https://pypi.org/simple",
            "--python",
            str(python),
            "--exclude-newer",
            cutoff,
            "--cache-dir",
            str(cache),
            "--color",
            "never",
        ]
        if candidate:
            command.extend(["--preview-features", PREVIEW])
        env = {k: v for k, v in os.environ.items() if not k.startswith("UV_")}
        env.update(
            UV_CONCURRENT_DOWNLOADS="4", RAYON_NUM_THREADS="1", TOKIO_WORKER_THREADS="1"
        )

        def timed_lock(*flags):
            start = time.perf_counter()
            try:
                proc = subprocess.run(
                    [*command, *flags],
                    cwd=root,
                    env=env,
                    capture_output=True,
                    timeout=timeout,
                )
            except subprocess.TimeoutExpired:
                return {
                    "seconds": time.perf_counter() - start,
                    "exit": None,
                    "error": f"timed out after {timeout} seconds",
                }
            result = {"seconds": time.perf_counter() - start, "exit": proc.returncode}
            if proc.returncode:
                message = (
                    proc.stderr.decode(errors="replace")
                    .replace(tmp, "[project]")
                    .replace(str(cache), "[cache]")
                    .replace(str(python), "[python]")
                )
                result["error"] = "\n".join(
                    line
                    for line in message.splitlines()
                    if not line.startswith("Using ")
                )[-1000:]
            return result

        result = timed_lock()
        if "error" in result:
            return result, set()
        data = (root / "uv.lock").read_bytes()
        lock = tomllib.loads(data.decode())
        if bool(lock.get("lock-build-dependencies")) != candidate:
            raise RuntimeError("Binary did not write the expected lock mode")
        if keep:
            keep.write_bytes(data)
            result["offline_check"] = timed_lock("--check", "--offline")
        packages = package_ids(lock)
        return result | {
            "bytes": len(data),
            "packages": len(packages),
            "nodes": len(lock["package"]),
            "edges": sum(
                len(p.get("dependencies", []))
                + len(p.get("build-dependencies", []))
                + sum(map(len, p.get("optional-dependencies", {}).values()))
                + sum(map(len, p.get("dev-dependencies", {}).values()))
                for p in lock["package"]
            ),
        }, packages


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("baseline", type=Path)
    parser.add_argument("candidate", type=Path)
    parser.add_argument("output", type=Path)
    parser.add_argument("--python", required=True, type=Path)
    parser.add_argument("--exclude-newer", default=CUTOFF)
    parser.add_argument("--inventory", type=Path)
    parser.add_argument("--warehouse", type=Path)
    parser.add_argument("--limit", type=int)
    parser.add_argument(
        "--jobs",
        type=int,
        help="Parallel workload pairs (default: 1 named, 8 inventory)",
    )
    parser.add_argument("--timeout", type=int, default=180)
    args = parser.parse_args()
    if args.inventory and not (
        Path("/.dockerenv").exists() or os.environ.get("IN_SCAN_CONTAINER") == "1"
    ):
        parser.error("Run the PyPI inventory in a container using run.sh")
    if not args.inventory and not args.warehouse:
        parser.error("Provide the Warehouse fixture's pyproject.toml for named runs")
    if args.output.exists():
        parser.error(f"Choose a new output directory: {args.output}")
    args.output.mkdir(parents=True)
    binaries = {
        "baseline": args.baseline.resolve(),
        "candidate": args.candidate.resolve(),
    }

    def digest(path):
        with path.open("rb") as stream:
            return hashlib.file_digest(stream, "sha256").hexdigest()

    inputs = list(workloads(args.inventory, args.warehouse))[: args.limit]
    jobs = args.jobs or (8 if args.inventory else 1)
    metadata = {
        "jobs": jobs,
        "inputs": len(inputs),
        "cutoff": args.exclude_newer,
        "sha256": {k: digest(v) for k, v in binaries.items()},
        "workloads": "inventory" if args.inventory else "named",
        "no_build": True,
    }
    (args.output / "method.json").write_text(json.dumps(metadata, indent=2) + "\n")
    with (
        tempfile.TemporaryDirectory() as caches,
        (args.output / "results.jsonl").open("x", buffering=1) as output,
    ):

        def compare(index, name, requirements):
            result = {"name": name}
            if not requirements:
                return result | {
                    side: {"error": "No version in pinned input"} for side in binaries
                }
            ids = {}
            for side in list(binaries) if index % 2 == 0 else list(reversed(binaries)):
                keep = None if args.inventory else args.output / f"{name}-{side}.lock"
                result[side], ids[side] = measure(
                    binaries[side],
                    args.python.absolute(),
                    requirements,
                    Path(caches) / side,
                    args.exclude_newer,
                    side == "candidate",
                    keep,
                    args.timeout,
                )
            if not any("error" in result[side] for side in binaries):
                result["added"] = len(ids["candidate"] - ids["baseline"])
                result["removed"] = len(ids["baseline"] - ids["candidate"])
            return result

        with ThreadPoolExecutor(jobs) as pool:
            futures = [
                pool.submit(compare, i, name, reqs)
                for i, (name, reqs) in enumerate(inputs)
            ]
            for completed, future in enumerate(as_completed(futures), start=1):
                result = future.result()
                print(json.dumps(result), file=output)
                print(f"{completed}/{len(inputs)}: {result['name']}", flush=True)


if __name__ == "__main__":
    main()
