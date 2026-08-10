"""Build uv with profile-guided optimization using real ecosystem projects."""

from __future__ import annotations

import argparse
import os
import shlex
import shutil
import subprocess
import sys
import tempfile
from dataclasses import dataclass
from pathlib import Path

REPOSITORY_ROOT = Path(__file__).resolve().parent.parent


@dataclass(frozen=True, slots=True)
class CorpusProject:
    name: str
    requirements: str
    python_version: str | None = None
    install: bool = True


# Train on real project manifests and requirement snapshots that uv already
# maintains for ecosystem tests. Keep the evaluation projects separate.
TRAINING_PROJECTS = (
    CorpusProject("black", "test/ecosystem/black/pyproject.toml"),
    CorpusProject("poetry", "test/ecosystem/poetry/pyproject.toml"),
    CorpusProject("pandas", "test/ecosystem/pandas/pyproject.toml"),
    CorpusProject("packse", "test/ecosystem/packse/pyproject.toml", "3.12"),
    CorpusProject(
        "github-wikidata-bot",
        "test/ecosystem/github-wikidata-bot/pyproject.toml",
        "3.12",
    ),
    CorpusProject("flyte", "test/requirements/flyte.in", "3.11", install=False),
    CorpusProject("trio", "test/requirements/trio.in"),
    CorpusProject("uv-benchmark", "scripts/benchmark/pyproject.toml"),
)

EVALUATION_PROJECTS = (
    CorpusProject("jupyterlab", "test/ecosystem/jupyterlab/pyproject.toml"),
    CorpusProject("semantic-kernel", "test/ecosystem/semantic-kernel/pyproject.toml"),
    CorpusProject("transformers", "test/ecosystem/transformers/pyproject.toml"),
)


@dataclass(frozen=True, slots=True)
class PreparedProject:
    name: str
    project: Path
    requirements: Path
    python_version: str | None = None
    install: bool = True


