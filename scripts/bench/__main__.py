"""Benchmark uv against other packaging tools.

This script assumes that Python 3.12 is installed.

By default, this script also assumes that `pip`, `pip-tools`, `virtualenv`, `poetry` and
`hyperfine` are installed, and that a uv release builds exists at `./target/release/uv`
(relative to the repository root). However, the set of tools is configurable.

To set up the required environment, run:

    cargo build --release
    ./target/release/uv venv
    source .venv/bin/activate
    ./target/release/uv pip sync ./scripts/bench/requirements.txt

Then, to benchmark uv against `pip-tools`:

    python -m scripts.bench --uv --pip-compile requirements.in

It's most common to benchmark multiple uv versions against one another by building
from multiple branches and specifying the path to each binary, as in:

    # Build the baseline version.
    git checkout main
    cargo build --release
    mv ./target/release/uv ./target/release/baseline

    # Build the feature version.
    git checkout feature
    cargo build --release

    # Run the benchmark.
    python -m scripts.bench \
        --uv-path ./target/release/uv \
        --uv-path ./target/release/baseline \
        requirements.in

By default, the script will run the resolution benchmarks when a `requirements.in` file
is provided, and the installation benchmarks when a `requirements.txt` file is provided:

    # Run the resolution benchmarks against the Trio project.
    python -m scripts.bench \
        --uv-path ./target/release/uv \
        --uv-path ./target/release/baseline \
        ./scripts/requirements/trio.in

    # Run the installation benchmarks against the Trio project.
    python -m scripts.bench \
        --uv-path ./target/release/uv \
        --uv-path ./target/release/baseline \
        ./scripts/requirements/compiled/trio.txt

You can also specify the benchmark to run explicitly:

    # Run the "uncached install" benchmark against the Trio project.
    python -m scripts.bench \
        --uv-path ./target/release/uv \
        --uv-path ./target/release/baseline \
        --benchmark install-cold \
        ./scripts/requirements/compiled/trio.txt
"""

import abc
import argparse
import enum
import logging
import os.path
import shlex
import shutil
import subprocess
import tempfile
import typing


class Benchmark(enum.Enum):
    """Enumeration of the benchmarks to run."""

    RESOLVE_COLD = "resolve-cold"
    RESOLVE_WARM = "resolve-warm"
    RESOLVE_INCREMENTAL = "resolve-incremental"
    INSTALL_COLD = "install-cold"
    INSTALL_WARM = "install-warm"


class Command(typing.NamedTuple):
    name: str
    """The name of the command to benchmark."""

    prepare: str
    """The command to run before each benchmark run."""

    command: list[str]
    """The command to benchmark."""


class Hyperfine(typing.NamedTuple):
    benchmark: Benchmark
    """The benchmark to run."""

    commands: list[Command]
    """The commands to benchmark."""

    warmup: int
    """The number of warmup runs to perform."""

    min_runs: int
    """The minimum number of runs to perform."""

    verbose: bool
    """Whether to print verbose output."""

    json: bool
    """Whether to export results to JSON."""

    def run(self) -> None:
        """Run the benchmark using `hyperfine`."""
        args = ["hyperfine"]

        # Export to JSON.
        if self.json:
            args.append("--export-json")
            args.append(f"{self.benchmark.value}.json")

        # Preamble: benchmark-wide setup.
        if self.verbose:
            args.append("--show-output")
        args.append("--warmup")
        args.append(str(self.warmup))
        args.append("--min-runs")
        args.append(str(self.min_runs))

        # Add all command names,
        for command in self.commands:
            args.append("--command-name")
            args.append(command.name)

        # Add all prepare statements.
        for command in self.commands:
            args.append("--prepare")
            args.append(command.prepare)

        # Add all commands.
        for command in self.commands:
            args.append(shlex.join(command.command))

        subprocess.check_call(args)


# The requirement to use when benchmarking an incremental resolution.
# Ideally, this requirement is compatible with all requirements files, but does not
# appear in any resolutions.
INCREMENTAL_REQUIREMENT = "django"


