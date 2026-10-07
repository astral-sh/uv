"""Same-source PGO runtime comparison. All fixtures and caches stay outside the repository.

Prepare validates fixtures without recording timings. Run requires both completed native
PGO binaries and an idle host. Cold means empty uv cache, not cold OS page cache.
"""

import argparse
import base64
import csv
import hashlib
import io
import json
import os
import platform
import shutil
import statistics
import subprocess
import time
import zipfile
from pathlib import Path

import psutil

ROOT = Path(os.environ["UV_CGU_RUNTIME_ROOT"]).resolve()
CORPUS = Path(os.environ["UV_CGU_CORPUS"])
REVISION = os.environ["UV_CGU_REVISION"]
CANDIDATE = Path(os.environ["UV_CGU_CANDIDATE"])
BASELINE = Path(os.environ["UV_CGU_BASELINE"])
PYTHON = Path(os.environ["UV_CGU_PYTHON"])
ZULIP_PYTHON = Path(os.environ["UV_CGU_ZULIP_PYTHON"])
ACTIVE = ROOT / "active"
CACHE = ACTIVE / "cache"
PROJECT = ACTIVE / "project"
SOURCE = ACTIVE / "source"
VENV = PROJECT / ".venv"
WHEELS = ROOT / "fixtures" / "wheels"
TEMPLATES = ROOT / "fixtures" / "templates"
SEEDS = ROOT / "seeds"
ENV = {
    key: value
    for key, value in os.environ.items()
    if not key.startswith("UV_") and key not in ("VIRTUAL_ENV", "CONDA_PREFIX")
}
ENV.update(
    {
        "UV_PYTHON_DOWNLOADS": "never",
        "UV_NO_PROGRESS": "1",
        "UV_OFFLINE": "1",
        "UV_CONCURRENT_DOWNLOADS": "4",
        "UV_CONCURRENT_BUILDS": "2",
        "UV_CONCURRENT_INSTALLS": "4",
        "PYTHONDONTWRITEBYTECODE": "1",
        "NO_COLOR": "1",
    }
)


def digest(data):
    return hashlib.sha256(data).hexdigest()


def copytree(source, destination):
    shutil.copytree(source, destination, symlinks=True, dirs_exist_ok=True)


def remove(path):
    if path.is_symlink() or path.is_file():
        path.unlink()
    elif path.exists():
        shutil.rmtree(path)


def wheel(name, version, requirements=(), files=None):
    normalized = name.replace("-", "_")
    info = f"{normalized}-{version}.dist-info"
    contents = {
        f"{normalized}/__init__.py": f"VERSION = {version!r}\n".encode(),
        f"{info}/METADATA": (
            "Metadata-Version: 2.3\nName: "
            + name
            + "\nVersion: "
            + version
            + "\n"
            + "".join(
                "Requires-Dist: " + requirement + "\n" for requirement in requirements
            )
        ).encode(),
        f"{info}/WHEEL": b"Wheel-Version: 1.0\nGenerator: uv-cgu-runtime-fixture\nRoot-Is-Purelib: true\nTag: py3-none-any\n",
    }
    if files:
        contents.update(files)
    else:
        # Deterministic package data makes fresh installs exercise actual file copies.
        contents[f"{normalized}/data.bin"] = b"".join(
            hashlib.sha256(f"{name}:{version}:{number}".encode()).digest()
            for number in range(256)
        )
    record = io.StringIO()
    writer = csv.writer(record, lineterminator="\n")
    for filename, data in sorted(contents.items()):
        writer.writerow(
            [
                filename,
                "sha256="
                + base64.urlsafe_b64encode(hashlib.sha256(data).digest())
                .rstrip(b"=")
                .decode(),
                len(data),
            ]
        )
    writer.writerow([f"{info}/RECORD", "", ""])
    contents[f"{info}/RECORD"] = record.getvalue().encode()
    destination = WHEELS / f"{normalized}-{version}-py3-none-any.whl"
    with zipfile.ZipFile(destination, "w", compression=zipfile.ZIP_DEFLATED) as archive:
        for filename, data in sorted(contents.items()):
            entry = zipfile.ZipInfo(filename, (2020, 1, 1, 0, 0, 0))
            entry.compress_type = zipfile.ZIP_DEFLATED
            entry.external_attr = 0o644 << 16
            archive.writestr(entry, data)
    return destination


