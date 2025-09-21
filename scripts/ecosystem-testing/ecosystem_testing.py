#!/usr/bin/env python3
# NB: LLM code ahead
# /// script
# requires-python = ">=3.14"
# dependencies = [
#     "tqdm",
# ]
# ///

import argparse
import concurrent.futures
import csv
import json
import os
import shutil
import subprocess
import time
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
    pipes.
    """

    start = time.time()

    process = subprocess.Popen(
        cmd,
        stdin=subprocess.PIPE,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
    )

    stdin = f"{package}=={version}" if version else package

    stdout, stderr = communicate(process, stdin)

    # At this point, the process is a zombie, so has called `exit()`, but we haven't reaped it with `wait4` yet.

    # rusage is only available on unix
    if os.name == "posix":
        # Wait for process and get resource usage
        _pid, exit_code, rusage = os.wait4(process.pid, 0)
    else:
        exit_code = process.wait()
        rusage = None

    max_rss = rusage.ru_maxrss if rusage else 0

    package_dir = output_dir.joinpath(package)
    package_dir.mkdir(parents=True, exist_ok=True)
    package_dir.joinpath("stdout.txt").write_text(stdout)
    package_dir.joinpath("stderr.txt").write_text(stderr)
    summary = Summary(
        package=package, exit_code=exit_code, max_rss=max_rss, time=time.time() - start
    )
    package_dir.joinpath("summary.json").write_text(json.dumps(summary.__dict__))
    return summary


def communicate(process: subprocess.Popen, stdin: str) -> tuple[str, str]:
    """Like `Popen.communicate`, but without the `os.wait` call.

    Start threads to drain the pipes to avoid blocking on full pipes, but don't use
    libc's `wait` so we can use `os.wait4` later.
    """
    process.stdin.write(stdin)
    process.stdin.close()

    # Mutable objects to communicate across threads
    stdout = []
    stderr = []

    def read_stdout():
        stdout.append(process.stdout.read())
        process.stdout.close()

    def read_stderr():
        stderr.append(process.stderr.read())
        process.stderr.close()

    stdout_thread = Thread(target=read_stdout, daemon=True)
    stderr_thread = Thread(target=read_stderr, daemon=True)
    stdout_thread.start()
    stderr_thread.start()
    stdout_thread.join()
    stderr_thread.join()

    return stdout[0], stderr[0]


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
        with cwd.joinpath("package_versions.csv").open() as f:
            latest_versions = {
                row["package_name"]: row["latest_version"] for row in csv.DictReader(f)
            }
    else:
        latest_versions = None

    excluded_packages = [
        # 5000 releases, no solution
        "nucliadb",
        # These packages have many non-small versions
        "tf-models-nightly",
        "mtmtrain",
        "llm-dialog-manager",
        # Slow and have no solution
        "edx-enterprise",
        "kcli",
        "emmet-api",
    ]
    for package in excluded_packages:
        top_15k_pypi.remove(package)

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

    print(f"Success: {success}/{total} ({success / total:.0%})")

    successes = [summary for summary in all_results if summary.exit_code == 0]

    print("\n# top 5 slowest resolutions for successes")
    slowest = sorted(successes, key=lambda x: x.time, reverse=True)[:5]
    for summary in slowest:
        print(
            f"{summary.package}: {summary.time:.2f}s (exit code: {summary.exit_code})"
        )

    if os.name == "posix":
        print("\n# top 5 max RSS for successes")
        largest_rss = sorted(successes, key=lambda x: x.max_rss, reverse=True)[:5]
        for summary in largest_rss:
            # Only linux, max RSS is in KB
            print(
                f"{summary.package}: {summary.max_rss / 1024:.1f} MB (exit code: {summary.exit_code})"
            )


if __name__ == "__main__":
    main()
