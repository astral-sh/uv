"""Build uv with profile-guided optimization using offline release workloads."""

from __future__ import annotations

import argparse
import os
import shutil
import subprocess
import sys
import tempfile
import zipfile
from dataclasses import dataclass
from pathlib import Path

REPOSITORY_ROOT = Path(__file__).resolve().parent.parent
SNAPSHOT_DIRECTORY = REPOSITORY_ROOT / "crates" / "uv" / "tests" / "it" / "snapshots"


@dataclass(frozen=True)
class CorpusDefinition:
    name: str
    prefix: str
    families: tuple[str, ...]
    family_size: int
    shared_size: int
    versions: tuple[int, ...]
    ecosystem_projects: tuple[str, ...]


TRAINING_CORPUS = CorpusDefinition(
    name="training",
    prefix="pgo",
    families=("web", "data", "tools"),
    family_size=18,
    shared_size=12,
    versions=(1, 2, 3),
    ecosystem_projects=(
        "packse",
        "github-wikidata-bot",
        "poetry",
        "saleor",
        "black",
    ),
)

EVALUATION_CORPUS = CorpusDefinition(
    name="evaluation",
    prefix="heldout",
    families=("science", "workflow", "notebook", "analytics"),
    family_size=14,
    shared_size=16,
    versions=(1, 2, 3, 4),
    ecosystem_projects=("jupyterlab", "semantic-kernel", "transformers"),
)


