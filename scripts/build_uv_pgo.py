"""Build uv with profile-guided optimization using offline release workloads."""

# /// script
# requires-python = ">=3.11"
# dependencies = []
# ///

from __future__ import annotations

import argparse
import os
import shlex
import shutil
import subprocess
import sys
import tempfile
import tomllib
import zipfile
from collections import defaultdict
from dataclasses import dataclass
from pathlib import Path
from typing import Any

REPOSITORY_ROOT = Path(__file__).resolve().parent.parent
SNAPSHOT_DIRECTORY = REPOSITORY_ROOT / "crates" / "uv" / "tests" / "it" / "snapshots"


@dataclass(frozen=True, slots=True)
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
    prefix="real",
    families=(),
    family_size=0,
    shared_size=0,
    versions=(),
    ecosystem_projects=(
        "packse",
        "github-wikidata-bot",
        "poetry",
        "saleor",
        "black",
        "home-assistant-core",
        "pandas",
        "pyx-external",
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


@dataclass(frozen=True, slots=True)
class LockedGraph:
    name: str
    project: Path
    wheelhouse: Path
    requirements: Path
    package_count: int


REAL_PROJECT_ROOTS = {
    "packse": ("hatchling", "pytest", "twine", "watchfiles"),
    "github-wikidata-bot": ("httpx", "pydantic", "requests", "sentry-sdk"),
    "poetry": ("keyring", "dulwich", "virtualenv", "cleo"),
    "saleor": ("django", "celery", "boto3", "graphene", "redis", "stripe"),
    "black": ("aiohttp", "ipython", "click", "uvloop"),
    "home-assistant-core": ("httpx", "sqlalchemy", "pyopenssl", "jinja2"),
    "pandas": ("aiohttp", "numpy", "scipy", "pytest"),
    "pyx-external": ("fastapi", "pydantic", "sqlalchemy", "sentry-sdk"),
    "uv": ("mkdocs", "mkdocs-material", "pydantic", "black"),
    "uv-benchmark": ("pdm", "poetry", "pip-tools", "pipx"),
}


@dataclass(frozen=True, slots=True)
class PreparedCorpus:
    root: Path
    project: Path
    requirements: Path
    wheelhouse: Path
    ecosystem_projects: tuple[Path, ...]
    wheel_count: int
    real_graphs: tuple[LockedGraph, ...] = ()


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


def prepare_corpus(root: Path, definition: CorpusDefinition) -> PreparedCorpus:
    if definition == TRAINING_CORPUS:
        return prepare_real_training_corpus(root)

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


def prepare_real_training_corpus(root: Path) -> PreparedCorpus:
    corpus_root = root / TRAINING_CORPUS.name
    ecosystem = corpus_root / "ecosystem"
    graph_root = corpus_root / "graphs"
    project = corpus_root / "project"
    for directory in (ecosystem, graph_root, project):
        directory.mkdir(parents=True, exist_ok=True)

    # Older PGO prototypes populated these directories with fictitious packages.
    # Never retain those wheels or cached distributions when reusing a target root.
    for stale in (
        corpus_root / "wheelhouse",
        corpus_root / "cache",
        corpus_root / "installed",
        project / ".venv",
    ):
        shutil.rmtree(stale, ignore_errors=True)
    (project / "uv.lock").unlink(missing_ok=True)

    known_versions = known_locked_versions()
    projects = [
        prepare_real_ecosystem_project(ecosystem, name, known_versions)
        for name in TRAINING_CORPUS.ecosystem_projects
    ]
    projects.extend(
        [
            prepare_local_lock_project(
                ecosystem,
                "uv",
                REPOSITORY_ROOT / "pyproject.toml",
                REPOSITORY_ROOT / "uv.lock",
            ),
            prepare_local_lock_project(
                ecosystem,
                "uv-benchmark",
                REPOSITORY_ROOT / "scripts" / "benchmark" / "pyproject.toml",
                REPOSITORY_ROOT / "scripts" / "benchmark" / "uv.lock",
            ),
        ]
    )

    graphs = tuple(
        prepare_locked_graph(graph_root, locked_project) for locked_project in projects
    )
    primary = next(graph for graph in graphs if graph.name == "github-wikidata-bot")
    requirements = corpus_root / "requirements.in"
    requirements.write_text(primary.requirements.read_text(), encoding="utf-8")

    quoted_requirements = ",\n    ".join(
        f'"{requirement}"' for requirement in REAL_PROJECT_ROOTS[primary.name]
    )
    (project / "pyproject.toml").write_text(
        "[project]\n"
        'name = "locked-ecosystem-training"\n'
        'version = "1.0.0"\n'
        'requires-python = ">=3.9"\n'
        f"dependencies = [\n    {quoted_requirements},\n]\n\n"
        "[tool.uv]\n"
        "package = false\n",
        encoding="utf-8",
    )

    return PreparedCorpus(
        root=corpus_root,
        project=project,
        requirements=requirements,
        wheelhouse=primary.wheelhouse,
        ecosystem_projects=tuple(projects),
        wheel_count=sum(graph.package_count for graph in graphs),
        real_graphs=graphs,
    )


def known_locked_versions() -> dict[str, str]:
    """Map immutable source hashes to exact versions from unredacted training locks."""
    paths = [
        REPOSITORY_ROOT / "uv.lock",
        REPOSITORY_ROOT / "scripts" / "benchmark" / "uv.lock",
        *(
            SNAPSHOT_DIRECTORY / f"it__ecosystem__{name}-lock-file.snap"
            for name in TRAINING_CORPUS.ecosystem_projects
        ),
    ]
    versions: dict[str, str] = {}
    for path in paths:
        contents = path.read_text(encoding="utf-8")
        if path.suffix == ".snap":
            contents = snapshot_lockfile(contents, path)
        for package in tomllib.loads(contents).get("package", []):
            version = package.get("version")
            source = package.get("sdist", {})
            digest = source.get("hash")
            if version and digest and "[X]" not in version:
                previous = versions.setdefault(digest, version)
                if previous != version:
                    raise RuntimeError(
                        f"Source hash {digest} maps to both {previous} and {version}"
                    )
    return versions


def prepare_real_ecosystem_project(
    root: Path,
    name: str,
    known_versions: dict[str, str],
) -> Path:
    manifest = REPOSITORY_ROOT / "test" / "ecosystem" / name / "pyproject.toml"
    snapshot = SNAPSHOT_DIRECTORY / f"it__ecosystem__{name}-lock-file.snap"
    contents = snapshot_lockfile(snapshot.read_text(encoding="utf-8"), snapshot)
    lock = tomllib.loads(contents)

    requires_python = lock.get("requires-python")
    if requires_python and "[X]" in requires_python:
        actual = tomllib.loads(manifest.read_text(encoding="utf-8"))["project"][
            "requires-python"
        ]
        if not actual.startswith(requires_python.partition("[X]")[0]):
            raise RuntimeError(
                f"The project manifest does not restore {requires_python!r}"
            )
        contents = contents.replace(requires_python, actual)

    for package in lock.get("package", []):
        version = package.get("version")
        if not version or "[X]" not in version:
            continue
        digest = package.get("sdist", {}).get("hash")
        actual = known_versions.get(digest, "")
        if not actual or not actual.startswith(version.partition("[X]")[0]):
            raise RuntimeError(
                f"No unredacted training lock identifies {package['name']} {version}"
            )
        contents = contents.replace(version, actual)

    if "[X]" in contents:
        raise RuntimeError(f"Unresolved snapshot redactions remain in {snapshot}")

    project = root / name
    project.mkdir(parents=True, exist_ok=True)
    shutil.copyfile(manifest, project / "pyproject.toml")
    (project / "uv.lock").write_text(contents, encoding="utf-8")
    return project


def prepare_local_lock_project(
    root: Path,
    name: str,
    manifest: Path,
    lockfile: Path,
) -> Path:
    project = root / name
    project.mkdir(parents=True, exist_ok=True)
    shutil.copyfile(manifest, project / "pyproject.toml")
    shutil.copyfile(lockfile, project / "uv.lock")
    return project


def snapshot_lockfile(contents: str, snapshot: Path) -> str:
    header, separator, lockfile = contents.partition("\n---\n")
    if not separator or not header.startswith("---\n"):
        raise RuntimeError(f"Could not locate the Insta metadata header in {snapshot}")
    return lockfile


def prepare_locked_graph(root: Path, project: Path) -> LockedGraph:
    lock = tomllib.loads((project / "uv.lock").read_text(encoding="utf-8"))
    packages = [
        package
        for package in lock.get("package", [])
        if "registry" in package.get("source", {})
    ]
    versions: dict[str, list[str]] = defaultdict(list)
    for package in packages:
        versions[package["name"]].append(package["version"])

    graph = root / project.name
    wheelhouse = graph / "wheelhouse"
    wheelhouse.mkdir(parents=True, exist_ok=True)
    for wheel in wheelhouse.glob("*.whl"):
        wheel.unlink()

    for package in packages:
        write_locked_wheel(wheelhouse, package, versions)

    requirements = graph / "requirements.in"
    roots = REAL_PROJECT_ROOTS[project.name]
    missing = set(roots).difference(versions)
    if missing:
        raise RuntimeError(
            f"Locked graph {project.name} lacks package roots: {missing}"
        )
    requirements.write_text("\n".join(roots) + "\n", encoding="utf-8")

    return LockedGraph(
        name=project.name,
        project=project,
        wheelhouse=wheelhouse,
        requirements=requirements,
        package_count=len(packages),
    )


def write_locked_wheel(
    wheelhouse: Path,
    package: dict[str, Any],
    versions: dict[str, list[str]],
) -> None:
    name = str(package["name"])
    version = str(package["version"])
    normalized = name.replace("-", "_")
    distribution = f"{normalized}-{version}.dist-info"
    optional = package.get("optional-dependencies", {})
    requirements = [
        locked_requirement(dependency, versions)
        for dependency in package.get("dependencies", [])
    ]
    requirements.extend(
        locked_requirement(dependency, versions, extra=extra)
        for extra, dependencies in optional.items()
        for dependency in dependencies
    )
    metadata = [
        "Metadata-Version: 2.3",
        f"Name: {name}",
        f"Version: {version}",
        *(f"Provides-Extra: {extra}" for extra in optional),
        *(f"Requires-Dist: {requirement}" for requirement in requirements),
        "",
    ]
    wheel = (
        "Wheel-Version: 1.0\n"
        "Generator: uv locked ecosystem corpus\n"
        "Root-Is-Purelib: true\n"
        "Tag: py3-none-any\n"
    )
    paths = [
        f"{normalized}.py",
        f"{distribution}/METADATA",
        f"{distribution}/WHEEL",
        f"{distribution}/RECORD",
    ]
    contents = [
        f'NAME = "{name}"\nVERSION = "{version}"\n',
        "\n".join(metadata),
        wheel,
        "".join(f"{path},,\n" for path in paths),
    ]
    destination = wheelhouse / f"{normalized}-{version}-py3-none-any.whl"
    with zipfile.ZipFile(destination, "w", compression=zipfile.ZIP_STORED) as archive:
        for path, content in zip(paths, contents, strict=True):
            info = zipfile.ZipInfo(path, date_time=(1980, 1, 1, 0, 0, 0))
            info.external_attr = 0o644 << 16
            archive.writestr(info, content)


def locked_requirement(
    dependency: dict[str, Any],
    versions: dict[str, list[str]],
    *,
    extra: str | None = None,
) -> str:
    name = str(dependency["name"])
    extras = dependency.get("extra", [])
    requirement = f"{name}[{','.join(extras)}]" if extras else name
    version = dependency.get("version")
    if version:
        requirement += f"=={version}"
    elif len(versions.get(name, [])) == 1:
        requirement += f"=={versions[name][0]}"
    elif name in versions:
        raise RuntimeError(f"Locked dependency {name} has ambiguous package versions")

    marker = dependency.get("marker")
    if extra:
        marker = (
            f'({marker}) and extra == "{extra}"' if marker else f'extra == "{extra}"'
        )
    return f"{requirement}; {marker}" if marker else requirement


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

    lockfile = snapshot_lockfile(snapshot.read_text(encoding="utf-8"), snapshot)
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

    for graph in corpus.real_graphs:
        if graph.wheelhouse == corpus.wheelhouse:
            continue
        commands.append(
            (
                f"resolve-{graph.name}",
                [
                    str(binary),
                    "pip",
                    "compile",
                    str(graph.requirements),
                    "--offline",
                    "--no-index",
                    "--find-links",
                    str(graph.wheelhouse),
                    *python,
                    "--quiet",
                    "--output-file",
                    str(graph.requirements.with_suffix(".txt")),
                ],
            )
        )

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

    # The Windows trampoline calls process::exit, bypassing LLVM's profile flush.
    if launcher.is_file() and launcher.suffix != ".exe":
        commands.append(("launcher", [str(launcher), "--version"]))

    for label, command in commands:
        workload_environment = training_environment.copy()
        if profile_dir is not None:
            group = profile_group(label)
            suffix = "%4m" if group == "real-graphs" else "%m-%p"
            workload_environment["LLVM_PROFILE_FILE"] = str(
                profile_dir / f"uv-{group}-{suffix}.profraw"
            )
        print(f"Training uv workload: {label}", flush=True)
        run(command, environment=workload_environment)

    return tuple(label for label, _ in commands)


def profile_group(label: str) -> str:
    return "real-graphs" if label.startswith("resolve-") else label


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