BACKEND = """from pathlib import Path
import zipfile

NAME = "bench_source"
VERSION = "1.0.0"
METADATA = b"Metadata-Version: 2.3\\nName: bench-source\\nVersion: 1.0.0\\nRequires-Dist: bench-leaf-00==2.0.0\\n"
WHEEL = b"Wheel-Version: 1.0\\nRoot-Is-Purelib: true\\nTag: py3-none-any\\n"

def mark(hook):
    with Path("hooks.log").open("a") as stream:
        stream.write(hook + "\\n")

def get_requires_for_build_wheel(config_settings=None):
    import bench_builder
    assert bench_builder.VERSION == "1.0.0"
    mark("get_requires_for_build_wheel")
    return ["bench-helper==1.0.0"]

def prepare_metadata_for_build_wheel(metadata_directory, config_settings=None):
    import bench_builder, bench_helper
    assert bench_helper.VERSION == "1.0.0"
    mark("prepare_metadata_for_build_wheel")
    target = Path(metadata_directory) / f"{NAME}-{VERSION}.dist-info"
    target.mkdir()
    (target / "METADATA").write_bytes(METADATA)
    (target / "WHEEL").write_bytes(WHEEL)
    return target.name

def build_wheel(wheel_directory, config_settings=None, metadata_directory=None):
    import bench_builder, bench_helper
    assert bench_helper.VERSION == "1.0.0"
    mark("build_wheel")
    filename = f"{NAME}-{VERSION}-py3-none-any.whl"
    contents = {
        f"{NAME}/__init__.py": b"VERSION = '1.0.0'\\n",
        f"{NAME}-{VERSION}.dist-info/METADATA": METADATA,
        f"{NAME}-{VERSION}.dist-info/WHEEL": WHEEL,
        f"{NAME}-{VERSION}.dist-info/RECORD": b"",
    }
    with zipfile.ZipFile(Path(wheel_directory) / filename, "w", compression=zipfile.ZIP_DEFLATED) as archive:
        for path, data in sorted(contents.items()):
            entry = zipfile.ZipInfo(path, (2020, 1, 1, 0, 0, 0))
            entry.compress_type = zipfile.ZIP_DEFLATED
            entry.external_attr = 0o644 << 16
            archive.writestr(entry, data)
    return filename
"""


def fixtures():
    WHEELS.mkdir(parents=True, exist_ok=True)
    for version in range(1, 5):
        wheel("bench-shared", f"{version}.0.0")
        for number in range(50):
            wheel(f"bench-leaf-{number:02}", f"{version}.0.0")
            wheel(
                f"bench-branch-{number:02}",
                f"{version}.0.0",
                [
                    f"bench-shared=={version}.0.0",
                    f"bench-leaf-{number:02}=={version}.0.0",
                ],
            )
    wheel(
        "bench-application",
        "1.0.0",
        [
            "bench-shared==2.0.0",
            *(f"bench-branch-{number:02}>=1,<5" for number in range(50)),
        ],
    )
    wheel("bench-builder", "1.0.0")
    wheel("bench-helper", "1.0.0")
    TEMPLATES.mkdir(parents=True, exist_ok=True)
    project = TEMPLATES / "project"
    project.mkdir(exist_ok=True)
    (project / "pyproject.toml").write_text(
        '[project]\nname = "cgu-runtime-project"\nversion = "1.0.0"\nrequires-python = ">=3.13,<3.14"\ndependencies = ["bench-application==1.0.0"]\n\n[tool.uv]\npackage = false\nno-index = true\nfind-links = ['
        + json.dumps(str(WHEELS))
        + "]\n"
    )
    source = TEMPLATES / "source"
    source.mkdir(exist_ok=True)
    (source / "pyproject.toml").write_text(
        '[project]\nname = "bench-source"\nversion = "1.0.0"\nrequires-python = ">=3.13"\ndynamic = ["dependencies"]\n\n[build-system]\nrequires = ["bench-builder==1.0.0"]\nbuild-backend = "backend"\nbackend-path = ["."]\n'
    )
    (source / "backend.py").write_text(BACKEND)
    (ROOT / "fixtures" / "requirements.in").write_text("bench-application==1.0.0\n")
    (ROOT / "fixtures" / "source.in").write_text(
        "bench-source @ " + SOURCE.as_uri() + "\n"
    )
    for name in ("sentry", "zulip"):
        project = TEMPLATES / name
        project.mkdir(exist_ok=True)
        shutil.copy2(CORPUS / name / "pyproject.toml", project / "pyproject.toml")
        cache = SEEDS / f"ecosystem-{name}"
        cache.mkdir(parents=True, exist_ok=True)
        for bucket in ("simple-v25", "wheels-v6", "sdists-v9"):
            source_cache = CORPUS / "cache" / "universal" / name / bucket
            if source_cache.exists():
                copytree(source_cache, cache / bucket)
        # Metadata-only caches never need links to downloaded wheel archives.
        for link in cache.rglob("*"):
            if link.is_symlink():
                link.unlink()


