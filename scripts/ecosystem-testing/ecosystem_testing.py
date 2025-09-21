#!/usr/bin/env python3
# NB: LLM code ahead
# /// script
# requires-python = ">=3.14"
# dependencies = [
#     "tqdm",
# ]
# ///

import argparse
import concurrent
import csv
import functools
import json
import multiprocessing
import os
import shutil
import signal
import subprocess
import sys
import time
from collections import deque
from concurrent.futures import ThreadPoolExecutor
from dataclasses import dataclass
from pathlib import Path
from threading import Thread

from tqdm.auto import tqdm

cwd = Path(__file__).parent


@dataclass
class Summary:
    package: str
    exit_code: int
    max_rss: int
    time: float


def run_uv(
    cmd: list[str], package: str, output_dir: Path, version: str | None
) -> Summary:
    """Run a uv subprocess.

    The logic captures the max RSS from the process and avoids deadlocks from full
    pipes."""

    start = time.time()

    process = subprocess.Popen(
        cmd,
        stdin=subprocess.PIPE,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
    )

    process.stdin.write(f"{package}=={version}" if version else package)
    process.stdin.close()

    # Use thread-safe deques to collect output from threads
    stdout_lines = deque()
    stderr_lines = deque()

    def read_stdout():
        for line in iter(process.stdout.readline, ""):
            stdout_lines.append(line)
        process.stdout.close()

    def read_stderr():
        for line in iter(process.stderr.readline, ""):
            stderr_lines.append(line)
        process.stderr.close()

    # Start threads to drain the pipes to avoid deadlocks on full pipes
    stdout_thread = Thread(target=read_stdout)
    stderr_thread = Thread(target=read_stderr)
    stdout_thread.daemon = True
    stderr_thread.daemon = True
    stdout_thread.start()
    stderr_thread.start()

    # Wait for process and get resource usage
    _pid, exit_code, rusage = os.wait4(process.pid, 0)

    stdout_thread.join()
    stderr_thread.join()

    stdout = "".join(stdout_lines)
    stderr = "".join(stderr_lines)

    max_rss = rusage.ru_maxrss

    package_dir = output_dir.joinpath(package)
    package_dir.mkdir(parents=True, exist_ok=True)
    package_dir.joinpath("stdout.txt").write_text(stdout)
    package_dir.joinpath("stderr.txt").write_text(stderr)
    summary = Summary(
        package=package, exit_code=exit_code, max_rss=max_rss, time=time.time() - start
    )
    package_dir.joinpath("summary.json").write_text(json.dumps(summary.__dict__))
    return summary


def signal_handler(executor: ThreadPoolExecutor, signum, frame):
    """Handle Ctrl+C gracefully."""
    print(f"Stopping for SIGINT (signal {signum})")
    executor.shutdown(wait=False, cancel_futures=True)
    print("Stopped.")
    sys.exit(1)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--python", "-p", type=str, default="3.13")
    parser.add_argument("--output-dir", type=Path, default="output")
    parser.add_argument("--uv", type=Path, default=Path("uv"))
    parser.add_argument("--limit", type=int, default=None)
    parser.add_argument("--cache", type=Path, default=cwd.joinpath("cache"))
    parser.add_argument("--offline", action="store_true")
    parser.add_argument("--latest", action="store_true")
    args = parser.parse_args()

    top_15k_pypi = json.loads(cwd.joinpath("top-pypi-packages.json").read_text())
    top_15k_pypi = [pkg["project"] for pkg in top_15k_pypi["rows"]]

    if args.latest:
        latest_versions = cwd.joinpath("package_versions.csv").read_text()
        latest_versions = {
            row["package_name"]: row["latest_version"]
            for row in csv.DictReader(latest_versions.splitlines())
        }
    else:
        latest_versions = None

    # 5000 releases, no solution
    top_15k_pypi.remove("nucliadb")
    # Remove slow packages
    for slow in [
        # These packages have many non-small versions
        "tf-models-nightly",
        "mtmtrain",
        "llm-dialog-manager",
        "edx-enterprise",  # Doesn't solve
        "kcli",
        "emmet-api",
    ]:
        top_15k_pypi.remove(slow)

    output_dir = cwd.joinpath(args.output_dir)
    if output_dir.exists():
        shutil.rmtree(output_dir)
    output_dir.mkdir(parents=True, exist_ok=True)
    output_dir.joinpath(".gitignore").write_text("*")

    cmd = [
        args.uv,
        "pip",
        "compile",
        "-",
        "-p",
        args.python,
        "--universal",
        "--no-build",
        "--cache-dir",
        args.cache,
        "--color",
        "never",
        "--no-header",
        "--no-annotate",
    ]
    if args.offline:
        cmd.append("--offline")
    success = 0
    all_results = []  # Track all results for analysis
    max_package_len = max(len(package) for package in top_15k_pypi[: args.limit])

    with ThreadPoolExecutor(max_workers=os.cpu_count() * 2) as executor:
        # Shutdown executor on ctrl+c
        previous_sigint_handler = signal.signal(
            signal.SIGINT, functools.partial(signal_handler, executor)
        )

        tasks = []
        packages_pending = []
        for package in top_15k_pypi[: args.limit]:
            if latest_versions:
                if version := latest_versions.get(package):
                    pass
                else:
                    tqdm.write(f"Missing version: {package}")
                    continue
            else:
                version = None
            packages_pending.append(package)
            tasks.append(executor.submit(run_uv, cmd, package, output_dir, version))

        total = len(packages_pending)
        with tqdm(total=total) as progress_bar:
            for result in concurrent.futures.as_completed(tasks):
                summary = result.result()

                all_results.append(summary)
                progress_bar.update(1)
                packages_pending.remove(summary.package)
                if packages_pending:
                    progress_bar.set_postfix_str(
                        f"{packages_pending[0]:>{max_package_len}}"
                    )
                if summary.exit_code == 0:
                    success += 1
    signal.signal(signal.SIGINT, previous_sigint_handler)

    print(f"Success: {success}/{total}")

    successes = [summary for summary in all_results if summary.exit_code == 0]
    print("\n# top 5 max RSS for successes")
    largest_rss = sorted(successes, key=lambda x: x.max_rss, reverse=True)[:5]
    for summary in largest_rss:
        print(
            f"{summary.package}: {summary.max_rss / 1024:.1f} MB (exit code: {summary.exit_code})"
        )

    print("\n# top 5 slowest resolutions for successes")
    slowest = sorted(successes, key=lambda x: x.time, reverse=True)[:5]
    for summary in slowest:
        print(
            f"{summary.package}: {summary.time:.2f}s (exit code: {summary.exit_code})"
        )


if __name__ == "__main__":
    main()
