"""Build uv with profile-guided optimization using pinned ecosystem projects."""

# /// script
# requires-python = ">=3.11"
# dependencies = []
# ///

from __future__ import annotations

import argparse
import os
import re
import shlex
import shutil
import subprocess
import sys
import tempfile
from dataclasses import dataclass
from pathlib import Path

REPOSITORY_ROOT = Path(__file__).resolve().parent.parent
DEPENDENCY_EXCLUDE_NEWER = "2026-06-30T00:00:00Z"


@dataclass(frozen=True, slots=True)
class EcosystemProject:
    name: str
    python_version: str
    constraints: tuple[str, ...] = ()
    groups: tuple[str, ...] = ()
    native_platforms: tuple[str, ...] | None = None

    def __post_init__(self) -> None:
        if re.fullmatch(r"3\.\d+", self.python_version) is None:
            raise ValueError(
                f"{self.name} must specify a Python major.minor version, "
                f"got {self.python_version!r}"
            )

    @property
    def supports_native_workloads(self) -> bool:
        return self.native_platforms is None or sys.platform in self.native_platforms

    @property
    def python_arguments(self) -> tuple[str, str]:
        current = f"{sys.version_info.major}.{sys.version_info.minor}"
        python = (
            sys.executable if self.python_version == current else self.python_version
        )
        return "--python", python

    @property
    def group_arguments(self) -> tuple[str, ...]:
        return tuple(
            argument for group in self.groups for argument in ("--group", group)
        )


# Train on existing ecosystem fixtures covering applications, workspaces,
# libraries, and packaging tools across their supported Python versions.
CORPUS_PROJECTS = (
    EcosystemProject(name="cibuildwheel", python_version="3.11"),
    EcosystemProject(name="cookiecutter", python_version="3.12"),
    EcosystemProject(name="flask", python_version="3.11"),
    EcosystemProject(name="httpx", python_version="3.12"),
    EcosystemProject(
        name="llm",
        python_version="3.12",
        constraints=("numpy==2.2.6",),
    ),
    EcosystemProject(name="openai-python", python_version="3.13"),
    EcosystemProject(
        name="poetry",
        python_version="3.11",
        constraints=("rapidfuzz==3.9.6",),
    ),
    EcosystemProject(name="pytest-cov", python_version="3.14"),
    EcosystemProject(
        name="sentry",
        python_version="3.13",
        native_platforms=("linux",),
    ),
    EcosystemProject(
        name="zulip",
        python_version="3.12",
        groups=("dev",),
        native_platforms=("win32",),
    ),
    EcosystemProject(
        name="pyx-workspace",
        python_version="3.14",
        groups=("dev",),
    ),
)


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--target", help="Host-native Rust target triple")
    parser.add_argument(
        "--debug",
        action="store_true",
        help="Use debug builds to validate the complete PGO pipeline",
    )
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
    mode = parser.add_mutually_exclusive_group()
    mode.add_argument(
        "--train-only",
        action="store_true",
        help="Only produce <target-dir>/uv.profdata for a subsequent release build",
    )
    mode.add_argument(
        "--prepare-corpus",
        action="store_true",
        help="Only prepare the pinned ecosystem training corpus",
    )
    args = parser.parse_args()

    target_dir = (
        args.target_dir
        or Path(
            os.environ.get("CARGO_TARGET_DIR", REPOSITORY_ROOT / "target" / "uv-pgo")
        )
    ).resolve()
    corpus_directory = target_dir / "corpus"
    profile_dir = (args.profile_dir or target_dir / "profiles").resolve()
    merged_profile = target_dir / "uv.profdata"

    environment = os.environ.copy()
    if args.prepare_corpus:
        prepare_corpus(corpus_directory)
        print(f"Prepared {len(CORPUS_PROJECTS)} ecosystem projects", flush=True)
        return

    host = rustc_host()
    target = args.target or host
    if target != host:
        parser.error(
            f"PGO training requires the host-native target {host}, got {target}"
        )

    profiler = find_llvm_profdata(host, args.llvm_profdata)
    prepare_corpus(corpus_directory)
    print(f"Prepared {len(CORPUS_PROJECTS)} ecosystem projects", flush=True)

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
    profile = "debug" if args.debug else "release"
    print(f"Building instrumented {profile} uv and uvx", flush=True)
    run(cargo_command(target, debug=args.debug), environment=instrumented_environment)

    binary_directory = instrumented_target_dir / target / profile
    executable_suffix = ".exe" if "windows" in target else ""
    binary = binary_directory / f"uv{executable_suffix}"
    launcher = binary_directory / f"uvx{executable_suffix}"
    if not binary.is_file() or not launcher.is_file():
        raise RuntimeError(
            f"Instrumented uv or uvx binary missing from {binary_directory}"
        )

    profiles = train_uv(
        binary,
        launcher,
        corpus_directory,
        profile_dir,
        environment=instrumented_environment,
    )
    merge_profiles(profiler, profiles, merged_profile, environment=environment)
    if args.train_only:
        return

    optimized_environment = environment | {
        "CARGO_TARGET_DIR": str(target_dir),
        "RUSTFLAGS": append_flags(
            environment.get("RUSTFLAGS"), f"-Cprofile-use={merged_profile}"
        ),
    }
    print(f"Building profile-guided {profile} uv and uvx", flush=True)
    run(cargo_command(target, debug=args.debug), environment=optimized_environment)
    print(
        f"Profile-guided uv: {target_dir / target / profile / binary.name}", flush=True
    )