CASES = [
    {
        "name": "startup-version",
        "category": "startup",
        "kind": "version",
        "repetitions": 30,
    },
    {"name": "startup-help", "category": "startup", "kind": "help", "repetitions": 30},
    {"name": "venv-fresh", "category": "filesystem", "kind": "venv"},
    {
        "name": "resolve-local-cold",
        "category": "resolver",
        "kind": "compile",
        "cold": True,
    },
    {"name": "resolve-local-warm", "category": "resolver", "kind": "compile"},
    {
        "name": "resolve-sentry-universal",
        "category": "resolver",
        "kind": "ecosystem",
        "ecosystem": "sentry",
    },
    {
        "name": "resolve-zulip-universal",
        "category": "resolver",
        "kind": "ecosystem",
        "ecosystem": "zulip",
    },
    {
        "name": "project-lock-fresh",
        "category": "resolver",
        "kind": "lock",
        "fresh": True,
    },
    {"name": "project-lock-noop", "category": "noop", "kind": "lock"},
    {"name": "project-export-frozen", "category": "noop", "kind": "export"},
    {"name": "sync-frozen-fresh", "category": "filesystem", "kind": "sync"},
    {"name": "sync-frozen-noop", "category": "noop", "kind": "sync", "noop": True},
    {
        "name": "install-local-cold",
        "category": "filesystem",
        "kind": "install",
        "cold": True,
    },
    {"name": "install-cached-fresh", "category": "filesystem", "kind": "install"},
    {
        "name": "source-metadata-cold",
        "category": "source-build",
        "kind": "metadata",
        "cold": True,
    },
    {"name": "source-wheel-build", "category": "source-build", "kind": "build"},
]


def base(binary):
    return [
        str(binary),
        "--cache-dir",
        str(CACHE),
        "--offline",
        "--no-progress",
        "--color",
        "never",
    ]


def command(case, binary):
    arguments = base(binary)
    kind = case["kind"]
    local = ["--no-index", "--find-links", str(WHEELS)]
    python = ["--python", str(PYTHON)]
    if kind == "version":
        return [str(binary), "--version"]
    if kind == "help":
        return [str(binary), "pip", "--help"]
    if kind == "venv":
        return arguments + ["venv", str(VENV), *python, "--quiet"]
    if kind == "compile":
        return arguments + [
            "pip",
            "compile",
            str(ROOT / "fixtures" / "requirements.in"),
            *python,
            *local,
            "--no-build",
            "--quiet",
            "--no-header",
            "--no-annotate",
        ]
    if kind == "ecosystem":
        interpreter = (
            str(PYTHON) if case["ecosystem"] == "sentry" else str(ZULIP_PYTHON)
        )
        groups = ["--group", "dev"] if case["ecosystem"] == "zulip" else []
        return arguments + [
            "pip",
            "compile",
            str(PROJECT / "pyproject.toml"),
            "--project",
            str(PROJECT),
            "--python",
            interpreter,
            *groups,
            "--exclude-newer",
            "2026-06-30T00:00:00Z",
            "--universal",
            "--no-build",
            "--quiet",
            "--no-header",
            "--no-annotate",
        ]
    if kind == "lock":
        return arguments + [
            "lock",
            "--project",
            str(PROJECT),
            *python,
            "--no-build",
            "--quiet",
        ]
    if kind == "export":
        return arguments + [
            "export",
            "--project",
            str(PROJECT),
            "--frozen",
            "--no-emit-workspace",
            "--no-header",
            "--no-hashes",
            "--quiet",
        ]
    if kind == "sync":
        return arguments + [
            "sync",
            "--project",
            str(PROJECT),
            *python,
            "--frozen",
            "--no-install-project",
            "--no-build",
            "--link-mode",
            "copy",
            "--quiet",
        ]
    if kind == "install":
        return arguments + [
            "pip",
            "install",
            "--target",
            str(ACTIVE / "installed"),
            *python,
            *local,
            "--no-build",
            "--link-mode",
            "copy",
            "--quiet",
            "-r",
            str(ROOT / "fixtures" / "requirements.txt"),
        ]
    if kind == "metadata":
        return arguments + [
            "pip",
            "compile",
            str(ROOT / "fixtures" / "source.in"),
            *python,
            *local,
            "--quiet",
            "--no-header",
            "--no-annotate",
        ]
    if kind == "build":
        return arguments + [
            "build",
            str(SOURCE),
            "--wheel",
            "--out-dir",
            str(ACTIVE / "dist"),
            *python,
            *local,
            "--quiet",
        ]
    raise ValueError(kind)


