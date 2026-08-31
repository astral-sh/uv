"""Replay fixed PGO install/sync workloads with isolated, preseeded offline caches.

Run on one Windows host, with the checkout used by the release training script:
  python replay_pgo_training.py --repo . --workdir C:/pgo-replay \
    --binary old=C:/old/uv.exe --binary new=C:/new/uv.exe --rounds 2

Preparation and directory cleanup are outside measured intervals. Each binary gets
its own copy of the seed cache. Every sample installs into a fresh directory.
Profile merging follows production's per-workload-group LLVM_PROFILE_FILE pattern.
The default two rounds run old/new/new/old for every project and command.
"""

# /// script
# requires-python = ">=3.11"
# dependencies = []
# ///

from __future__ import annotations

import argparse
import contextlib
import hashlib
import importlib.metadata
import importlib.util
import io
import json
import os
import re
import shutil
import subprocess
import sys
import time
from pathlib import Path


def load_training_script(repo: Path):
    spec = importlib.util.spec_from_file_location(
        "fixed_pgo_training", repo / "scripts" / "build_uv_pgo.py"
    )
    if spec is None or spec.loader is None:
        raise RuntimeError("Could not import the fixed PGO training script")
    module = importlib.util.module_from_spec(spec)
    sys.modules[spec.name] = module
    spec.loader.exec_module(module)
    return module


def commands_for(module, binary: Path, corpus: Path, profile_dir: Path):
    commands = []

    def record(command, *, environment, **kwargs):
        commands.append((command, environment))

    original_run = module.run
    module.run = record
    try:
        with contextlib.redirect_stdout(io.StringIO()):
            labels = module.run_workloads(
                binary,
                binary.with_name("missing-replay-launcher"),
                corpus,
                os.environ.copy(),
                profile_dir=profile_dir,
            )
    finally:
        module.run = original_run
    if len(labels) != len(commands):
        raise RuntimeError("Training labels and commands do not match")
    return dict(zip(labels, commands, strict=True))


def file_sha256(path: Path) -> str:
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def installed_manifest(directory: Path) -> list[tuple[str, str]]:
    return sorted(
        (
            re.sub(r"[-_.]+", "-", distribution.metadata["Name"]).lower(),
            distribution.version,
        )
        for distribution in importlib.metadata.distributions(path=[str(directory)])
    )


