"""Compare complete PGO builds and offline workloads on one native runner."""

from __future__ import annotations

import hashlib
import importlib.util
import json
import os
import platform
import shutil
import subprocess
import sys
import threading
import time
import tomllib
from pathlib import Path

import psutil

REPOSITORY = Path(__file__).resolve().parent.parent
ROOT = Path(os.environ["RUNNER_TEMP"]) / "cgu-comparison"
EVIDENCE = ROOT / "evidence"
SOURCE_REVISION = "f363bea422a1255f3928f686aa58a5e59e019090"


def save(path, value):
    path.write_text(json.dumps(value, indent=2) + "\n", encoding="utf-8")


def output(command, *, environment=None):
    return subprocess.check_output(
        command, cwd=REPOSITORY, env=environment, text=True
    ).strip()


def host():
    return {
        "timestamp_utc": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
        "platform": dict(platform.uname()._asdict()),
        "logical_cpus": psutil.cpu_count(),
        "physical_cpus": psutil.cpu_count(logical=False),
        "memory": psutil.virtual_memory()._asdict(),
        "cpu_times": psutil.cpu_times()._asdict(),
        "disk": shutil.disk_usage(ROOT)._asdict(),
    }


def measured_build(command, environment, units, stage):
    log_path = EVIDENCE / f"cgu{units}-{stage}.log"
    samples_path = EVIDENCE / f"cgu{units}-{stage}-memory.jsonl"
    print(f"Starting CGU{units} {stage}", flush=True)
    started = time.perf_counter()
    process = subprocess.Popen(
        command,
        cwd=REPOSITORY,
        env=environment,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        text=True,
        errors="replace",
    )

    def copy_log():
        with log_path.open("w", encoding="utf-8") as log:
            for line in process.stdout:
                log.write(line)
                log.flush()
                print(line, end="", flush=True)

    reader = threading.Thread(target=copy_log)
    reader.start()
    root_process = psutil.Process(process.pid)
    peak_single = 0
    peak_sum = 0
    peak_working_set = 0
    with samples_path.open("w", encoding="utf-8") as samples:
        while process.poll() is None:
            entries = []
            try:
                children = [root_process, *root_process.children(recursive=True)]
            except psutil.NoSuchProcess:
                children = []
            for child in children:
                try:
                    memory = child.memory_info()
                    entries.append(
                        {"pid": child.pid, "name": child.name(), "rss": memory.rss}
                    )
                    peak_single = max(peak_single, memory.rss)
                    peak_working_set = max(
                        peak_working_set, getattr(memory, "peak_wset", 0)
                    )
                except (psutil.NoSuchProcess, psutil.AccessDenied):
                    continue
            total = sum(entry["rss"] for entry in entries)
            peak_sum = max(peak_sum, total)
            samples.write(
                json.dumps(
                    {
                        "elapsed_seconds": time.perf_counter() - started,
                        "processes": entries,
                        "sum_rss_bytes": total,
                    }
                )
                + "\n"
            )
            samples.flush()
            time.sleep(0.5)
    returncode = process.wait()
    elapsed = time.perf_counter() - started
    reader.join()
    result = {
        "codegen_units": units,
        "stage": stage,
        "command": command,
        "elapsed_seconds": elapsed,
        "sampled_peak_process_rss_bytes": peak_single,
        "sampled_peak_sum_rss_bytes": peak_sum,
        "windows_observed_peak_working_set_bytes": peak_working_set or None,
        "memory_sampling_interval_seconds": 0.5,
        "exit_code": returncode,
        "rustflags": environment.get("RUSTFLAGS"),
    }
    save(EVIDENCE / f"cgu{units}-{stage}.json", result)
    if returncode:
        raise subprocess.CalledProcessError(returncode, command)
    print(json.dumps(result), flush=True)
    return result


def load_pipeline():
    spec = importlib.util.spec_from_file_location(
        "build_uv_pgo", REPOSITORY / "scripts/build_uv_pgo.py"
    )
    module = importlib.util.module_from_spec(spec)
    sys.modules[spec.name] = module
    spec.loader.exec_module(module)
    return module