def reset(case, seeded=True):
    remove(ACTIVE)
    ACTIVE.mkdir()
    project = TEMPLATES / case.get("ecosystem", "project")
    copytree(project, PROJECT)
    copytree(TEMPLATES / "source", SOURCE)
    kind = case["kind"]
    if case.get("fresh"):
        remove(PROJECT / "uv.lock")
    seed = SEEDS / case["name"] / "cache"
    if kind == "ecosystem":
        seed = SEEDS / ("ecosystem-" + case["ecosystem"])
    if seeded and not case.get("cold") and seed.exists():
        copytree(seed, CACHE)
    else:
        CACHE.mkdir()
    if seeded and case.get("noop"):
        copytree(SEEDS / case["name"] / "venv", VENV)


def invoke(case, binary, timed=False):
    start = time.perf_counter() if timed else None
    completed = subprocess.run(
        command(case, binary),
        env=ENV,
        cwd=ACTIVE,
        capture_output=True,
        check=False,
    )
    elapsed = time.perf_counter() - start if timed else None
    if completed.returncode:
        raise RuntimeError(
            f"{case['name']} failed ({completed.returncode})\n{completed.stderr.decode()}\nCommand: {command(case, binary)}"
        )
    return completed, elapsed


def file_manifest(directory):
    # uv does not compile bytecode in these workloads. Exclude interpreter-created
    # __pycache__ only; all package files, metadata and RECORD contents are compared.
    files = {}
    for path in sorted(directory.rglob("*")):
        if "__pycache__" in path.parts:
            continue
        relative = str(path.relative_to(directory))
        if path.is_symlink():
            files[relative] = {"link": str(path.readlink())}
        elif path.is_file():
            files[relative] = {
                "sha256": digest(path.read_bytes()),
                "bytes": path.stat().st_size,
            }
    return files


def fingerprint(case, completed):
    kind = case["kind"]
    result = {"stdout_sha256": digest(completed.stdout)}
    if kind == "lock":
        result["lock_sha256"] = digest((PROJECT / "uv.lock").read_bytes())
    if kind in ("sync", "install"):
        installed = (
            ACTIVE / "installed"
            if kind == "install"
            else VENV
            / (
                "Lib/site-packages"
                if os.name == "nt"
                else "lib/python3.13/site-packages"
            )
        )
        result["installed_files"] = file_manifest(installed)
        modules = [
            "bench_shared",
            "bench_branch_00",
            "bench_branch_49",
            "bench_leaf_00",
            "bench_leaf_49",
        ]
        script = (
            "import sys;sys.path.insert(0,"
            + repr(str(installed))
            + ");"
            + ";".join("import " + module for module in modules)
            + ";print("
            + ",".join(module + ".VERSION" for module in modules)
            + ")"
        )
        output = subprocess.check_output(
            [str(PYTHON), "-I", "-B", "-c", script], env=ENV
        )
        if output.decode().split() != ["2.0.0"] * 5:
            raise RuntimeError(f"Unexpected installed versions: {output!r}")
        result["imports"] = output.decode().strip()
    if kind == "venv":
        result["venv_files"] = file_manifest(VENV)
        result["interpreter"] = subprocess.check_output(
            [
                str(VENV / ("Scripts/python.exe" if os.name == "nt" else "bin/python")),
                "-I",
                "-B",
                "-c",
                "import sys; print(sys.version); print(sys.prefix)",
            ],
            env=ENV,
        ).decode()
    if kind in ("metadata", "build"):
        hooks = (SOURCE / "hooks.log").read_text().splitlines()
        needed = (
            "prepare_metadata_for_build_wheel" if kind == "metadata" else "build_wheel"
        )
        if needed not in hooks or "get_requires_for_build_wheel" not in hooks:
            raise RuntimeError(f"Backend hooks missing: {hooks}")
        result["backend_hooks"] = hooks
    if kind == "build":
        wheels = list((ACTIVE / "dist").glob("*.whl"))
        if len(wheels) != 1:
            raise RuntimeError(f"Wrong wheel outputs: {wheels}")
        with zipfile.ZipFile(wheels[0]) as archive:
            result["wheel_contents"] = {
                name: digest(archive.read(name)) for name in sorted(archive.namelist())
            }
    return result