@dataclass(frozen=True, slots=True)
class PreparedCorpus:
    root: Path
    projects: tuple[PreparedProject, ...]


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--target", help="Host-native Rust target triple")
    parser.add_argument(
        "--target-dir",
        type=Path,
        help="Cargo target directory (default: CARGO_TARGET_DIR or target/uv-pgo)",
    )
    parser.add_argument(
        "--profile-dir",
        type=Path,
        help="Raw profile directory (default: <target-dir>/profiles)",
    )
    parser.add_argument(
        "--llvm-profdata",
        type=Path,
        help="Override the active Rust toolchain's llvm-profdata executable",
    )
    modes = parser.add_mutually_exclusive_group()
    modes.add_argument(
        "--train-only",
        action="store_true",
        help="Only produce <target-dir>/uv.profdata for a subsequent release build",
    )
    modes.add_argument(
        "--prepare-corpus",
        action="store_true",
        help="Only prepare the real-project training corpus",
    )
    modes.add_argument(
        "--prepare-evaluation",
        action="store_true",
        help="Only prepare independent, held-out evaluation projects",
    )
    modes.add_argument(
        "--exercise-binary",
        type=Path,
        help="Run the training workloads using an existing, uninstrumented uv binary",
    )
    args = parser.parse_args()

    target_dir = Path(
        args.target_dir
        or os.environ.get("CARGO_TARGET_DIR", REPOSITORY_ROOT / "target" / "uv-pgo")
    ).resolve()
    name = "evaluation" if args.prepare_evaluation else "training"
    projects = EVALUATION_PROJECTS if args.prepare_evaluation else TRAINING_PROJECTS
    corpus = prepare_corpus(target_dir / "corpus" / name, projects)
    print(
        f"Prepared {len(corpus.projects)} real {name} projects in {corpus.root}",
        flush=True,
    )

    if args.prepare_corpus or args.prepare_evaluation:
        return

    environment = os.environ.copy()
    if args.exercise_binary is not None:
        binary = args.exercise_binary.resolve()
        launcher = binary.with_name("uvx.exe" if binary.suffix == ".exe" else "uvx")
        run_workloads(binary, launcher, corpus, environment)
        return

    host = rustc_host()
    target = args.target or host
    if target != host:
        parser.error(
            f"PGO training requires the host-native target {host}, got {target}"
        )

    profiler = find_llvm_profdata(host, args.llvm_profdata)
    profile_dir = (args.profile_dir or target_dir / "profiles").resolve()
    profile_dir.mkdir(parents=True, exist_ok=True)
    for profile in profile_dir.glob("uv-*.profraw"):
        profile.unlink()

    environment["CARGO_INCREMENTAL"] = "0"
    if target.endswith("-apple-darwin"):
        for variable in ("CFLAGS", "CXXFLAGS"):
            environment[variable] = append_flags(
                environment.get(variable), "-fno-profile-generate -fno-profile-use"
            )
    if target.endswith("-pc-windows-msvc") and "+crt-static" not in environment.get(
        "RUSTFLAGS", ""
    ):
        environment["RUSTFLAGS"] = append_flags(
            environment.get("RUSTFLAGS"), "-C target-feature=+crt-static"
        )

    instrumented_target_dir = target_dir / "instrumented"
    instrumented_environment = environment | {
        "CARGO_TARGET_DIR": str(instrumented_target_dir),
        "RUSTFLAGS": append_flags(
            environment.get("RUSTFLAGS"), f"-Cprofile-generate={profile_dir}"
        ),
    }
    print("Building instrumented release uv and uvx", flush=True)
    run(cargo_command(target), environment=instrumented_environment)

    binary_directory = instrumented_target_dir / target / "release"
    executable_suffix = ".exe" if "windows" in target else ""
    binary = binary_directory / f"uv{executable_suffix}"
    launcher = binary_directory / f"uvx{executable_suffix}"
    if not binary.is_file() or not launcher.is_file():
        raise RuntimeError(
            f"Instrumented uv or uvx binary missing from {binary_directory}"
        )

    profiles, workload_count = train_uv(
        binary,
        launcher,
        corpus,
        profile_dir,
        environment=instrumented_environment,
    )
    merged_profile = target_dir / "uv.profdata"
    merge_profiles(
        profiler,
        profiles,
        merged_profile,
        workload_count=workload_count,
        environment=environment,
    )
    if args.train_only:
        return

    optimized_environment = environment | {
        "CARGO_TARGET_DIR": str(target_dir),
        "RUSTFLAGS": append_flags(
            environment.get("RUSTFLAGS"), f"-Cprofile-use={merged_profile}"
        ),
    }
    print("Building optimized release uv and uvx", flush=True)
    run(cargo_command(target), environment=optimized_environment)
    print(f"Optimized uv: {target_dir / target / 'release' / binary.name}", flush=True)


def train_uv(
    binary: Path,
    launcher: Path,
    corpus: PreparedCorpus,
    profile_directory: Path,
    *,
    environment: dict[str, str],
) -> tuple[list[Path], int]:
    labels = run_workloads(
        binary,
        launcher,
        corpus,
        environment,
        profile_dir=profile_directory,
    )
    profiles = sorted(profile_directory.glob("uv-*.profraw"))
    for group in {profile_group(label) for label in labels}:
        workload_profiles = list(profile_directory.glob(f"uv-{group}-*.profraw"))
        if not workload_profiles or any(
            profile.stat().st_size == 0 for profile in workload_profiles
        ):
            raise RuntimeError(f"No complete uv profiling data for workload {group!r}")
    if not profiles:
        raise RuntimeError(f"No uv profiling data found in {profile_directory}")
    return profiles, len(labels)