class Suite(abc.ABC):
    """Abstract base class for packaging tools."""

    def command(
        self,
        benchmark: Benchmark,
        requirements_file: str,
        *,
        cwd: str,
    ) -> Command | None:
        """Generate a command to benchmark a given tool."""
        match benchmark:
            case Benchmark.RESOLVE_COLD:
                return self.resolve_cold(requirements_file, cwd=cwd)
            case Benchmark.RESOLVE_WARM:
                return self.resolve_warm(requirements_file, cwd=cwd)
            case Benchmark.RESOLVE_INCREMENTAL:
                return self.resolve_incremental(requirements_file, cwd=cwd)
            case Benchmark.INSTALL_COLD:
                return self.install_cold(requirements_file, cwd=cwd)
            case Benchmark.INSTALL_WARM:
                return self.install_warm(requirements_file, cwd=cwd)
            case _:
                raise ValueError(f"Invalid benchmark: {benchmark}")

    @abc.abstractmethod
    def resolve_cold(self, requirements_file: str, *, cwd: str) -> Command | None:
        """Resolve a set of dependencies using pip-tools, from a cold cache.

        The resolution is performed from scratch, i.e., without an existing lock file,
        and the cache directory is cleared between runs.
        """

    @abc.abstractmethod
    def resolve_warm(self, requirements_file: str, *, cwd: str) -> Command | None:
        """Resolve a set of dependencies using pip-tools, from a warm cache.

        The resolution is performed from scratch, i.e., without an existing lock file;
        however, the cache directory is _not_ cleared between runs.
        """

    @abc.abstractmethod
    def resolve_incremental(
        self, requirements_file: str, *, cwd: str
    ) -> Command | None:
        """Resolve a modified lockfile using pip-tools, from a warm cache.

        The resolution is performed with an existing lock file, and the cache directory
        is _not_ cleared between runs. However, a new dependency is added to the set
        of input requirements, which does not appear in the lock file.
        """

    @abc.abstractmethod
    def install_cold(self, requirements_file: str, *, cwd: str) -> Command | None:
        """Install a set of dependencies using pip-tools, from a cold cache.

        The virtual environment is recreated before each run, and the cache directory
        is cleared between runs.
        """

    @abc.abstractmethod
    def install_warm(self, requirements_file: str, *, cwd: str) -> Command | None:
        """Install a set of dependencies using pip-tools, from a cold cache.

        The virtual environment is recreated before each run, and the cache directory
        is cleared between runs.
        """


class PipCompile(Suite):
    def __init__(self, path: str | None = None) -> None:
        self.name = path or "pip-compile"
        self.path = path or "pip-compile"

    def resolve_cold(self, requirements_file: str, *, cwd: str) -> Command | None:
        cache_dir = os.path.join(cwd, ".cache")
        output_file = os.path.join(cwd, "requirements.txt")

        return Command(
            name=f"{self.name} ({Benchmark.RESOLVE_COLD.value})",
            prepare=f"rm -rf {cwd} && rm -f {output_file}",
            command=[
                self.path,
                os.path.abspath(requirements_file),
                "--cache-dir",
                cache_dir,
                "--output-file",
                output_file,
                "--rebuild",
            ],
        )

    def resolve_warm(self, requirements_file: str, *, cwd: str) -> Command | None:
        cache_dir = os.path.join(cwd, ".cache")
        output_file = os.path.join(cwd, "requirements.txt")

        return Command(
            name=f"{self.name} ({Benchmark.RESOLVE_WARM.value})",
            prepare=f"rm -f {output_file}",
            command=[
                self.path,
                os.path.abspath(requirements_file),
                "--cache-dir",
                cache_dir,
                "--output-file",
                output_file,
            ],
        )

    def resolve_incremental(
        self, requirements_file: str, *, cwd: str
    ) -> Command | None:
        cache_dir = os.path.join(cwd, ".cache")
        baseline = os.path.join(cwd, "baseline.txt")

        # First, perform a cold resolution, to ensure that the lock file exists.
        # TODO(charlie): Make this a `setup`.
        subprocess.check_call(
            [
                self.path,
                os.path.abspath(requirements_file),
                "--cache-dir",
                cache_dir,
                "--output-file",
                baseline,
            ],
            cwd=cwd,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
        )
        assert os.path.exists(baseline), f"Lock file doesn't exist at: {baseline}"

        input_file = os.path.join(cwd, "requirements.in")
        output_file = os.path.join(cwd, "requirements.txt")

        # Add a dependency to the requirements file.
        with open(input_file, "w") as fp1:
            fp1.write(f"{INCREMENTAL_REQUIREMENT}\n")
            with open(requirements_file) as fp2:
                fp1.writelines(fp2.readlines())

        return Command(
            name=f"{self.name} ({Benchmark.RESOLVE_INCREMENTAL.value})",
            prepare=f"rm -f {output_file} && cp {baseline} {output_file}",
            command=[
                self.path,
                input_file,
                "--cache-dir",
                cache_dir,
                "--output-file",
                output_file,
            ],
        )

    def install_cold(self, requirements_file: str, *, cwd: str) -> Command | None: ...

    def install_warm(self, requirements_file: str, *, cwd: str) -> Command | None: ...