def prepare(binary):
    fixtures()
    reset({"name": "bootstrap", "kind": "compile"}, seeded=False)
    completed, _ = invoke({"name": "bootstrap", "kind": "compile"}, binary)
    (ROOT / "fixtures" / "requirements.txt").write_bytes(completed.stdout)
    completed, _ = invoke({"name": "bootstrap-lock", "kind": "lock"}, binary)
    shutil.copy2(PROJECT / "uv.lock", TEMPLATES / "project" / "uv.lock")
    references = {}
    for case in CASES:
        print("Preparing", case["name"], flush=True)
        reset(case, seeded=case["kind"] == "ecosystem")
        completed, _ = invoke(case, binary)
        if not case.get("cold") and case["kind"] not in (
            "version",
            "help",
            "ecosystem",
        ):
            seed = SEEDS / case["name"]
            seed.mkdir(parents=True, exist_ok=True)
            if (seed / "cache").exists():
                remove(seed / "cache")
            copytree(CACHE, seed / "cache")
            if case.get("noop"):
                if (seed / "venv").exists():
                    remove(seed / "venv")
                copytree(VENV, seed / "venv")
        reset(case)
        completed, _ = invoke(case, binary)
        references[case["name"]] = fingerprint(case, completed)
        (ROOT / f"prepare-{case['name']}.stdout").write_bytes(completed.stdout)
        print("Validated", case["name"], flush=True)
    metadata = {
        "revision": REVISION,
        "candidate": str(binary),
        "candidate_sha256": digest(binary.read_bytes()),
        "python": subprocess.check_output(
            [str(PYTHON), "--version"], text=True
        ).strip(),
        "cases": CASES,
        "references": references,
        "notes": [
            "No recorded preparation timings.",
            "Cold means empty uv cache; OS filesystem caches are not cleared.",
            "Working directories and virtualenv paths are reused sequentially.",
            "Only __pycache__ files are excluded from installed/venv fingerprints.",
            "Package file contents, metadata, RECORD, lockfiles and requirements output otherwise compare exactly.",
            "Source backend marker verifies metadata/build hooks actually run.",
            "Filesystem installs use explicit copy mode, avoiding hardlink/reflink selection differences.",
        ],
    }
    (ROOT / "prepared.json").write_text(json.dumps(metadata, indent=2) + "\n")
    print("Prepared", len(CASES), "cases without timing.", flush=True)


def percentiles(values):
    ordered = sorted(values)

    def percentile(fraction):
        index = (len(ordered) - 1) * fraction
        lower = int(index)
        upper = min(lower + 1, len(ordered) - 1)
        return ordered[lower] + (ordered[upper] - ordered[lower]) * (index - lower)

    return {
        "median_seconds": statistics.median(values),
        "min_seconds": min(values),
        "max_seconds": max(values),
        "p10_seconds": percentile(0.1),
        "p90_seconds": percentile(0.9),
        "iqr_seconds": percentile(0.75) - percentile(0.25),
        "samples_seconds": values,
    }


def host_context():
    return {
        "timestamp_utc": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
        "os": dict(platform.uname()._asdict()),
        "libc": platform.libc_ver(),
        "logical_cpus": psutil.cpu_count(),
        "physical_cpus": psutil.cpu_count(logical=False),
        "memory": psutil.virtual_memory()._asdict(),
        "cpu_times": psutil.cpu_times()._asdict(),
    }