def merge_profiles(
    profiler: Path,
    profiles: list[Path],
    destination: Path,
    *,
    workload_count: int,
    environment: dict[str, str],
) -> None:
    profile_size = sum(profile.stat().st_size for profile in profiles)
    with tempfile.NamedTemporaryFile(
        dir=destination.parent, prefix="uv-", suffix=".profdata", delete=False
    ) as temporary_file:
        temporary_profile = Path(temporary_file.name)
    try:
        run(
            [
                str(profiler),
                "merge",
                "--output",
                str(temporary_profile),
                *map(str, profiles),
            ],
            environment=environment,
        )
        temporary_profile.replace(destination)
    finally:
        temporary_profile.unlink(missing_ok=True)

    print(
        f"Merged {len(profiles)} PGO profiles ({profile_size:,} bytes) "
        f"from {workload_count} workloads: {destination}",
        flush=True,
    )


def prepare_corpus(
    root: Path, definitions: tuple[CorpusProject, ...]
) -> PreparedCorpus:
    root.mkdir(parents=True, exist_ok=True)

    # Remove artifacts produced by older generated-wheel training corpora.
    for stale in (
        "cache",
        "ecosystem",
        "graphs",
        "installed",
        "project",
        "wheelhouse",
    ):
        shutil.rmtree(root / stale, ignore_errors=True)

    projects: list[PreparedProject] = []
    for definition in definitions:
        source = REPOSITORY_ROOT / definition.requirements
        if not source.is_file():
            raise RuntimeError(f"Real project requirements not found: {source}")

        project = root / definition.name
        project.mkdir(parents=True, exist_ok=True)
        shutil.rmtree(project / ".venv", ignore_errors=True)
        shutil.rmtree(project / "installed", ignore_errors=True)
        (project / "uv.lock").unlink(missing_ok=True)

        requirements = project / source.name
        shutil.copyfile(source, requirements)
        projects.append(
            PreparedProject(
                definition.name,
                project,
                requirements,
                definition.python_version,
                definition.install,
            )
        )

    return PreparedCorpus(root, tuple(projects))


def run_workloads(
    binary: Path,
    launcher: Path,
    corpus: PreparedCorpus,
    environment: dict[str, str],
    profile_dir: Path | None = None,
) -> tuple[str, ...]:
    if not binary.is_file():
        raise RuntimeError(f"uv binary not found: {binary}")

    training_environment = environment.copy()
    training_environment.pop("UV_OFFLINE", None)
    training_environment.update(
        {
            "UV_CACHE_DIR": str(corpus.root / "cache" / "poetry"),
            "UV_NO_PROGRESS": "1",
            "UV_PYTHON_DOWNLOADS": "never",
        }
    )
    python = ["--python", sys.executable]
    commands: list[tuple[str, list[str]]] = []

    for project in corpus.projects:
        version = (
            ["--python-version", project.python_version]
            if project.python_version is not None
            else []
        )
        command = [
            str(binary),
            "pip",
            "compile",
            str(project.requirements),
            *python,
            *version,
            "--no-build",
            "--quiet",
            "--output-file",
            str(project.project / "requirements.txt"),
        ]
        commands.extend(
            (
                (f"resolve-cold-{project.name}", command),
                (f"resolve-warm-{project.name}", command),
            )
        )

    primary = next(
        (project for project in corpus.projects if project.name == "poetry"),
        None,
    )
    if primary is None:
        raise RuntimeError("The training corpus must include the Poetry project")

    commands.extend(
        [
            (
                "universal",
                [
                    str(binary),
                    "pip",
                    "compile",
                    str(primary.requirements),
                    *python,
                    "--universal",
                    "--no-build",
                    "--quiet",
                    "--output-file",
                    str(primary.project / "universal-requirements.txt"),
                ],
            ),
            (
                "lock",
                [
                    str(binary),
                    "lock",
                    "--project",
                    str(primary.project),
                    *python,
                    "--no-build",
                    "--quiet",
                ],
            ),
            (
                "export",
                [
                    str(binary),
                    "export",
                    "--project",
                    str(primary.project),
                    "--frozen",
                    "--no-emit-project",
                    "--no-hashes",
                    "--quiet",
                    "--output-file",
                    str(primary.project / "exported-requirements.txt"),
                ],
            ),
        ]
    )

    for project in corpus.projects:
        if project.python_version is not None or not project.install:
            continue
        commands.append(
            (
                f"install-{project.name}",
                [
                    str(binary),
                    "pip",
                    "install",
                    *python,
                    "--target",
                    str(project.project / "installed"),
                    "--requirements",
                    str(project.project / "requirements.txt"),
                    "--no-build",
                    "--quiet",
                ],
            )
        )

    commands.append(
        (
            "sync",
            [
                str(binary),
                "sync",
                "--project",
                str(primary.project),
                "--frozen",
                *python,
                "--no-install-project",
                "--no-build",
                "--quiet",
            ],
        )
    )

    if launcher.is_file():
        commands.append(("launcher", [str(launcher), "--version"]))

    for label, command in commands:
        workload_environment = training_environment.copy()
        for prefix in ("resolve-cold-", "resolve-warm-", "install-"):
            if label.startswith(prefix):
                project = label.removeprefix(prefix)
                workload_environment["UV_CACHE_DIR"] = str(
                    corpus.root / "cache" / project
                )
                break
        if profile_dir is not None:
            group = profile_group(label)
            suffix = (
                "%4m"
                if group in {"resolve-cold", "resolve-warm", "install"}
                else "%m-%p"
            )
            workload_environment["LLVM_PROFILE_FILE"] = str(
                profile_dir / f"uv-{group}-{suffix}.profraw"
            )
        print(f"Training uv workload: {label}", flush=True)
        run(command, environment=workload_environment)

    return tuple(label for label, _ in commands)