@dataclass(frozen=True)
class PreparedCorpus:
    root: Path
    project: Path
    requirements: Path
    wheelhouse: Path
    ecosystem_projects: tuple[Path, ...]
    wheel_count: int


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
    parser.add_argument(
        "--train-only",
        action="store_true",
        help="Only produce <target-dir>/uv.profdata for a subsequent release build",
    )
    modes = parser.add_mutually_exclusive_group()
    modes.add_argument(
        "--prepare-corpus",
        action="store_true",
        help="Only prepare the deterministic offline training corpus",
    )
    modes.add_argument(
        "--prepare-evaluation",
        action="store_true",
        help="Only prepare independent, held-out offline evaluation workloads",
    )
    modes.add_argument(
        "--exercise-binary",
        type=Path,
        help="Run the training workloads using an existing, uninstrumented uv binary",
    )
    args = parser.parse_args()

    if args.train_only and (
        args.prepare_corpus or args.prepare_evaluation or args.exercise_binary
    ):
        parser.error("--train-only cannot be combined with another execution mode")

    target_dir = Path(
        args.target_dir
        or os.environ.get("CARGO_TARGET_DIR", REPOSITORY_ROOT / "target" / "uv-pgo")
    ).resolve()
    definition = EVALUATION_CORPUS if args.prepare_evaluation else TRAINING_CORPUS
    corpus = prepare_corpus(target_dir / "corpus", definition)
    print(
        f"Prepared {corpus.wheel_count} offline wheels and "
        f"{len(corpus.ecosystem_projects)} {definition.name} ecosystem projects "
        f"in {corpus.root}",
        flush=True,
    )

    if args.prepare_corpus or args.prepare_evaluation:
        return

    environment = os.environ.copy()
    if args.exercise_binary is not None:
        binary = args.exercise_binary.resolve()
        run_workloads(binary, binary.with_name("uvx"), corpus, environment)
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

    instrumented_target_dir = target_dir / "instrumented"
    instrumented_environment = environment.copy()
    instrumented_environment.update(
        {
            "CARGO_INCREMENTAL": "0",
            "CARGO_TARGET_DIR": str(instrumented_target_dir),
            "RUSTFLAGS": append_flags(
                environment.get("RUSTFLAGS"), f"-Cprofile-generate={profile_dir}"
            ),
        }
    )
    print("Building instrumented release uv and uvx", flush=True)
    run(cargo_command(target), environment=instrumented_environment)

    binary_directory = instrumented_target_dir / target / "release"
    binary = binary_directory / "uv"
    launcher = binary_directory / "uvx"
    if not binary.is_file() or not launcher.is_file():
        raise RuntimeError(
            f"Instrumented uv or uvx binary missing from {binary_directory}"
        )

    labels = run_workloads(
        binary,
        launcher,
        corpus,
        instrumented_environment,
        profile_dir=profile_dir,
    )
    profiles = sorted(profile_dir.glob("uv-*.profraw"))
    for label in labels:
        if not any(profile_dir.glob(f"uv-{label}-*.profraw")):
            raise RuntimeError(f"No uv profiling data found for workload {label!r}")
    if not profiles or any(profile.stat().st_size == 0 for profile in profiles):
        raise RuntimeError(f"No complete uv profiling data found in {profile_dir}")

    merged_profile = target_dir / "uv.profdata"
    with tempfile.NamedTemporaryFile(
        dir=target_dir,
        prefix="uv-",
        suffix=".profdata",
        delete=False,
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
        temporary_profile.replace(merged_profile)
    finally:
        temporary_profile.unlink(missing_ok=True)

    print(
        f"Merged {len(profiles)} profiles from {len(labels)} workloads: {merged_profile}",
        flush=True,
    )
    if args.train_only:
        return

    optimized_environment = environment.copy()
    optimized_environment.update(
        {
            "CARGO_INCREMENTAL": "0",
            "CARGO_TARGET_DIR": str(target_dir),
            "RUSTFLAGS": append_flags(
                environment.get("RUSTFLAGS"), f"-Cprofile-use={merged_profile}"
            ),
        }
    )
    print("Building optimized release uv and uvx", flush=True)
    run(cargo_command(target), environment=optimized_environment)
    print(f"Optimized uv: {target_dir / target / 'release' / 'uv'}", flush=True)


def prepare_corpus(root: Path, definition: CorpusDefinition) -> PreparedCorpus:
    corpus_root = root / definition.name
    wheelhouse = corpus_root / "wheelhouse"
    project = corpus_root / "project"
    ecosystem = corpus_root / "ecosystem"
    for directory in (wheelhouse, project, ecosystem):
        directory.mkdir(parents=True, exist_ok=True)

    for wheel in wheelhouse.glob("*.whl"):
        wheel.unlink()

    upper_bound = max(definition.versions) + 1
    for index in range(definition.shared_size):
        name = f"{definition.prefix}-shared-{index:02}"
        for version in definition.versions:
            write_wheel(wheelhouse, name, version, [])

    for family_index, family in enumerate(definition.families):
        for index in range(definition.family_size):
            name = f"{definition.prefix}-{family}-{index:02}"
            for version in definition.versions:
                requirements = family_requirements(
                    definition,
                    family_index,
                    family,
                    index,
                    version,
                    upper_bound,
                )
                write_wheel(wheelhouse, name, version, requirements)

    requirements = corpus_root / "requirements.in"
    root_requirements = [
        f"{definition.prefix}-{family}-00[speed]>=1,<{upper_bound}"
        for family in definition.families
    ]
    requirements.write_text("\n".join(root_requirements) + "\n", encoding="utf-8")

    quoted_requirements = ",\n    ".join(
        f'"{requirement}"' for requirement in root_requirements
    )
    extra = f"{definition.prefix}-{definition.families[-1]}-05[speed]>=1,<{upper_bound}"
    developer = f"{definition.prefix}-{definition.families[0]}-08>=1,<{upper_bound}"
    pyproject = (
        "[project]\n"
        f'name = "{definition.prefix}-application"\n'
        'version = "1.0.0"\n'
        'requires-python = ">=3.9"\n'
        f"dependencies = [\n    {quoted_requirements},\n]\n\n"
        "[project.optional-dependencies]\n"
        f'analysis = ["{extra}"]\n\n'
        "[dependency-groups]\n"
        f'dev = ["{developer}"]\n\n'
        "[tool.uv]\n"
        "package = false\n"
    )
    (project / "pyproject.toml").write_text(pyproject, encoding="utf-8")

    ecosystem_projects = tuple(
        prepare_ecosystem_project(ecosystem, name)
        for name in definition.ecosystem_projects
    )
    wheel_count = len(definition.versions) * (
        definition.shared_size + len(definition.families) * definition.family_size
    )
    return PreparedCorpus(
        root=corpus_root,
        project=project,
        requirements=requirements,
        wheelhouse=wheelhouse,
        ecosystem_projects=ecosystem_projects,
        wheel_count=wheel_count,
    )


def family_requirements(
    definition: CorpusDefinition,
    family_index: int,
    family: str,
    index: int,
    version: int,
    upper_bound: int,
) -> list[str]:
    prefix = definition.prefix
    requirements = [
        f"{prefix}-shared-{index % definition.shared_size:02}>=1,<{upper_bound}"
    ]
    if index + 1 < definition.family_size:
        requirements.append(f"{prefix}-{family}-{index + 1:02}>=1,<{upper_bound}")
    if index + 3 < definition.family_size and index % 3 == 0:
        requirements.append(
            f"{prefix}-{family}-{index + 3:02}>=1,<{upper_bound}; "
            'python_version >= "3.10"'
        )
    if index % 5 == 0:
        marker_index = (index + 2) % definition.shared_size
        requirements.extend(
            [
                f'{prefix}-shared-{marker_index:02}>=2; python_version < "3.12"',
                (
                    f"{prefix}-shared-{(index + 3) % definition.shared_size:02}>=1; "
                    'sys_platform == "win32"'
                ),
                (
                    f"{prefix}-shared-{(index + 4) % definition.shared_size:02}>=2; "
                    'extra == "speed"'
                ),
            ]
        )

    # The newest candidates from neighboring families disagree on the same shared
    # package, forcing the resolver to backtrack before finding compatible versions.
    if index % 6 == 0 and version == max(definition.versions):
        shared = f"{prefix}-shared-{index % definition.shared_size:02}"
        if family_index % 2 == 0:
            requirements.append(f"{shared}>={max(definition.versions)}")
        else:
            requirements.append(f"{shared}<{max(definition.versions)}")
    return requirements


def write_wheel(
    wheelhouse: Path,
    name: str,
    version: int,
    requirements: list[str],
) -> None:
    normalized = name.replace("-", "_")
    release = f"{version}.0.0"
    distribution = f"{normalized}-{release}.dist-info"
    metadata = [
        "Metadata-Version: 2.3",
        f"Name: {name}",
        f"Version: {release}",
        "Requires-Python: >=3.9",
        "Provides-Extra: speed",
        *(f"Requires-Dist: {requirement}" for requirement in requirements),
        "",
    ]
    wheel = (
        "Wheel-Version: 1.0\n"
        "Generator: uv PGO corpus\n"
        "Root-Is-Purelib: true\n"
        "Tag: py3-none-any\n"
    )
    paths = [
        f"{normalized}.py",
        f"{distribution}/METADATA",
        f"{distribution}/WHEEL",
        f"{distribution}/RECORD",
    ]
    record = "".join(f"{path},,\n" for path in paths)
    contents = [
        f'NAME = "{name}"\nVERSION = "{release}"\n',
        "\n".join(metadata),
        wheel,
        record,
    ]
    destination = wheelhouse / f"{normalized}-{release}-py3-none-any.whl"
    with zipfile.ZipFile(destination, "w", compression=zipfile.ZIP_STORED) as archive:
        for path, content in zip(paths, contents, strict=True):
            info = zipfile.ZipInfo(path, date_time=(1980, 1, 1, 0, 0, 0))
            info.external_attr = 0o644 << 16
            archive.writestr(info, content)


def prepare_ecosystem_project(root: Path, name: str) -> Path:
    source = REPOSITORY_ROOT / "test" / "ecosystem" / name / "pyproject.toml"
    snapshot = SNAPSHOT_DIRECTORY / f"it__ecosystem__{name}-lock-file.snap"
    project = root / name
    project.mkdir(parents=True, exist_ok=True)
    shutil.copyfile(source, project / "pyproject.toml")

    contents = snapshot.read_text(encoding="utf-8")
    header, separator, lockfile = contents.partition("\n---\n")
    if not separator or not header.startswith("---\n"):
        raise RuntimeError(f"Could not locate the Insta metadata header in {snapshot}")
    (project / "uv.lock").write_text(lockfile, encoding="utf-8")
    return project


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
    training_environment.update(
        {
            "UV_CACHE_DIR": str(corpus.root / "cache"),
            "UV_NO_PROGRESS": "1",
            "UV_OFFLINE": "1",
            "UV_PYTHON_DOWNLOADS": "never",
        }
    )
    common = ["--offline", "--no-index", "--find-links", str(corpus.wheelhouse)]
    python = ["--python", sys.executable]
    project = ["--project", str(corpus.project)]

    commands: list[tuple[str, list[str]]] = [
        (
            "resolve",
            [
                str(binary),
                "pip",
                "compile",
                str(corpus.requirements),
                *common,
                *python,
                "--quiet",
                "--output-file",
                str(corpus.root / "requirements.txt"),
            ],
        ),
        (
            "universal",
            [
                str(binary),
                "pip",
                "compile",
                str(corpus.requirements),
                *common,
                *python,
                "--universal",
                "--quiet",
                "--output-file",
                str(corpus.root / "universal-requirements.txt"),
            ],
        ),
        (
            "lock",
            [str(binary), "lock", *common, *python, *project, "--quiet"],
        ),
        (
            "export",
            [
                str(binary),
                "export",
                "--frozen",
                "--offline",
                "--all-groups",
                "--all-extras",
                "--no-emit-project",
                "--no-hashes",
                "--quiet",
                *project,
                "--output-file",
                str(corpus.root / "project-requirements.txt"),
            ],
        ),
        (
            "install",
            [
                str(binary),
                "pip",
                "install",
                *common,
                *python,
                "--target",
                str(corpus.root / "installed"),
                "--requirements",
                str(corpus.root / "requirements.txt"),
                "--quiet",
            ],
        ),
        (
            "sync",
            [
                str(binary),
                "sync",
                "--frozen",
                *common,
                *python,
                *project,
                "--all-groups",
                "--all-extras",
                "--no-install-project",
                "--quiet",
            ],
        ),
    ]

    for ecosystem_project in corpus.ecosystem_projects:
        commands.append(
            (
                f"export-{ecosystem_project.name}",
                [
                    str(binary),
                    "export",
                    "--frozen",
                    "--offline",
                    "--all-groups",
                    "--all-extras",
                    "--no-emit-project",
                    "--no-hashes",
                    "--no-header",
                    "--quiet",
                    "--project",
                    str(ecosystem_project),
                    "--output-file",
                    str(ecosystem_project / "requirements.txt"),
                ],
            )
        )

    if launcher.is_file():
        commands.append(("launcher", [str(launcher), "--version"]))

    for label, command in commands:
        workload_environment = training_environment.copy()
        if profile_dir is not None:
            workload_environment["LLVM_PROFILE_FILE"] = str(
                profile_dir / f"uv-{label}-%m-%p.profraw"
            )
        print(f"Training uv workload: {label}", flush=True)
        run(command, environment=workload_environment)

    return tuple(label for label, _ in commands)


def cargo_command(target: str) -> list[str]:
    return [
        os.environ.get("CARGO", "cargo"),
        "build",
        "--package",
        "uv",
        "--bin",
        "uv",
        "--bin",
        "uvx",
        "--release",
        "--locked",
        "--features",
        "self-update",
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
        profiler = Path(sysroot) / "lib" / "rustlib" / host / "bin" / "llvm-profdata"

    if not profiler.is_file() or not os.access(profiler, os.X_OK):
        raise RuntimeError(
            f"llvm-profdata not found at {profiler}; install llvm-tools-preview"
        )
    return profiler


def append_flags(existing: str | None, addition: str) -> str:
    return f"{existing} {addition}" if existing else addition


def run(command: list[str], *, environment: dict[str, str]) -> None:
    subprocess.run(command, cwd=REPOSITORY_ROOT, env=environment, check=True)


if __name__ == "__main__":
    main()