def corpus_snapshot(directory, environment):
    snapshots = {}
    interpreters = {}
    for project in sorted(directory.iterdir()):
        if not project.is_dir() or not (project / "pyproject.toml").is_file():
            continue
        for name in (
            "pyproject.toml",
            "uv.lock",
            "requirements.in",
            "constraints.txt",
            "requirements.txt",
            "universal-requirements.txt",
            "exported-requirements.txt",
        ):
            path = project / name
            if not path.exists():
                continue
            value = path.read_text(encoding="utf-8")
            # Separate target directories are the only expected input-path difference.
            for units in (16, 1):
                prefix = str(ROOT / f"cgu{units}")
                value = value.replace(prefix.replace("\\", "\\\\"), "<TARGET>")
                value = value.replace(prefix.replace("\\", "/"), "<TARGET>")
                value = value.replace(prefix, "<TARGET>")
            if name.endswith(".toml") or name == "uv.lock":
                normalized = json.dumps(tomllib.loads(value), sort_keys=True)
            else:
                normalized = value
            snapshots[f"{project.name}/{name}"] = hashlib.sha256(
                normalized.encode()
            ).hexdigest()
        executable = (
            project
            / ".venv"
            / ("Scripts/python.exe" if os.name == "nt" else "bin/python")
        )
        if executable.exists():
            interpreters[project.name] = output(
                [str(executable), "--version"], environment=environment
            )
    return {"files": snapshots, "python_versions": interpreters}