def profile_group(label: str) -> str:
    if label.startswith("resolve-cold-"):
        return "resolve-cold"
    if label.startswith("resolve-warm-"):
        return "resolve-warm"
    if label.startswith("install-"):
        return "install"
    return label


def cargo_command(target: str) -> list[str]:
    windows = target.endswith("-pc-windows-msvc")
    return [
        os.environ.get("CARGO", "cargo"),
        "build",
        "--package",
        "uv",
        "--bin",
        "uv",
        "--bin",
        "uvx",
        *(("--bin", "uvw") if windows else ()),
        "--release",
        "--locked",
        "--features",
        "self-update,windows-gui-bin" if windows else "self-update",
        "--target",
        target,
    ]


def rustc_host() -> str:
    version = subprocess.run(
        ["rustc", "--version", "--verbose"],
        cwd=REPOSITORY_ROOT,
        check=True,
        capture_output=True,
        text=True,
    ).stdout
    for line in version.splitlines():
        if line.startswith("host: "):
            return line.removeprefix("host: ")
    raise RuntimeError("Could not determine the active Rust compiler's host target")


def find_llvm_profdata(host: str, override: Path | None) -> Path:
    if override is not None:
        profiler = override.resolve()
    else:
        sysroot = subprocess.run(
            ["rustc", "--print", "sysroot"],
            cwd=REPOSITORY_ROOT,
            check=True,
            capture_output=True,
            text=True,
        ).stdout.strip()
        binary = "llvm-profdata.exe" if "windows" in host else "llvm-profdata"
        profiler = Path(sysroot) / "lib" / "rustlib" / host / "bin" / binary

    if not profiler.is_file() or not os.access(profiler, os.X_OK):
        raise RuntimeError(
            f"llvm-profdata not found at {profiler}; install llvm-tools-preview"
        )
    return profiler


def append_flags(existing: str | None, addition: str) -> str:
    return " ".join(flag for flag in (existing, addition) if flag)


def run(command: list[str], *, environment: dict[str, str]) -> None:
    print(f"> {shlex.join(command)}", flush=True)
    subprocess.run(command, cwd=REPOSITORY_ROOT, env=environment, check=True)


if __name__ == "__main__":
    try:
        main()
    except (OSError, RuntimeError, subprocess.CalledProcessError) as error:
        print(f"error: {error}", file=sys.stderr)
        raise SystemExit(1) from error