class PipSync(Suite):
    def __init__(self, path: str | None = None) -> None:
        self.name = path or "pip-sync"
        self.path = path or "pip-sync"

    def resolve_cold(self, requirements_file: str, *, cwd: str) -> Command | None: ...

    def resolve_warm(self, requirements_file: str, *, cwd: str) -> Command | None: ...

    def resolve_incremental(
        self, requirements_file: str, *, cwd: str
    ) -> Command | None: ...

    def install_cold(self, requirements_file: str, *, cwd: str) -> Command | None:
        cache_dir = os.path.join(cwd, ".cache")
        venv_dir = os.path.join(cwd, ".venv")

        return Command(
            name=f"{self.name} ({Benchmark.INSTALL_COLD.value})",
            prepare=f"rm -rf {cache_dir} && virtualenv --clear -p 3.12 {venv_dir}",
            command=[
                self.path,
                os.path.abspath(requirements_file),
                "--pip-args",
                f"--cache-dir {cache_dir}",
                "--python-executable",
                os.path.join(venv_dir, "bin", "python"),
            ],
        )

    def install_warm(self, requirements_file: str, *, cwd: str) -> Command | None:
        cache_dir = os.path.join(cwd, ".cache")
        venv_dir = os.path.join(cwd, ".venv")

        return Command(
            name=f"{self.name} ({Benchmark.INSTALL_WARM.value})",
            prepare=f"virtualenv --clear -p 3.12 {venv_dir}",
            command=[
                self.path,
                os.path.abspath(requirements_file),
                "--pip-args",
                f"--cache-dir {cache_dir}",
                "--python-executable",
                os.path.join(venv_dir, "bin", "python"),
            ],
        )