def verify_cache_clone(seed: Path, corpus: Path) -> dict[str, object]:
    """Check one regular wheel payload file per archive for identity and content.

    Windows cache references are regular files containing a relative archive ID,
    and Unix cache references are relative symlinks. Both survive relocation. The
    explicit payload checks also catch unexpected references to the seed tree.
    """
    checked = 0
    for original in (seed / "cache").glob("*/archive-v0/*/*.dist-info/METADATA"):
        copied = corpus / original.relative_to(seed)
        if not copied.resolve().is_relative_to(corpus):
            raise RuntimeError(f"Cache payload points outside its variant: {copied}")
        if copied.samefile(original):
            raise RuntimeError(f"Cache payload still shares the seed file: {copied}")
        if file_sha256(copied) != file_sha256(original):
            raise RuntimeError(f"Cache payload changed during copy: {copied}")
        checked += 1
    if checked == 0:
        raise RuntimeError("No cached wheel payloads were checked")
    return {"independent_archive_payloads_checked": checked, "shared_python_only": True}


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repo", type=Path, required=True)
    parser.add_argument("--workdir", type=Path, required=True)
    parser.add_argument(
        "--binary", action="append", required=True, metavar="LABEL=PATH"
    )
    parser.add_argument(
        "--projects", nargs="+", default=["flask", "sentry", "zulip", "pyx-workspace"]
    )
    parser.add_argument("--rounds", type=int, default=2)
    parser.add_argument("--timeout", type=float, default=900)
    args = parser.parse_args()
    if args.rounds < 1:
        parser.error("--rounds must be positive")
    binaries = {}
    for value in args.binary:
        label, separator, filename = value.partition("=")
        if not separator or re.fullmatch(r"[A-Za-z0-9_-]+", label) is None:
            parser.error("--binary requires a simple LABEL=PATH")
        path = Path(filename).resolve()
        if not path.is_file() or label in binaries:
            parser.error(f"Missing binary or duplicate label: {value}")
        binaries[label] = path
    if len(binaries) < 2:
        parser.error("At least two binaries are needed for a comparison")
    repo = args.repo.resolve()
    workdir = args.workdir.resolve()
    if workdir.exists():
        parser.error("--workdir must be a new directory, to preserve existing data")
    module = load_training_script(repo)
    projects = {project.name: project for project in module.CORPUS_PROJECTS}
    unknown = set(args.projects) - projects.keys()
    if unknown:
        parser.error(f"Unknown corpus projects: {sorted(unknown)}")
    module.CORPUS_PROJECTS = tuple(projects[name] for name in args.projects)
    workdir.mkdir(parents=True)
    (workdir / "logs").mkdir()
    (workdir / "profiles").mkdir()
    records_path = workdir / "commands.jsonl"

    def execute(label, command, environment, *, phase, variant=None, sample=None):
        command = [argument for argument in command if argument != "--quiet"]
        environment = environment.copy()
        environment.pop("UV_PROJECT_ENVIRONMENT", None)
        log_path = (
            workdir / "logs" / f"{phase}-{variant or 'seed'}-{sample or 0}-{label}.log"
        )
        print(f"[{phase}] {variant or 'seed'} {label}", flush=True)
        started = time.perf_counter()
        with log_path.open("wb") as output:
            try:
                result = subprocess.run(
                    command,
                    cwd=repo,
                    env=environment,
                    stdout=output,
                    stderr=subprocess.STDOUT,
                    timeout=args.timeout,
                    check=False,
                )
                returncode = result.returncode
            except subprocess.TimeoutExpired:
                returncode = "timeout"
        elapsed = time.perf_counter() - started
        record = {
            "phase": phase,
            "variant": variant,
            "sample": sample,
            "label": label,
            "seconds": elapsed,
            "returncode": returncode,
            "command": command,
            "log": str(log_path),
            "offline": environment.get("UV_OFFLINE") == "1",
            "cache": environment.get("UV_CACHE_DIR"),
            "profile": environment.get("LLVM_PROFILE_FILE"),
        }
        with records_path.open("a", encoding="utf-8") as output:
            output.write(json.dumps(record) + "\n")
        print(json.dumps(record), flush=True)
        if returncode != 0:
            print(log_path.read_text(encoding="utf-8", errors="replace"), flush=True)
            raise RuntimeError(f"Command failed: {label} ({returncode})")
        return record

    versions = {}
    for label, binary in binaries.items():
        environment = os.environ | {
            "LLVM_PROFILE_FILE": str(
                workdir / "profiles" / f"version-{label}-%m.profraw"
            )
        }
        result = subprocess.run(
            [str(binary), "--version"],
            env=environment,
            capture_output=True,
            text=True,
            check=True,
        )
        versions[label] = {
            "path": str(binary),
            "sha256": file_sha256(binary),
            "version": result.stdout.strip(),
        }
    metadata = {
        "binaries": versions,
        "projects": args.projects,
        "rounds": args.rounds,
        "python": sys.version,
        "platform": sys.platform,
        "cpu_count": os.cpu_count(),
        "training_script_sha256": file_sha256(repo / "scripts" / "build_uv_pgo.py"),
        "method": "preseeded offline per-variant cache; fresh destinations; alternating order",
    }
    (workdir / "metadata.json").write_text(json.dumps(metadata, indent=2) + "\n")

    # Resolve and warm the precise production workloads using the first binary.
    # The resulting lockfiles, requirements, and wheel cache are copied to all variants.
    seed = workdir / "seed"
    module.prepare_corpus(seed)
    seed_plans = commands_for(
        module, next(iter(binaries.values())), seed, workdir / "profiles"
    )
    for label, (command, environment) in seed_plans.items():
        if label == "python-install" or any(
            label.startswith(group + "-")
            for group in ("lock", "export", "install", "sync")
        ):
            execute(label, command, environment, phase="prepare")

    # Avoid copying installed environments or Python distributions. Only the immutable
    # training interpreters are shared; each variant's uv cache remains independent.
    for project in args.projects:
        for child in ("installed", ".venv"):
            destination = seed / project / child
            if destination.exists():
                shutil.rmtree(destination)
    plans = {}
    metadata["cache_isolation"] = {}
    for variant, binary in binaries.items():
        corpus = workdir / "variants" / variant
        print(f"Copying seed corpus/cache for {variant}", flush=True)
        shutil.copytree(
            seed,
            corpus,
            symlinks=True,
            ignore=lambda directory, names: (
                ("python",) if Path(directory) == seed else ()
            ),
        )
        metadata["cache_isolation"][variant] = verify_cache_clone(seed, corpus)
        print(
            f"Cache isolation {variant}: {metadata['cache_isolation'][variant]}",
            flush=True,
        )
        profiles = workdir / "profiles" / variant
        profiles.mkdir()
        plans[variant] = commands_for(module, binary, corpus, profiles)
        for _command, environment in plans[variant].values():
            environment["UV_PYTHON_INSTALL_DIR"] = str(seed / "python")
            environment["UV_OFFLINE"] = "1"
    (workdir / "metadata.json").write_text(json.dumps(metadata, indent=2) + "\n")

    comparisons = []
    for project in args.projects:
        for group in ("install", "sync"):
            expected = None
            label = f"{group}-{project}"
            for sample in range(args.rounds):
                order = list(binaries)
                if sample % 2:
                    order.reverse()
                for variant in order:
                    corpus = workdir / "variants" / variant
                    destination = (
                        corpus
                        / project
                        / ("installed" if group == "install" else ".venv")
                    )
                    if destination.exists():
                        shutil.rmtree(destination)
                    command, environment = plans[variant][label]
                    record = execute(
                        label,
                        command,
                        environment,
                        phase="measure",
                        variant=variant,
                        sample=sample,
                    )
                    if group == "install":
                        site_packages = destination
                    elif sys.platform == "win32":
                        site_packages = destination / "Lib" / "site-packages"
                    else:
                        site_packages = (
                            destination
                            / "lib"
                            / f"python{projects[project].python_version}"
                            / "site-packages"
                        )
                    manifest = installed_manifest(site_packages)
                    if not manifest:
                        raise RuntimeError(
                            f"No installed package metadata at {site_packages}"
                        )
                    if expected is None:
                        expected = manifest
                    elif manifest != expected:
                        raise RuntimeError(
                            f"Installed package mismatch: {variant} {label}"
                        )
                    record["packages"] = manifest
                    comparisons.append(record)
                    (workdir / "measurements.json").write_text(
                        json.dumps(comparisons, indent=2) + "\n"
                    )
    print(
        f"Completed {len(comparisons)} comparable measurements: {workdir}", flush=True
    )


if __name__ == "__main__":
    main()
