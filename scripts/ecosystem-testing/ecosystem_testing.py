#!/usr/bin/env -S uv run --script
# NB: LLM code ahead
# /// script
# requires-python = ">=3.13"
# dependencies = [
#     "tomli-w>=1.2.0,<2.0.0",
#     "tqdm>=4.67.1,<5.0.0",
# ]
# ///

import argparse
import concurrent.futures
import csv
import json
import os
import platform
import shutil
import subprocess
import time
import tomllib
from concurrent.futures import ThreadPoolExecutor
from dataclasses import dataclass
from pathlib import Path
from threading import Thread

import tomli_w
from tqdm.auto import tqdm

cwd = Path(__file__).parent


@dataclass
class Summary:
    package: str
    exit_code: int
    max_rss: int
    time: float


def run_uv(
    package: str,
    specification: str,
    uv: Path,
    mode: str,
    python: str,
    cache: Path,
    offline: bool,
    output: Path,
) -> Summary:
    """Resolve in a uv subprocess.

    The logic captures the max RSS from the process and avoids deadlocks from full
    pipes.
    """
    package_dir = output.joinpath(package)
    package_dir.mkdir()
    command = prepare_uv_command(
        specification,
        uv,
        mode,
        cache,
        offline,
        package_dir,
        python,
    )

    start = time.time()

    process = subprocess.Popen(
        command,
        cwd=package_dir,
        stdin=subprocess.PIPE,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
    )

    stdout, stderr = communicate(process, specification if mode == "compile" else None)

    # At this point, the process is a zombie, so has called `exit()`, but we haven't reaped it with `wait4` yet.

    # rusage is only available on unix
    if os.name == "posix":
        # Wait for process and get resource usage
        _pid, exit_code, rusage = os.wait4(process.pid, 0)
    else:
        exit_code = process.wait()
        rusage = None

    max_rss = rusage.ru_maxrss if rusage else 0

    package_dir.joinpath("stdout.txt").write_text(stdout)
    package_dir.joinpath("stderr.txt").write_text(stderr)
    summary = Summary(
        package=package, exit_code=exit_code, max_rss=max_rss, time=time.time() - start
    )
    package_dir.joinpath("summary.json").write_text(json.dumps(summary.__dict__))
    return summary


def prepare_uv_command(
    specification: str,
    uv: Path,
    mode: str,
    cache: Path,
    offline: bool,
    package_dir: Path,
    python: str,
) -> list[Path | str]:
    shared_args = [
        "--no-build",
        "--cache-dir",
        cache,
        "--color",
        "never",
    ]
    if offline:
        shared_args.append("--offline")
    if mode == "pyproject-toml":
        package_dir.joinpath("pyproject.toml").write_text(specification)
        command = [uv, "lock", *shared_args]
    elif mode == "lock":
        package_dir.joinpath("pyproject.toml").write_text(
            f"""
            [project]
            name = "testing"
            version = "0.1.0"
            requires-python = ">={python}"
            dependencies = ["{specification}"]
            """
        )
        command = [uv, "lock", *shared_args]
    elif mode == "compile":
        command = [
            uv,
            "pip",
            "compile",
            "-",
            "-p",
            python,
            # The results are more reproducible if they are platform independent
            "--universal",
            "--no-header",
            "--no-annotate",
            *shared_args,
        ]
    else:
        raise ValueError(f"Unknown mode: {mode}")
    return command