class Poetry(Suite):
    def __init__(self, path: str | None = None) -> None:
        self.name = path or "poetry"
        self.path = path or "poetry"

    def setup(self, requirements_file: str, *, cwd: str) -> None:
        """Initialize a Poetry project from a requirements file."""
        import tomli
        import tomli_w
        from packaging.requirements import Requirement

        # Parse all dependencies from the requirements file.
        with open(requirements_file) as fp:
            requirements = [
                Requirement(line)
                for line in fp
                if not line.lstrip().startswith("#") and len(line.strip()) > 0
            ]

        # Create a Poetry project.
        subprocess.check_call(
            [
                self.path,
                "init",
                "--name",
                "bench",
                "--no-interaction",
                "--python",
                "3.12.1",
            ],
            cwd=cwd,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
        )

        # Parse the pyproject.toml.
        with open(os.path.join(cwd, "pyproject.toml"), "rb") as fp:
            pyproject = tomli.load(fp)

        # Add the dependencies to the pyproject.toml.
        pyproject["tool"]["poetry"]["dependencies"].update(
            {
                str(requirement.name): str(requirement.specifier)
                if requirement.specifier
                else "*"
                for requirement in requirements
            }
        )

        with open(os.path.join(cwd, "pyproject.toml"), "wb") as fp:
            tomli_w.dump(pyproject, fp)

    def resolve_cold(self, requirements_file: str, *, cwd: str) -> Command | None:
        self.setup(requirements_file, cwd=cwd)

        poetry_lock = os.path.join(cwd, "poetry.lock")
        config_dir = os.path.join(cwd, "config", "pypoetry")
        cache_dir = os.path.join(cwd, "cache", "pypoetry")
        data_dir = os.path.join(cwd, "data", "pypoetry")

        return Command(
            name=f"{self.name} ({Benchmark.RESOLVE_COLD.value})",
            prepare=(
                f"rm -rf {config_dir} && "
                f"rm -rf {cache_dir} && "
                f"rm -rf {data_dir} &&"
                f"rm -rf {poetry_lock}"
            ),
            command=[
                f"POETRY_CONFIG_DIR={config_dir}",
                f"POETRY_CACHE_DIR={cache_dir}",
                f"POETRY_DATA_DIR={data_dir}",
                self.path,
                "lock",
                "--directory",
                cwd,
            ],
        )

    def resolve_warm(self, requirements_file: str, *, cwd: str) -> Command | None:
        self.setup(requirements_file, cwd=cwd)

        poetry_lock = os.path.join(cwd, "poetry.lock")
        config_dir = os.path.join(cwd, "config", "pypoetry")
        cache_dir = os.path.join(cwd, "cache", "pypoetry")
        data_dir = os.path.join(cwd, "data", "pypoetry")

        return Command(
            name=f"{self.name} ({Benchmark.RESOLVE_WARM.value})",
            prepare=f"rm -f {poetry_lock}",
            command=[
                f"POETRY_CONFIG_DIR={config_dir}",
                f"POETRY_CACHE_DIR={cache_dir}",
                f"POETRY_DATA_DIR={data_dir}",
                self.path,
                "lock",
                "--directory",
                cwd,
            ],
        )

    def resolve_incremental(
        self, requirements_file: str, *, cwd: str
    ) -> Command | None:
        import tomli
        import tomli_w

        self.setup(requirements_file, cwd=cwd)

        poetry_lock = os.path.join(cwd, "poetry.lock")
        assert not os.path.exists(
            poetry_lock
        ), f"Lock file already exists at: {poetry_lock}"

        # Run a resolution, to ensure that the lock file exists.
        # TODO(charlie): Make this a `setup`.
        subprocess.check_call(
            [self.path, "lock"],
            cwd=cwd,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
        )
        assert os.path.exists(poetry_lock), f"Lock file doesn't exist at: {poetry_lock}"

        # Add a dependency to the requirements file.
        with open(os.path.join(cwd, "pyproject.toml"), "rb") as fp:
            pyproject = tomli.load(fp)

        # Add the dependencies to the pyproject.toml.
        pyproject["tool"]["poetry"]["dependencies"].update(
            {
                INCREMENTAL_REQUIREMENT: "*",
            }
        )

        with open(os.path.join(cwd, "pyproject.toml"), "wb") as fp:
            tomli_w.dump(pyproject, fp)

        # Store the baseline lock file.
        baseline = os.path.join(cwd, "baseline.lock")
        shutil.copyfile(poetry_lock, baseline)

        poetry_lock = os.path.join(cwd, "poetry.lock")
        config_dir = os.path.join(cwd, "config", "pypoetry")
        cache_dir = os.path.join(cwd, "cache", "pypoetry")
        data_dir = os.path.join(cwd, "data", "pypoetry")

        return Command(
            name=f"{self.name} ({Benchmark.RESOLVE_INCREMENTAL.value})",
            prepare=f"rm -f {poetry_lock} && cp {baseline} {poetry_lock}",
            command=[
                f"POETRY_CONFIG_DIR={config_dir}",
                f"POETRY_CACHE_DIR={cache_dir}",
                f"POETRY_DATA_DIR={data_dir}",
                self.path,
                "lock",
                "--no-update",
                "--directory",
                cwd,
            ],
        )

    def install_cold(self, requirements_file: str, *, cwd: str) -> Command | None:
        self.setup(requirements_file, cwd=cwd)

        poetry_lock = os.path.join(cwd, "poetry.lock")
        assert not os.path.exists(
            poetry_lock
        ), f"Lock file already exists at: {poetry_lock}"

        # Run a resolution, to ensure that the lock file exists.
        # TODO(charlie): Make this a `setup`.
        subprocess.check_call(
            [self.path, "lock"],
            cwd=cwd,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
        )
        assert os.path.exists(poetry_lock), f"Lock file doesn't exist at: {poetry_lock}"

        config_dir = os.path.join(cwd, "config", "pypoetry")
        cache_dir = os.path.join(cwd, "cache", "pypoetry")
        data_dir = os.path.join(cwd, "data", "pypoetry")
        venv_dir = os.path.join(cwd, ".venv")

        return Command(
            name=f"{self.name} ({Benchmark.INSTALL_COLD.value})",
            prepare=(
                f"rm -rf {config_dir} && "
                f"rm -rf {cache_dir} && "
                f"rm -rf {data_dir} &&"
                f"virtualenv --clear -p 3.12 {venv_dir} --no-seed"
            ),
            command=[
                f"POETRY_CONFIG_DIR={config_dir}",
                f"POETRY_CACHE_DIR={cache_dir}",
                f"POETRY_DATA_DIR={data_dir}",
                f"VIRTUAL_ENV={venv_dir}",
                self.path,
                "install",
                "--no-root",
                "--sync",
                "--directory",
                cwd,
            ],
        )

    def install_warm(self, requirements_file: str, *, cwd: str) -> Command | None:
        self.setup(requirements_file, cwd=cwd)

        poetry_lock = os.path.join(cwd, "poetry.lock")
        assert not os.path.exists(
            poetry_lock
        ), f"Lock file already exists at: {poetry_lock}"

        # Run a resolution, to ensure that the lock file exists.
        subprocess.check_call(
            [self.path, "lock"],
            cwd=cwd,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
        )
        assert os.path.exists(poetry_lock), f"Lock file doesn't exist at: {poetry_lock}"

        config_dir = os.path.join(cwd, "config", "pypoetry")
        cache_dir = os.path.join(cwd, "cache", "pypoetry")
        data_dir = os.path.join(cwd, "data", "pypoetry")
        venv_dir = os.path.join(cwd, ".venv")

        return Command(
            name=f"{self.name} ({Benchmark.INSTALL_WARM.value})",
            prepare=f"virtualenv --clear -p 3.12 {venv_dir}",
            command=[
                f"POETRY_CONFIG_DIR={config_dir}",
                f"POETRY_CACHE_DIR={cache_dir}",
                f"POETRY_DATA_DIR={data_dir}",
                f"VIRTUAL_ENV={venv_dir}",
                self.path,
                "install",
                "--no-root",
                "--sync",
                "--directory",
                cwd,
            ],
        )