def main():
    ROOT.mkdir(parents=True, exist_ok=True)
    EVIDENCE.mkdir(exist_ok=True)
    pipeline = load_pipeline()
    target = pipeline.rustc_host()
    if target not in ("aarch64-apple-darwin", "x86_64-pc-windows-msvc"):
        raise RuntimeError(
            f"Expected a native macOS ARM64 or Windows x86_64 runner, got {target}"
        )
    revision = output(["git", "rev-parse", "HEAD"])
    environment = os.environ.copy()
    for name in ("RUSTC_WRAPPER", "RUSTC_WORKSPACE_WRAPPER", "CARGO_ENCODED_RUSTFLAGS"):
        environment.pop(name, None)
    environment.update(
        {
            "CARGO_INCREMENTAL": "0",
            "CARGO_BUILD_JOBS": str(psutil.cpu_count()),
            "CARGO_TERM_COLOR": "never",
            "SCCACHE_DISABLE": "1",
            "AWS_LC_SYS_USE_SYSTEM": "0",
            "UV_LINK_MODE": "copy",
            "UV_COMPILE_BYTECODE": "0",
        }
    )
    if os.name == "nt":
        environment["CARGO"] = str(REPOSITORY / "scripts/cargo.cmd")
        environment["RUSTFLAGS"] = "-C target-feature=+crt-static"
        environment["AWS_LC_SYS_PREBUILT_NASM"] = "0"
    else:
        environment["CARGO"] = str(REPOSITORY / "scripts/cargo.sh")
        environment["RUSTFLAGS"] = (
            "-C linker=rust-lld -C linker-flavor=ld64.lld -C link-arg=--icf=safe"
        )
        environment["MACOSX_DEPLOYMENT_TARGET"] = "11.0"
    # Download Rust dependencies before either timed build.
    subprocess.run(
        ["cargo", "fetch", "--locked", "--target", target],
        cwd=REPOSITORY,
        env=environment,
        check=True,
    )
    metadata = {
        "source_revision": SOURCE_REVISION,
        "experiment_revision": revision,
        "target": target,
        "rustc": output(["rustc", "--version", "--verbose"]),
        "cargo": output(["cargo", "--version"]),
        "cargo_auditable": "0.7.6 (pinned by scripts/install-cargo-extensions.sh)",
        "host_before": host(),
        "order": [16, 1],
        "build_jobs": int(environment["CARGO_BUILD_JOBS"]),
        "note": "One fresh complete build per configuration on the same runner, sequentially. Cargo downloads occur before timing. OS file caches are not cleared, so build times remain single observations with a possible order effect.",
    }
    save(EVIDENCE / "build-comparison.json", metadata)
    original_run = pipeline.run
    results = []
    snapshots = {}
    binaries = {}
    for units in (16, 1):
        destination = ROOT / f"cgu{units}"
        if destination.exists():
            raise RuntimeError(f"Build target must be fresh: {destination}")
        current = environment | {
            "CARGO_PROFILE_RELEASE_CODEGEN_UNITS": str(units),
            "CARGO_TARGET_DIR": str(destination),
        }
        for name in ("CARGO", "CARGO_PROFILE_RELEASE_CODEGEN_UNITS"):
            os.environ[name] = current[name]
        os.environ.update(current)

        def run(command, *, environment, allowed_exit_codes=(0,), units=units):
            if "build" in command and "--package" in command:
                stage = (
                    "instrumented"
                    if "profile-generate=" in environment.get("RUSTFLAGS", "")
                    else "profile-use"
                )
                results.append(measured_build(command, environment, units, stage))
            else:
                original_run(
                    command,
                    environment=environment,
                    allowed_exit_codes=allowed_exit_codes,
                )

        pipeline.run = run
        sys.argv = [
            str(REPOSITORY / "scripts/build_uv_pgo.py"),
            "--target",
            target,
            "--target-dir",
            str(destination),
        ]
        started = time.perf_counter()
        pipeline.main()
        elapsed = time.perf_counter() - started
        snapshots[str(units)] = corpus_snapshot(destination / "corpus", current)
        copy_directory = ROOT / "binaries" / f"cgu{units}"
        copy_directory.mkdir(parents=True)
        executable_names = (
            ("uv.exe", "uvx.exe", "uvw.exe") if os.name == "nt" else ("uv", "uvx")
        )
        binaries[str(units)] = {}
        for name in executable_names:
            source = destination / target / "release" / name
            copy = copy_directory / name
            shutil.copy2(source, copy)
            binaries[str(units)][name] = {
                "path": str(copy),
                "bytes": copy.stat().st_size,
                "sha256": hashlib.sha256(copy.read_bytes()).hexdigest(),
            }
        binary = copy_directory / ("uv.exe" if os.name == "nt" else "uv")
        binaries[str(units)]["version"] = output(
            [str(binary), "--version"], environment=current
        )
        metadata[f"cgu{units}_pipeline_elapsed_seconds"] = elapsed
        metadata["stages"] = results
        metadata["binaries"] = binaries
        save(EVIDENCE / "build-comparison.json", metadata)
    comparison = {
        "all_match": snapshots["16"] == snapshots["1"],
        "snapshots": snapshots,
    }
    save(EVIDENCE / "corpus-comparison.json", comparison)
    if not comparison["all_match"]:
        raise RuntimeError("PGO training files or Python versions differ")
    if binaries["16"]["version"] != binaries["1"]["version"]:
        raise RuntimeError("Binary versions differ")
    metadata["host_after"] = host()
    save(EVIDENCE / "build-comparison.json", metadata)
    executable = "uv.exe" if os.name == "nt" else "uv"
    candidate = ROOT / "binaries/cgu1" / executable
    baseline = ROOT / "binaries/cgu16" / executable
    runtime_root = ROOT / "runtime"
    runtime_root.mkdir()
    # The existing training script has installed pinned managed Pythons already.
    python_environment = environment | {
        "UV_PYTHON_INSTALL_DIR": str(ROOT / "cgu1/corpus/python")
    }
    # Find the interpreter through the project's known-good trained environment.
    interpreter_name = "Scripts/python.exe" if os.name == "nt" else "bin/python"
    python = ROOT / "cgu1/corpus/sentry/.venv" / interpreter_name
    zulip_python = ROOT / "cgu1/corpus/zulip/.venv" / interpreter_name
    runtime_environment = environment | {
        "UV_CGU_RUNTIME_ROOT": str(runtime_root),
        "UV_CGU_CORPUS": str(ROOT / "cgu1/corpus"),
        "UV_CGU_REVISION": revision,
        "UV_CGU_PYTHON": output(
            [str(python), "-c", "import sys; print(sys._base_executable)"],
            environment=python_environment,
        ),
        "UV_CGU_ZULIP_PYTHON": output(
            [str(zulip_python), "-c", "import sys; print(sys._base_executable)"],
            environment=python_environment,
        ),
        "UV_CGU_CANDIDATE": str(candidate),
        "UV_CGU_BASELINE": str(baseline),
    }
    harness = REPOSITORY / "scripts/compare_codegen_runtime.py"
    subprocess.run(
        [sys.executable, "-u", str(harness), "--prepare"],
        cwd=REPOSITORY,
        env=runtime_environment,
        check=True,
    )
    idle_samples = []
    for _ in range(30):
        busy = psutil.cpu_percent(interval=1)
        idle_samples.append(busy)
        if len(idle_samples) >= 3 and max(idle_samples[-3:]) < 10:
            break
    save(EVIDENCE / "runtime-idle-check.json", {"cpu_busy_percent": idle_samples})
    shutil.copy2(EVIDENCE / "corpus-comparison.json", ROOT / "corpus-comparison.json")
    subprocess.run(
        [sys.executable, "-u", str(harness), "--run", "--confirm-idle-host"],
        cwd=REPOSITORY,
        env=runtime_environment,
        check=True,
    )
    for name in ("comparison.json", "comparison.samples.jsonl", "prepared.json"):
        shutil.copy2(runtime_root / name, EVIDENCE / f"runtime-{name}")
    print(f"Completed both builds and runtime comparison: {EVIDENCE}", flush=True)


if __name__ == "__main__":
    main()