def communicate(process: subprocess.Popen, stdin: str | None) -> tuple[str, str]:
    """Like `Popen.communicate`, but without the `os.wait` call.

    Start threads to drain the pipes to avoid blocking on full pipes, but don't use
    libc's `wait` so we can use `os.wait4` later.
    """
    if stdin:
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
    parser.add_argument(
        "--input", type=Path, default=cwd.joinpath("top-pypi-packages.json")
    )
    parser.add_argument(
        "--mode",
        choices=["compile", "lock", "pyproject-toml"],
        default="compile",
        help="`compile`: `uv pip compile`, "
             "`lock`: `uv lock` from a single requirement"
             "`pyproject-toml`: `uv lock` from a directory of `pyproject.toml` files",
    )
    parser.add_argument("--python", "-p", type=str, default="3.13")
    parser.add_argument("--output", type=Path, default="output")
    parser.add_argument("--uv", type=Path, default=Path("uv"))
    parser.add_argument("--limit", type=int, default=None)
    parser.add_argument("--cache", type=Path, default=cwd.joinpath("cache"))
    parser.add_argument("--offline", action="store_true")
    parser.add_argument("--latest", action="store_true")
    args = parser.parse_args()

    if args.mode == "pyproject-toml":
        project_tomls = sorted((file.stem, file) for file in args.input.iterdir())
        jobs = {}
        no_project = 0
        dynamic_dependencies = 0
        for package, file in project_tomls:
            if len(jobs) >= args.limit:
                break
            if file.suffix != ".toml":
                continue
            project_toml = file.read_text()
            data = tomllib.loads(project_toml)
            project = data.get("project")
            if not project:
                no_project += 1
                continue
            if dynamic := project.get("dynamic"):
                if "dependencies" in dynamic:
                    dynamic_dependencies += 1
                    continue
                if "version" in dynamic:
                    dynamic.remove("version")
                # Usually there are no cycles back to the current project, so any version works
                project["version"] = "1.0.0"

            jobs[package] = tomli_w.dumps(data)

        print(f"`pyproject.toml`s without `[project]`: {no_project}")
        print(
            f"`pyproject.toml`s with `dynamic = ['dependencies']`: {dynamic_dependencies}"
        )
        if args.latest:
            raise ValueError("Latest versions are not supported in pyproject-toml mode")
    else:
        project_names = json.loads(args.input.read_text())
        project_names = sorted(pkg["project"] for pkg in project_names["rows"])

        if args.latest:
            with cwd.joinpath("package_versions.csv").open() as f:
                latest_versions = {
                    row["package_name"]: row["latest_version"]
                    for row in csv.DictReader(f)
                }
        else:
            latest_versions = None

        jobs = {}
        for package in project_names[: args.limit]:
            if latest_versions:
                if version := latest_versions.get(package):
                    jobs[package] = f"{package}=={version}"
                else:
                    tqdm.write(f"Missing version: {package}")
                    continue
            else:
                jobs[package] = package

    excluded_packages = [
        # 5000 releases, no solution
        "nucliadb",
        # These packages have many non-small versions
        "tf-models-nightly",
        "mtmtrain",
        "llm-dialog-manager",
        "python-must",
        # Slow and have no solution
        "edx-enterprise",
        "kcli",
        "emmet-api",
    ]
    for package in excluded_packages:
        jobs.pop(package, None)

    if args.output.exists():
        shutil.rmtree(args.output)
    args.output.mkdir(parents=True)
    args.output.joinpath(".gitignore").write_text("*")
    parameters = {
        "mode": args.mode,
        "python": args.python,
        "latest": args.latest,
    }
    args.output.joinpath("parameters.json").write_text(json.dumps(parameters))

    success = 0
    all_results = []  # Track all results for analysis
    max_package_len = max(len(package) for package in jobs)

    with ThreadPoolExecutor(max_workers=os.cpu_count() * 2) as executor:
        tasks = []
        packages_pending = []
        for package, specification in jobs.items():
            packages_pending.append(package)

            tasks.append(
                executor.submit(
                    run_uv,
                    package,
                    specification,
                    args.uv,
                    args.mode,
                    args.python,
                    args.cache,
                    args.offline,
                    args.output,
                )
            )
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
            # On linux, max RSS is in KB, on macOS, it is in bytes
            if platform.system() == "Linux":
                max_rss = summary.max_rss / 1024
            elif platform.system() == "Darwin":
                max_rss = summary.max_rss / 1024 / 1024
            else:
                raise NotImplementedError(f"Unknown platform: {platform.system()}")
            print(
                f"{summary.package}: {max_rss:.1f} MB (exit code: {summary.exit_code})"
            )


if __name__ == "__main__":
    main()