class Pdm(Suite):
    def __init__(self, path: str | None = None) -> None:
        self.name = path or "pdm"
        self.path = path or "pdm"

    def setup(self, requirements_file: str, *, cwd: str) -> None:
        """Initialize a PDM project from a requirements file."""
        import tomli
        import tomli_w
        from packaging.requirements import Requirement

        # Parse all dependencies from the requirements file.
        with open(requirements_file) as fp:
            requirements = [
                Requirement(line)
                for line in fp
                if not line.lstrip().startswith("#") and len(line.strip()) > 0
            ]

        # Create a PDM project.
        subprocess.check_call(
            [self.path, "init", "--non-interactive", "--python", "3.12"],
            cwd=cwd,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
        )

        # Parse the pyproject.toml.
        with open(os.path.join(cwd, "pyproject.toml"), "rb") as fp:
            pyproject = tomli.load(fp)

        # Add the dependencies to the pyproject.toml.
        pyproject["project"]["dependencies"] = [
            str(requirement) for requirement in requirements
        ]

        with open(os.path.join(cwd, "pyproject.toml"), "wb") as fp:
            tomli_w.dump(pyproject, fp)

    def resolve_cold(self, requirements_file: str, *, cwd: str) -> Command | None:
        self.setup(requirements_file, cwd=cwd)

        pdm_lock = os.path.join(cwd, "pdm.lock")
        cache_dir = os.path.join(cwd, "cache", "pdm")

        return Command(
            name=f"{self.name} ({Benchmark.RESOLVE_COLD.value})",
            prepare=f"rm -rf {cache_dir} && rm -rf {pdm_lock} && {self.path} config cache_dir {cache_dir}",
            command=[
                self.path,
                "lock",
                "--project",
                cwd,
            ],
        )

    def resolve_warm(self, requirements_file: str, *, cwd: str) -> Command | None:
        self.setup(requirements_file, cwd=cwd)

        pdm_lock = os.path.join(cwd, "pdm.lock")
        cache_dir = os.path.join(cwd, "cache", "pdm")

        return Command(
            name=f"{self.name} ({Benchmark.RESOLVE_WARM.value})",
            prepare=f"rm -rf {pdm_lock} && {self.path} config cache_dir {cache_dir}",
            command=[
                self.path,
                "lock",
                "--project",
                cwd,
            ],
        )

    def resolve_incremental(
        self, requirements_file: str, *, cwd: str
    ) -> Command | None:
        import tomli
        import tomli_w

        self.setup(requirements_file, cwd=cwd)

        pdm_lock = os.path.join(cwd, "pdm.lock")
        assert not os.path.exists(pdm_lock), f"Lock file already exists at: {pdm_lock}"

        # Run a resolution, to ensure that the lock file exists.
        # TODO(charlie): Make this a `setup`.
        subprocess.check_call(
            [self.path, "lock"],
            cwd=cwd,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
        )
        assert os.path.exists(pdm_lock), f"Lock file doesn't exist at: {pdm_lock}"

        # Add a dependency to the requirements file.
        with open(os.path.join(cwd, "pyproject.toml"), "rb") as fp:
            pyproject = tomli.load(fp)

        # Add the dependencies to the pyproject.toml.
        pyproject["project"]["dependencies"] += [INCREMENTAL_REQUIREMENT]

        with open(os.path.join(cwd, "pyproject.toml"), "wb") as fp:
            tomli_w.dump(pyproject, fp)

        # Store the baseline lock file.
        baseline = os.path.join(cwd, "baseline.lock")
        shutil.copyfile(pdm_lock, baseline)

        pdm_lock = os.path.join(cwd, "pdm.lock")
        cache_dir = os.path.join(cwd, "cache", "pdm")

        return Command(
            name=f"{self.name} ({Benchmark.RESOLVE_INCREMENTAL.value})",
            prepare=f"rm -f {pdm_lock} && cp {baseline} {pdm_lock} && {self.path} config cache_dir {cache_dir}",
            command=[
                self.path,
                "lock",
                "--update-reuse",
                "--project",
                cwd,
            ],
        )

    def install_cold(self, requirements_file: str, *, cwd: str) -> Command | None:
        self.setup(requirements_file, cwd=cwd)

        pdm_lock = os.path.join(cwd, "pdm.lock")
        assert not os.path.exists(pdm_lock), f"Lock file already exists at: {pdm_lock}"

        # Run a resolution, to ensure that the lock file exists.
        # TODO(charlie): Make this a `setup`.
        subprocess.check_call(
            [self.path, "lock"],
            cwd=cwd,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
        )
        assert os.path.exists(pdm_lock), f"Lock file doesn't exist at: {pdm_lock}"

        venv_dir = os.path.join(cwd, ".venv")
        cache_dir = os.path.join(cwd, "cache", "pdm")

        return Command(
            name=f"{self.name} ({Benchmark.INSTALL_COLD.value})",
            prepare=(
                f"rm -rf {cache_dir} && "
                f"{self.path} config cache_dir {cache_dir} && "
                f"virtualenv --clear -p 3.12 {venv_dir} --no-seed"
            ),
            command=[
                f"VIRTUAL_ENV={venv_dir}",
                self.path,
                "sync",
                "--project",
                cwd,
            ],
        )

    def install_warm(self, requirements_file: str, *, cwd: str) -> Command | None:
        self.setup(requirements_file, cwd=cwd)

        pdm_lock = os.path.join(cwd, "pdm.lock")
        assert not os.path.exists(pdm_lock), f"Lock file already exists at: {pdm_lock}"

        # Run a resolution, to ensure that the lock file exists.
        # TODO(charlie): Make this a `setup`.
        subprocess.check_call(
            [self.path, "lock"],
            cwd=cwd,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
        )
        assert os.path.exists(pdm_lock), f"Lock file doesn't exist at: {pdm_lock}"

        venv_dir = os.path.join(cwd, ".venv")
        cache_dir = os.path.join(cwd, "cache", "pdm")

        return Command(
            name=f"{self.name} ({Benchmark.INSTALL_WARM.value})",
            prepare=(
                f"{self.path} config cache_dir {cache_dir} && "
                f"virtualenv --clear -p 3.12 {venv_dir} --no-seed"
            ),
            command=[
                f"VIRTUAL_ENV={venv_dir}",
                self.path,
                "sync",
                "--project",
                cwd,
            ],
        )