def train_uv(
    binary: Path,
    launcher: Path,
    corpus_directory: Path,
    profile_directory: Path,
    *,
    environment: dict[str, str],
) -> list[Path]:
    labels = run_workloads(
        binary,
        launcher,
        corpus_directory,
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
    print(f"Profiled {len(labels)} uv ecosystem workloads", flush=True)
    return profiles


def merge_profiles(
    profiler: Path,
    profiles: list[Path],
    destination: Path,
    *,
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
        f"Merged {len(profiles)} PGO profiles ({profile_size:,} bytes): {destination}",
        flush=True,
    )


def prepare_corpus(corpus_directory: Path) -> None:
    corpus_directory.mkdir(parents=True, exist_ok=True)

    for project in CORPUS_PROJECTS:
        source = (
            REPOSITORY_ROOT / "test" / "ecosystem" / project.name / "pyproject.toml"
        )
        if not source.is_file():
            raise RuntimeError(f"Real project manifest not found: {source}")

        destination = corpus_directory / project.name
        shutil.rmtree(destination, ignore_errors=True)
        shutil.copytree(
            source.parent,
            destination,
            ignore=shutil.ignore_patterns(".venv", "installed", "uv.lock"),
        )


def run_workloads(
    binary: Path,
    launcher: Path,
    corpus_directory: Path,
    environment: dict[str, str],
    profile_dir: Path | None = None,
) -> tuple[str, ...]:
    if not binary.is_file():
        raise RuntimeError(f"uv binary not found: {binary}")

    training_environment = environment.copy()
    training_environment.pop("UV_OFFLINE", None)
    training_environment.update(
        {
            "UV_CACHE_DIR": str(corpus_directory / "cache"),
            "UV_EXCLUDE_NEWER": DEPENDENCY_EXCLUDE_NEWER,
            "UV_NO_PROGRESS": "1",
            "UV_PYTHON_INSTALL_DIR": str(corpus_directory / "python"),
            "UV_PYTHON_DOWNLOADS": "never",
        }
    )
    labels = prepare_pythons(binary, training_environment, profile_dir)
    commands: list[tuple[str, list[str]]] = []

    for project in CORPUS_PROJECTS:
        if not project.supports_native_workloads:
            continue

        project_directory = corpus_directory / project.name
        command = [
            str(binary),
            "pip",
            "compile",
            str(project_directory / "pyproject.toml"),
            "--project",
            str(project_directory),
            *project.group_arguments,
            *project.python_arguments,
            "--no-build",
            "--quiet",
            "--output-file",
            str(project_directory / "requirements.txt"),
        ]
        commands.extend(
            (
                (f"resolve-cold-{project.name}", command),
                (f"resolve-warm-{project.name}", command),
            )
        )

    for project in CORPUS_PROJECTS:
        project_directory = corpus_directory / project.name
        constraints = [
            argument
            for constraint in project.constraints
            for argument in ("--upgrade-package", constraint)
        ]
        commands.extend(
            (
                (
                    f"universal-{project.name}",
                    [
                        str(binary),
                        "pip",
                        "compile",
                        str(project_directory / "pyproject.toml"),
                        "--project",
                        str(project_directory),
                        *project.group_arguments,
                        *project.python_arguments,
                        "--universal",
                        "--no-build",
                        "--quiet",
                        "--output-file",
                        str(project_directory / "universal-requirements.txt"),
                    ],
                ),
                (
                    f"lock-{project.name}",
                    [
                        str(binary),
                        "lock",
                        "--project",
                        str(project_directory),
                        *project.python_arguments,
                        *constraints,
                        "--no-build",
                        "--quiet",
                    ],
                ),
                (
                    f"export-{project.name}",
                    [
                        str(binary),
                        "export",
                        "--project",
                        str(project_directory),
                        *project.python_arguments,
                        *project.group_arguments,
                        "--frozen",
                        "--no-emit-workspace",
                        "--no-hashes",
                        "--quiet",
                        "--output-file",
                        str(project_directory / "exported-requirements.txt"),
                    ],
                ),
            )
        )

        if project.supports_native_workloads:
            commands.extend(
                (
                    (
                        f"install-{project.name}",
                        [
                            str(binary),
                            "pip",
                            "install",
                            *project.python_arguments,
                            "--target",
                            str(project_directory / "installed"),
                            "--requirements",
                            str(project_directory / "exported-requirements.txt"),
                            "--no-build",
                            "--quiet",
                        ],
                    ),
                    (
                        f"sync-{project.name}",
                        [
                            str(binary),
                            "sync",
                            "--project",
                            str(project_directory),
                            "--frozen",
                            *project.python_arguments,
                            *project.group_arguments,
                            "--no-install-workspace",
                            "--no-build",
                            "--quiet",
                        ],
                    ),
                )
            )

    if launcher.is_file():
        commands.append(("launcher", [str(launcher), "--version"]))

    for label, command in commands:
        workload_environment = training_environment.copy()
        group = profile_group(label)
        if label != group:
            project = label.removeprefix(f"{group}-")
            workload_environment["UV_CACHE_DIR"] = str(
                corpus_directory / "cache" / project
            )
        if profile_dir is not None:
            workload_environment["LLVM_PROFILE_FILE"] = str(
                profile_dir / f"uv-{group}-%m.profraw"
            )
        print(f"Training uv workload: {label}", flush=True)
        run(command, environment=workload_environment)

    return (*labels, *(label for label, _ in commands))


def prepare_pythons(
    binary: Path,
    environment: dict[str, str],
    profile_dir: Path | None,
) -> tuple[str, ...]:
    current = f"{sys.version_info.major}.{sys.version_info.minor}"
    versions = sorted(
        {project.python_version for project in CORPUS_PROJECTS} - {current}
    )
    if not versions:
        return ()

    install_environment = environment.copy()
    install_environment.pop("UV_PYTHON_DOWNLOADS", None)
    if profile_dir is not None:
        install_environment["LLVM_PROFILE_FILE"] = str(
            profile_dir / "uv-python-install-%m.profraw"
        )

    print(f"Preparing training Python versions: {', '.join(versions)}", flush=True)
    run(
        [str(binary), "python", "install", "--no-bin", "--no-registry", *versions],
        environment=install_environment,
    )
    return ("python-install",)


def profile_group(label: str) -> str:
    for group in (
        "resolve-cold",
        "resolve-warm",
        "universal",
        "lock",
        "export",
        "install",
        "sync",
    ):
        if label.startswith(f"{group}-"):
            return group
    return label


def cargo_command(target: str, *, debug: bool = False) -> list[str]:
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
        *(() if debug else ("--release",)),
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


def append_flags(existing: str | None, additional: str) -> str:
    return " ".join(flag for flag in (existing, additional) if flag)


def run(
    command: list[str],
    *,
    environment: dict[str, str],
    allowed_exit_codes: tuple[int, ...] = (0,),
) -> None:
    logged_arguments = 16
    displayed_command = shlex.join(command[:logged_arguments])
    if len(command) > logged_arguments:
        displayed_command += (
            f" ... ({len(command) - logged_arguments} arguments omitted)"
        )
    print(f"> {displayed_command}", flush=True)
    completed = subprocess.run(
        command, cwd=REPOSITORY_ROOT, env=environment, check=False
    )
    if completed.returncode not in allowed_exit_codes:
        raise subprocess.CalledProcessError(completed.returncode, command)


if __name__ == "__main__":
    try:
        main()
    except (OSError, RuntimeError, subprocess.CalledProcessError) as error:
        print(f"error: {error}", file=sys.stderr)
        raise SystemExit(1) from error
