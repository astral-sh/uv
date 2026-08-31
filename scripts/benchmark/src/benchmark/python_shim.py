"""Measure Python shim startup against the interpreter it actually selects.

This module only needs the standard library and hyperfine. See the benchmark
README for build and invocation instructions. Currently requires a POSIX host.
"""

import argparse
import hashlib
import json
import os
import platform
import shlex
import shutil
import statistics
import subprocess
import sys
import tempfile
from datetime import UTC, datetime
from pathlib import Path

PROBE = """import json, sys
print(json.dumps({
    "executable": sys.executable,
    "prefix": sys.prefix,
    "base_prefix": sys.base_prefix,
    "version": list(sys.version_info[:3]),
}))
"""


def output(command: list[str], *, cwd: Path, env: dict[str, str]) -> str:
    return subprocess.check_output(command, cwd=cwd, env=env, text=True).strip()


def digest(path: Path) -> str:
    with path.open("rb") as file:
        return hashlib.file_digest(file, "sha256").hexdigest()


def summarize(results: list[dict], output_dir: Path) -> None:
    groups: dict[tuple[str, str, str], list[dict]] = {}
    for result in results:
        direct = next(
            item for item in result["results"] if item["command"] == "direct-python"
        )
        for item in result["results"]:
            key = (result["scenario"], result["cache"], item["command"])
            groups.setdefault(key, []).append(
                {
                    "median_ms": item["median"] * 1000,
                    "overhead_ms": (item["median"] - direct["median"]) * 1000,
                    "ratio": item["median"] / direct["median"],
                }
            )
    summary = []
    for (scenario, cache, command), measurements in sorted(groups.items()):
        medians = [item["median_ms"] for item in measurements]
        summary.append(
            {
                "scenario": scenario,
                "cache": cache,
                "command": command,
                "median_ms": statistics.median(medians),
                "min_repetition_median_ms": min(medians),
                "max_repetition_median_ms": max(medians),
                "overhead_ms": statistics.median(
                    item["overhead_ms"] for item in measurements
                ),
                "ratio": statistics.median(item["ratio"] for item in measurements),
            }
        )
    (output_dir / "summary.json").write_text(json.dumps(summary, indent=2) + "\n")