class uv(Suite):
    def __init__(self, *, path: str | None = None) -> Command | None:
        """Initialize a uv benchmark."""
        self.name = path or "uv"
        self.path = path or os.path.join(
            os.path.dirname(
                os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
            ),
            "target",
            "release",
            "uv",
        )

    def resolve_cold(self, requirements_file: str, *, cwd: str) -> Command | None:
        cache_dir = os.path.join(cwd, ".cache")
        output_file = os.path.join(cwd, "requirements.txt")

        return Command(
            name=f"{self.name} ({Benchmark.RESOLVE_COLD.value})",
            prepare=f"rm -rf {cwd} && rm -f {output_file}",
            command=[
                self.path,
                "pip",
                "compile",
                os.path.abspath(requirements_file),
                "--cache-dir",
                cache_dir,
                "--output-file",
                output_file,
            ],
        )

    def resolve_warm(self, requirements_file: str, *, cwd: str) -> Command | None:
        cache_dir = os.path.join(cwd, ".cache")
        output_file = os.path.join(cwd, "requirements.txt")

        return Command(
            name=f"{self.name} ({Benchmark.RESOLVE_WARM.value})",
            prepare=f"rm -f {output_file}",
            command=[
                self.path,
                "pip",
                "compile",
                os.path.abspath(requirements_file),
                "--cache-dir",
                cache_dir,
                "--output-file",
                output_file,
            ],
        )

    def resolve_incremental(
        self, requirements_file: str, *, cwd: str
    ) -> Command | None:
        cache_dir = os.path.join(cwd, ".cache")
        baseline = os.path.join(cwd, "baseline.txt")

        # First, perform a cold resolution, to ensure that the lock file exists.
        # TODO(charlie): Make this a `setup`.
        subprocess.check_call(
            [
                self.path,
                "pip",
                "compile",
                os.path.abspath(requirements_file),
                "--cache-dir",
                cache_dir,
                "--output-file",
                baseline,
            ],
            cwd=cwd,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
        )
        assert os.path.exists(baseline), f"Lock file doesn't exist at: {baseline}"

        input_file = os.path.join(cwd, "requirements.in")
        output_file = os.path.join(cwd, "requirements.txt")

        # Add a dependency to the requirements file.
        with open(input_file, "w") as fp1:
            fp1.write(f"{INCREMENTAL_REQUIREMENT}\n")
            with open(requirements_file) as fp2:
                fp1.writelines(fp2.readlines())

        return Command(
            name=f"{self.name} ({Benchmark.RESOLVE_INCREMENTAL.value})",
            prepare=f"rm -f {output_file} && cp {baseline} {output_file}",
            command=[
                self.path,
                "pip",
                "compile",
                input_file,
                "--cache-dir",
                cache_dir,
                "--output-file",
                output_file,
            ],
        )

    def install_cold(self, requirements_file: str, *, cwd: str) -> Command | None:
        cache_dir = os.path.join(cwd, ".cache")
        venv_dir = os.path.join(cwd, ".venv")

        return Command(
            name=f"{self.name} ({Benchmark.INSTALL_COLD.value})",
            prepare=f"rm -rf {cache_dir} && virtualenv --clear -p 3.12 {venv_dir}",
            command=[
                f"VIRTUAL_ENV={venv_dir}",
                self.path,
                "pip",
                "sync",
                os.path.abspath(requirements_file),
                "--cache-dir",
                cache_dir,
            ],
        )

    def install_warm(self, requirements_file: str, *, cwd: str) -> Command | None:
        cache_dir = os.path.join(cwd, ".cache")
        venv_dir = os.path.join(cwd, ".venv")

        return Command(
            name=f"{self.name} ({Benchmark.INSTALL_WARM.value})",
            prepare=f"virtualenv --clear -p 3.12 {venv_dir}",
            command=[
                f"VIRTUAL_ENV={venv_dir}",
                self.path,
                "pip",
                "sync",
                os.path.abspath(requirements_file),
                "--cache-dir",
                cache_dir,
            ],
        )


