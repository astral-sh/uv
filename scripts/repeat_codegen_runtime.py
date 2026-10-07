"""Repeat offline runtime measurements using previously measured native binaries."""

import hashlib
import json
import os
import shutil
import subprocess
import sys

import psutil
from compare_codegen_units import (
    EVIDENCE,
    REPOSITORY,
    ROOT,
    load_pipeline,
    output,
    save,
)


def wait_for_idle(round_number):
    samples = []
    for _ in range(180):
        samples.append(psutil.cpu_percent(interval=1))
        if len(samples) >= 5 and max(samples[-5:]) < 10:
            save(
                EVIDENCE / f"runtime-repeat-idle-{round_number}.json",
                {"cpu_busy_percent": samples, "idle": True},
            )
            return
    save(
        EVIDENCE / f"runtime-repeat-idle-{round_number}.json",
        {"cpu_busy_percent": samples, "idle": False},
    )
    raise RuntimeError("Runner did not become idle; no timing round started")


def main():
    build = json.loads((EVIDENCE / "build-comparison.json").read_text())
    suffix = ".exe" if os.name == "nt" else ""
    for units in (16, 1):
        for name in (f"uv{suffix}", f"uvx{suffix}"):
            binary = ROOT / f"binaries/cgu{units}" / name
            expected = build["binaries"][str(units)][name]["sha256"]
            if hashlib.sha256(binary.read_bytes()).hexdigest() != expected:
                raise RuntimeError(f"Binary digest mismatch: {binary}")
            if os.name != "nt":
                binary.chmod(0o755)

    candidate = ROOT / "binaries/cgu1" / f"uv{suffix}"
    baseline = ROOT / "binaries/cgu16" / f"uv{suffix}"
    corpus = ROOT / "runtime-corpus"
    pipeline = load_pipeline()
    pipeline.CORPUS_PROJECTS = tuple(
        project
        for project in pipeline.CORPUS_PROJECTS
        if project.name in ("sentry", "zulip")
    )
    pipeline.prepare_corpus(corpus)
    pipeline.run_workloads(
        candidate, candidate.with_name(f"uvx{suffix}"), corpus, os.environ.copy()
    )
    interpreter = "Scripts/python.exe" if os.name == "nt" else "bin/python"
    runtime_root = ROOT / "runtime-repeat"
    environment = os.environ | {
        "UV_CGU_RUNTIME_ROOT": str(runtime_root),
        "UV_CGU_CORPUS": str(corpus),
        "UV_CGU_REVISION": build["experiment_revision"],
        "UV_CGU_CANDIDATE": str(candidate),
        "UV_CGU_BASELINE": str(baseline),
        "UV_CGU_PYTHON": output(
            [
                str(corpus / "sentry/.venv" / interpreter),
                "-c",
                "import sys; print(sys._base_executable)",
            ]
        ),
        "UV_CGU_ZULIP_PYTHON": output(
            [
                str(corpus / "zulip/.venv" / interpreter),
                "-c",
                "import sys; print(sys._base_executable)",
            ]
        ),
    }
    shutil.copy2(EVIDENCE / "corpus-comparison.json", ROOT / "corpus-comparison.json")
    harness = REPOSITORY / "scripts/compare_codegen_runtime.py"
    subprocess.run(
        [sys.executable, "-u", str(harness), "--prepare"],
        env=environment,
        check=True,
    )
    for round_number in (1, 2):
        wait_for_idle(round_number)
        subprocess.run(
            [
                sys.executable,
                "-u",
                str(harness),
                "--run",
                "--confirm-idle-host",
                "--repetitions",
                "30",
                "--output",
                str(EVIDENCE / f"runtime-repeat-{round_number}.json"),
            ],
            env=environment,
            check=True,
        )


if __name__ == "__main__":
    main()