def benchmark(args: argparse.Namespace, output_dir: Path, root: Path) -> None:
    uv = args.uv_path.resolve()
    shim = uv.with_name("uv-python")
    if not uv.is_file() or not shim.is_file():
        raise ValueError("--uv-path must have a uv-python binary alongside it")

    # Do not inherit user configuration or interpreter-selection overrides.
    env = {
        key: value
        for key, value in os.environ.items()
        if not key.startswith(("UV_", "PYTHON", "CONDA_"))
        and key
        not in {"VIRTUAL_ENV", "RUST_LOG", "RUST_BACKTRACE", "TRACING_DURATIONS_FILE"}
    }
    cache = root / "cache"
    bin_dir = root / "bin"
    env.update(
        {
            "UV_CACHE_DIR": str(cache),
            "UV_PYTHON_INSTALL_DIR": str(root / "managed"),
            "UV_PYTHON_BIN_DIR": str(bin_dir),
            "UV_PYTHON_PREFERENCE": "only-managed",
            "UV_PYTHON_NO_REGISTRY": "1",
            "UV_PYTHON_INSTALL_REGISTRY": "0",
            "UV_NO_CONFIG": "1",
            "UV_NO_PROGRESS": "1",
            "PATH": os.pathsep.join([str(uv.parent), str(bin_dir), env["PATH"]]),
        }
    )
    subprocess.run(
        [
            str(uv),
            "python",
            "install",
            "--preview-features",
            "python-shim",
            args.python,
        ],
        cwd=root,
        env=env,
        check=True,
    )
    # All downloads and environment creation happen outside the measurements.
    env.update({"UV_OFFLINE": "1", "UV_PYTHON_DOWNLOADS": "never"})
    python = output([str(uv), "python", "find", "cpython"], cwd=root, env=env)
    python_info = json.loads(output([python, "-c", PROBE], cwd=root, env=env))
    major, minor, _patch = python_info["version"]
    names = ["python", f"python{major}", f"python{major}.{minor}"]
    for name in names:
        if not (bin_dir / name).is_file():
            raise ValueError(f"Expected an installed shim at {bin_dir / name}")

    plain = root / "plain"
    project = root / "project"
    plain.mkdir()
    project.mkdir()
    venv = project / ".venv"
    subprocess.run(
        [str(uv), "venv", "--python", python, str(venv)],
        cwd=root,
        env=env,
        check=True,
    )
    scenarios = [
        ("no-venv", plain, python, env),
        ("discovered-venv", project, str(venv / "bin/python"), env),
        (
            "active-venv",
            plain,
            str(venv / "bin/python"),
            env | {"VIRTUAL_ENV": str(venv)},
        ),
    ]
    metadata = {
        "started_at": datetime.now(UTC).isoformat(),
        "platform": platform.platform(),
        "machine": platform.machine(),
        "processor": platform.processor(),
        "cpu_count": os.cpu_count(),
        "load_average_start": os.getloadavg(),
        "uv_path": str(uv),
        "uv_version": output([str(uv), "--version"], cwd=root, env=env),
        "uv_sha256": digest(uv),
        "shim_sha256": digest(shim),
        "hyperfine_version": output(["hyperfine", "--version"], cwd=root, env=env),
        "python": python_info,
        "runs": args.runs,
        "warmup": args.warmup,
        "repetitions": args.repetitions,
        "cold_cache": "Delete the isolated uv cache before each sample; OS caches are not evicted.",
        "timed_command": "python -c pass; uv-python-find is a lookup-only diagnostic",
        "shell": "none",
        "selection_checks": [],
    }

    # Fail before timing if a shim selects a different interpreter or environment.
    for scenario, cwd, direct, scenario_env in scenarios:
        expected = json.loads(output([direct, "-c", PROBE], cwd=cwd, env=scenario_env))
        for name in names:
            selected = json.loads(
                output([str(bin_dir / name), "-c", PROBE], cwd=cwd, env=scenario_env)
            )
            if any(
                selected[key] != expected[key]
                for key in ("prefix", "base_prefix", "version")
            ) or not os.path.samefile(selected["executable"], expected["executable"]):
                raise ValueError(
                    f"{scenario}/{name} selected {selected}, not {expected}"
                )
            metadata["selection_checks"].append(
                {"scenario": scenario, "name": name, "selected": selected}
            )
        found = output(
            [str(uv), "python", "find", "cpython"], cwd=cwd, env=scenario_env
        )
        if not os.path.samefile(found, direct):
            raise ValueError(
                f"{scenario}: uv python find selected {found}, not {direct}"
            )
        # Preserve cold/warm discovery traces to make the cache distinction auditable.
        shutil.rmtree(cache)
        for state in ("cold", "warm"):
            trace = subprocess.run(
                [str(uv), "python", "find", "cpython", "--verbose"],
                cwd=cwd,
                env=scenario_env | {"RUST_LOG": "uv_python=trace"},
                capture_output=True,
                text=True,
                check=True,
            )
            (output_dir / f"{scenario}-{state}-trace.txt").write_text(trace.stderr)
            expected_trace = (
                "Querying interpreter executable"
                if state == "cold"
                else "Found cached interpreter info"
            )
            if expected_trace not in trace.stderr:
                raise ValueError(f"Could not verify {scenario}/{state} cache behavior")

    (output_dir / "metadata.json").write_text(json.dumps(metadata, indent=2) + "\n")
    clear_cache = shlex.join(
        [
            sys.executable,
            "-c",
            "import shutil, sys; shutil.rmtree(sys.argv[1], ignore_errors=True)",
            str(cache),
        ]
    )
    results = []
    for repetition in range(args.repetitions):
        # Alternate scenario and command order to expose drift between repetitions.
        ordered_scenarios = (
            scenarios if repetition % 2 == 0 else list(reversed(scenarios))
        )
        for scenario, cwd, direct, scenario_env in ordered_scenarios:
            commands = [("direct-python", [direct, "-c", "pass"])]
            commands.extend(
                (name, [str(bin_dir / name), "-c", "pass"]) for name in names
            )
            commands.append(("uv-python-find", [str(uv), "python", "find", "cpython"]))
            if repetition % 2:
                commands.reverse()
            for state in ("warm", "cold"):
                result_path = output_dir / f"{scenario}-{state}-{repetition + 1}.json"
                command = [
                    "hyperfine",
                    "--shell=none",
                    "--style=basic",
                    "--warmup",
                    str(args.warmup),
                    "--runs",
                    str(args.runs),
                    "--export-json",
                    str(result_path),
                ]
                if state == "cold":
                    command.extend(["--prepare", clear_cache])
                for name, measured in commands:
                    command.extend(["--command-name", name, shlex.join(measured)])
                print(
                    f"\n{scenario}, {state} cache, repetition {repetition + 1}",
                    flush=True,
                )
                subprocess.run(command, cwd=cwd, env=scenario_env, check=True)
                result = json.loads(result_path.read_text())
                result.update({"scenario": scenario, "cache": state})
                results.append(result)
                summarize(results, output_dir)
    metadata["finished_at"] = datetime.now(UTC).isoformat()
    metadata["load_average_end"] = os.getloadavg()
    (output_dir / "metadata.json").write_text(json.dumps(metadata, indent=2) + "\n")


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--uv-path", type=Path, required=True)
    parser.add_argument("--python", default="3.12", help="CPython version to install")
    parser.add_argument(
        "--output-dir", type=Path, required=True, help="New results directory"
    )
    parser.add_argument("--runs", type=int, default=100)
    parser.add_argument("--warmup", type=int, default=10)
    parser.add_argument("--repetitions", type=int, default=3)
    args = parser.parse_args()
    if os.name != "posix":
        parser.error("This benchmark currently requires a POSIX host")
    if args.runs < 2 or args.warmup < 1 or args.repetitions < 1:
        parser.error("Require at least two runs, one warmup, and one repetition")
    if shutil.which("hyperfine") is None:
        parser.error("hyperfine must be installed")
    output_dir = args.output_dir.resolve()
    output_dir.mkdir(parents=True, exist_ok=False)
    with tempfile.TemporaryDirectory(prefix="fixture-", dir=output_dir) as directory:
        benchmark(args, output_dir, Path(directory))


if __name__ == "__main__":
    main()