def benchmark(baseline, candidate, repetitions, selected_cases, output):
    metadata = json.loads((ROOT / "prepared.json").read_text())
    if digest(candidate.read_bytes()) != metadata["candidate_sha256"]:
        raise RuntimeError("Candidate changed since fixture preparation")
    binaries = {"cgu16": baseline, "cgu1": candidate}
    result = {
        "revision": REVISION,
        "method": metadata["notes"],
        "binaries": {
            name: {
                "path": str(path),
                "bytes": path.stat().st_size,
                "sha256": digest(path.read_bytes()),
                "version": subprocess.check_output(
                    [str(path), "--version"], text=True
                ).strip(),
            }
            for name, path in binaries.items()
        },
        "cases": [],
    }
    versions = {item["version"] for item in result["binaries"].values()}
    if len(versions) != 1:
        raise RuntimeError(f"Binary version mismatch: {versions}")
    result["binary_versions_equal"] = True
    result["host_at_start"] = host_context()
    controls = ROOT.parent / "same-revision-controls.json"
    if controls.exists():
        result["build_controls"] = json.loads(controls.read_text())
    corpus_comparison = ROOT.parent / "corpus-comparison.json"
    if corpus_comparison.exists():
        result["training_corpus_comparison"] = json.loads(corpus_comparison.read_text())
        if not result["training_corpus_comparison"]["all_match"]:
            raise RuntimeError("PGO training corpus or Python versions differ")
    cases = [
        case for case in CASES if not selected_cases or case["name"] in selected_cases
    ]
    result["selected_cases"] = [case["name"] for case in cases]
    result["warmups_per_binary"] = 2
    samples_path = output.with_suffix(".samples.jsonl")
    samples_path.write_text("")
    result["raw_samples_path"] = str(samples_path)
    for case in cases:
        expected = metadata["references"][case["name"]]
        for binary in binaries.values():
            for _ in range(2):
                reset(case)
                completed, _ = invoke(case, binary)
                if fingerprint(case, completed) != expected:
                    raise RuntimeError("Warmup output mismatch: " + case["name"])
        samples = {name: [] for name in binaries}
        for trial in range(max(case.get("repetitions", 0), repetitions)):
            for name in ["cgu16", "cgu1"] if trial % 2 == 0 else ["cgu1", "cgu16"]:
                reset(case)
                completed, duration = invoke(case, binaries[name], timed=True)
                matches = fingerprint(case, completed) == expected
                with samples_path.open("a") as sample_file:
                    sample_file.write(
                        json.dumps(
                            {
                                "case": case["name"],
                                "trial": trial,
                                "binary": name,
                                "seconds": duration,
                                "outputs_equal": matches,
                            }
                        )
                        + "\n"
                    )
                if not matches:
                    raise RuntimeError(
                        "Timed output mismatch: " + case["name"] + " " + name
                    )
                samples[name].append(duration)
        entry = {
            "name": case["name"],
            "category": case["category"],
            "outputs_equal": True,
            "timings": {name: percentiles(values) for name, values in samples.items()},
        }
        entry["cgu1_change_percent"] = (
            entry["timings"]["cgu1"]["median_seconds"]
            / entry["timings"]["cgu16"]["median_seconds"]
            - 1
        ) * 100
        result["cases"].append(entry)
        print(
            case["name"],
            json.dumps(
                {
                    name: round(statistics.median(values) * 1000, 3)
                    for name, values in samples.items()
                }
            ),
            "ms",
            round(entry["cgu1_change_percent"], 2),
            "%",
            flush=True,
        )
        output.write_text(json.dumps(result, indent=2) + "\n")
    result["host_at_end"] = host_context()
    output.write_text(json.dumps(result, indent=2) + "\n")
    print(output, flush=True)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--prepare", action="store_true")
    parser.add_argument("--run", action="store_true")
    parser.add_argument("--confirm-idle-host", action="store_true")
    parser.add_argument("--candidate", type=Path, default=CANDIDATE)
    parser.add_argument(
        "--baseline",
        type=Path,
        default=BASELINE,
    )
    parser.add_argument("--repetitions", type=int, default=15)
    parser.add_argument(
        "--case", action="append", choices=[case["name"] for case in CASES]
    )
    parser.add_argument("--output", type=Path, default=ROOT / "comparison.json")
    args = parser.parse_args()
    if args.prepare == args.run:
        parser.error("Choose exactly one of --prepare or --run")
    ROOT.mkdir(parents=True, exist_ok=True)
    try:
        lock = os.open(ROOT / "run.lock", os.O_CREAT | os.O_EXCL | os.O_WRONLY)
    except FileExistsError:
        parser.error("Another preparation or benchmark holds run.lock")
    try:
        if args.prepare:
            prepare(args.candidate)
        else:
            if not args.confirm_idle_host:
                parser.error(
                    "Timing requires --confirm-idle-host after compilation finishes"
                )
            benchmark(
                args.baseline, args.candidate, args.repetitions, args.case, args.output
            )
    finally:
        os.close(lock)
        (ROOT / "run.lock").unlink()