def main():
    """Run the benchmark."""
    parser = argparse.ArgumentParser(
        description="Benchmark uv against other packaging tools."
    )
    parser.add_argument(
        "file",
        type=str,
        help=(
            "The file to read the dependencies from (typically: `requirements.in` "
            "(for resolution) or `requirements.txt` (for installation))."
        ),
    )
    parser.add_argument(
        "--verbose", "-v", action="store_true", help="Print verbose output."
    )
    parser.add_argument("--json", action="store_true", help="Export results to JSON.")
    parser.add_argument(
        "--warmup",
        type=int,
        help="The number of warmup runs to perform.",
        default=3,
    )
    parser.add_argument(
        "--min-runs",
        type=int,
        help="The minimum number of runs to perform.",
        default=10,
    )
    parser.add_argument(
        "--benchmark",
        "-b",
        type=str,
        help="The benchmark(s) to run.",
        choices=[benchmark.value for benchmark in Benchmark],
        action="append",
    )
    parser.add_argument(
        "--pip-sync",
        help="Whether to benchmark `pip-sync` (requires `pip-tools` to be installed).",
        action="store_true",
    )
    parser.add_argument(
        "--pip-compile",
        help="Whether to benchmark `pip-compile` (requires `pip-tools` to be installed).",
        action="store_true",
    )
    parser.add_argument(
        "--poetry",
        help="Whether to benchmark Poetry (requires Poetry to be installed).",
        action="store_true",
    )
    parser.add_argument(
        "--pdm",
        help="Whether to benchmark PDM (requires PDM to be installed).",
        action="store_true",
    )
    parser.add_argument(
        "--uv",
        help="Whether to benchmark uv (assumes a uv binary exists at `./target/release/uv`).",
        action="store_true",
    )
    parser.add_argument(
        "--pip-sync-path",
        type=str,
        help="Path(s) to the `pip-sync` binary to benchmark.",
        action="append",
    )
    parser.add_argument(
        "--pip-compile-path",
        type=str,
        help="Path(s) to the `pip-compile` binary to benchmark.",
        action="append",
    )
    parser.add_argument(
        "--poetry-path",
        type=str,
        help="Path(s) to the Poetry binary to benchmark.",
        action="append",
    )
    parser.add_argument(
        "--pdm-path",
        type=str,
        help="Path(s) to the PDM binary to benchmark.",
        action="append",
    )
    parser.add_argument(
        "--uv-path",
        type=str,
        help="Path(s) to the uv binary to benchmark.",
        action="append",
    )

    args = parser.parse_args()
    logging.basicConfig(
        level=logging.INFO if args.verbose else logging.WARN,
        format="%(asctime)s %(levelname)s %(message)s",
        datefmt="%Y-%m-%d %H:%M:%S",
    )

    verbose = args.verbose
    json = args.json
    warmup = args.warmup
    min_runs = args.min_runs

    requirements_file = os.path.abspath(args.file)
    if not os.path.exists(requirements_file):
        raise ValueError(f"File not found: {requirements_file}")

    # Determine the tools to benchmark, based on the user-provided arguments.
    suites = []
    if args.pip_sync:
        suites.append(PipSync())
    if args.pip_compile:
        suites.append(PipCompile())
    if args.poetry:
        suites.append(Poetry())
    if args.pdm:
        suites.append(Pdm())
    if args.uv:
        suites.append(uv())
    for path in args.pip_sync_path or []:
        suites.append(PipSync(path=path))
    for path in args.pip_compile_path or []:
        suites.append(PipCompile(path=path))
    for path in args.poetry_path or []:
        suites.append(Poetry(path=path))
    for path in args.pdm_path or []:
        suites.append(Pdm(path=path))
    for path in args.uv_path or []:
        suites.append(uv(path=path))

    # If no tools were specified, benchmark all tools.
    if not suites:
        suites = [
            PipSync(),
            PipCompile(),
            Poetry(),
            uv(),
        ]

    # Determine the benchmarks to run, based on user input. If no benchmarks were
    # specified, infer an appropriate set based on the file extension.
    benchmarks = (
        [Benchmark(benchmark) for benchmark in args.benchmark]
        if args.benchmark is not None
        else [Benchmark.RESOLVE_COLD, Benchmark.RESOLVE_WARM]
        if requirements_file.endswith(".in")
        else [Benchmark.INSTALL_COLD, Benchmark.INSTALL_WARM]
        if requirements_file.endswith(".txt")
        else list(Benchmark)
    )

    logging.info("Reading requirements from: {}".format(requirements_file))
    logging.info("```")
    with open(args.file, "r") as f:
        for line in f:
            logging.info(line.rstrip())
    logging.info("```")

    with tempfile.TemporaryDirectory() as cwd:
        for benchmark in benchmarks:
            # Generate the benchmark command for each tool.
            commands = [
                command
                for suite in suites
                if (
                    command := suite.command(
                        benchmark, requirements_file, cwd=tempfile.mkdtemp(dir=cwd)
                    )
                )
            ]

            if commands:
                hyperfine = Hyperfine(
                    benchmark=benchmark,
                    commands=commands,
                    warmup=warmup,
                    min_runs=min_runs,
                    verbose=verbose,
                    json=json,
                )
                hyperfine.run()


if __name__ == "__main__":
    main()
