"""Benchmark Puffin against other packaging tools.

This script assumes that `pip`, `pip-tools`, `virtualenv`, `poetry` and `hyperfine` are
installed, and that a Puffin release builds exists at `./target/release/puffin`
(relative to the repository root).

This script assumes that Python 3.10 is installed.

To set up the required environment, run:

    cargo build --release
    ./target/release/puffin venv
    ./target/release/puffin pip-sync ./scripts/requirements.txt

Example usage:

    python -m scripts.bench -t puffin -t pip-tools requirements.in

Tools can be repeated and accompanied by a binary to benchmark multiple versions of the
same tool, as in:

    python -m scripts.bench \
        -t puffin -p ./target/release/puffin \
        -t puffin -p ./target/release/baseline \
        requirements.in
"""
import abc
import argparse
import enum
import logging
import os.path
import shlex
import subprocess
import tempfile
from itertools import zip_longest

import tomli
import tomli_w
from packaging.requirements import Requirement

WARMUP = 3
MIN_RUNS = 10


class Tool(enum.Enum):
    """Enumeration of the tools to benchmark."""

    PIP_TOOLS = "pip-tools"
    """`pip-sync` and `pip-compile`, from the `pip-tools` package."""

    POETRY = "poetry"
    """The `poetry` package manager."""

    PUFFIN = "puffin"
    """A Puffin release build, assumed to be located at `./target/release/puffin`."""


class Benchmark(enum.Enum):
    """Enumeration of the benchmarks to run."""

    RESOLVE_COLD = "resolve-cold"
    RESOLVE_WARM = "resolve-warm"
    INSTALL_COLD = "install-cold"
    INSTALL_WARM = "install-warm"


class Suite(abc.ABC):
    """Abstract base class for packaging tools."""

    def run_benchmark(
        self,
        benchmark: Benchmark,
        requirements_file: str,
        *,
        verbose: bool,
    ) -> None:
        """Run a benchmark for a given tool."""
        match benchmark:
            case Benchmark.RESOLVE_COLD:
                self.resolve_cold(requirements_file, verbose=verbose)
            case Benchmark.RESOLVE_WARM:
                self.resolve_warm(requirements_file, verbose=verbose)
            case Benchmark.INSTALL_COLD:
                self.install_cold(requirements_file, verbose=verbose)
            case Benchmark.INSTALL_WARM:
                self.install_warm(requirements_file, verbose=verbose)

    @abc.abstractmethod
    def resolve_cold(self, requirements_file: str, *, verbose: bool) -> None:
        """Resolve a set of dependencies using pip-tools, from a cold cache.

        The resolution is performed from scratch, i.e., without an existing lock file,
        and the cache directory is cleared between runs.
        """

    @abc.abstractmethod
    def resolve_warm(self, requirements_file: str, *, verbose: bool) -> None:
        """Resolve a set of dependencies using pip-tools, from a warm cache.

        The resolution is performed from scratch, i.e., without an existing lock file;
        however, the cache directory is _not_ cleared between runs.
        """

    @abc.abstractmethod
    def install_cold(self, requirements_file: str, *, verbose: bool) -> None:
        """Install a set of dependencies using pip-tools, from a cold cache.

        The virtual environment is recreated before each run, and the cache directory
        is cleared between runs.
        """

    @abc.abstractmethod
    def install_warm(self, requirements_file: str, *, verbose: bool) -> None:
        """Install a set of dependencies using pip-tools, from a cold cache.

        The virtual environment is recreated before each run, and the cache directory
        is cleared between runs.
        """


class PipTools(Suite):
    def __init__(self, path: str | None = None) -> None:
        self.path = path

    def resolve_cold(self, requirements_file: str, *, verbose: bool) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            cache_dir = os.path.join(temp_dir, ".cache")
            output_file = os.path.join(temp_dir, "requirements.txt")

            subprocess.check_call(
                [
                    "hyperfine",
                    *(["--show-output"] if verbose else []),
                    "--command-name",
                    f"{self.path or Tool.PIP_TOOLS.value} ({Benchmark.RESOLVE_COLD.value})",
                    "--warmup",
                    str(WARMUP),
                    "--min-runs",
                    str(MIN_RUNS),
                    "--prepare",
                    f"rm -rf {temp_dir} && rm -f {output_file}",
                    shlex.join(
                        [
                            self.path or "pip-compile",
                            os.path.abspath(requirements_file),
                            "--cache-dir",
                            cache_dir,
                            "--output-file",
                            output_file,
                            "--rebuild",
                        ]
                    ),
                ]
            )

    def resolve_warm(self, requirements_file: str, *, verbose: bool) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            cache_dir = os.path.join(temp_dir, ".cache")
            output_file = os.path.join(temp_dir, "requirements.txt")

            subprocess.check_call(
                [
                    "hyperfine",
                    *(["--show-output"] if verbose else []),
                    "--command-name",
                    f"{self.path or Tool.PIP_TOOLS.value} ({Benchmark.RESOLVE_WARM.value})",
                    "--warmup",
                    str(WARMUP),
                    "--min-runs",
                    str(MIN_RUNS),
                    "--prepare",
                    f"rm -f {output_file}",
                    shlex.join(
                        [
                            self.path or "pip-compile",
                            os.path.abspath(requirements_file),
                            "--cache-dir",
                            cache_dir,
                            "--output-file",
                            output_file,
                        ]
                    ),
                ]
            )

    def install_cold(self, requirements_file: str, *, verbose: bool) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            cache_dir = os.path.join(temp_dir, ".cache")
            venv_dir = os.path.join(temp_dir, ".venv")

            subprocess.check_call(
                [
                    "hyperfine",
                    *(["--show-output"] if verbose else []),
                    "--command-name",
                    f"{self.path or Tool.PIP_TOOLS.value} ({Benchmark.INSTALL_COLD.value})",
                    "--warmup",
                    str(WARMUP),
                    "--min-runs",
                    str(MIN_RUNS),
                    "--prepare",
                    f"rm -rf {cache_dir} && virtualenv --clear -p 3.10 {venv_dir}",
                    shlex.join(
                        [
                            self.path or "pip-sync",
                            os.path.abspath(requirements_file),
                            "--pip-args",
                            f"--cache-dir {cache_dir}",
                            "--python-executable",
                            os.path.join(venv_dir, "bin", "python"),
                        ]
                    ),
                ]
            )

    def install_warm(self, requirements_file: str, *, verbose: bool) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            cache_dir = os.path.join(temp_dir, ".cache")
            venv_dir = os.path.join(temp_dir, ".venv")

            subprocess.check_call(
                [
                    "hyperfine",
                    *(["--show-output"] if verbose else []),
                    "--command-name",
                    f"{self.path or Tool.PIP_TOOLS.value} ({Benchmark.INSTALL_WARM.value})",
                    "--warmup",
                    str(WARMUP),
                    "--min-runs",
                    str(MIN_RUNS),
                    "--prepare",
                    f"virtualenv --clear -p 3.10 {venv_dir}",
                    shlex.join(
                        [
                            self.path or "pip-sync",
                            os.path.abspath(requirements_file),
                            "--pip-args",
                            f"--cache-dir {cache_dir}",
                            "--python-executable",
                            os.path.join(venv_dir, "bin", "python"),
                        ]
                    ),
                ]
            )


class Puffin(Suite):
    def __init__(self, *, path: str | None = None) -> None:
        """Initialize a Puffin benchmark."""
        self.path = path

    def resolve_cold(self, requirements_file: str, *, verbose: bool) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            cache_dir = os.path.join(temp_dir, ".cache")
            output_file = os.path.join(temp_dir, "requirements.txt")

            subprocess.check_call(
                [
                    "hyperfine",
                    *(["--show-output"] if verbose else []),
                    "--command-name",
                    f"{self.path or Tool.PUFFIN.value} ({Benchmark.RESOLVE_COLD.value})",
                    "--warmup",
                    str(WARMUP),
                    "--min-runs",
                    str(MIN_RUNS),
                    "--prepare",
                    f"rm -rf {temp_dir} && rm -f {output_file}",
                    shlex.join(
                        [
                            self.path
                            or os.path.join(
                                os.path.dirname(
                                    os.path.dirname(
                                        os.path.dirname(os.path.abspath(__file__))
                                    )
                                ),
                                "target",
                                "release",
                                "puffin",
                            ),
                            "pip-compile",
                            os.path.abspath(requirements_file),
                            "--cache-dir",
                            cache_dir,
                            "--output-file",
                            output_file,
                        ]
                    ),
                ]
            )

    def resolve_warm(self, requirements_file: str, *, verbose: bool) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            cache_dir = os.path.join(temp_dir, ".cache")
            output_file = os.path.join(temp_dir, "requirements.txt")

            subprocess.check_call(
                [
                    "hyperfine",
                    *(["--show-output"] if verbose else []),
                    "--command-name",
                    f"{self.path or Tool.PIP_TOOLS.value} ({Benchmark.RESOLVE_WARM.value})",
                    "--warmup",
                    str(WARMUP),
                    "--min-runs",
                    str(MIN_RUNS),
                    "--prepare",
                    f"rm -f {output_file}",
                    shlex.join(
                        [
                            self.path
                            or os.path.join(
                                os.path.dirname(
                                    os.path.dirname(
                                        os.path.dirname(os.path.abspath(__file__))
                                    )
                                ),
                                "target",
                                "release",
                                "puffin",
                            ),
                            "pip-compile",
                            os.path.abspath(requirements_file),
                            "--cache-dir",
                            cache_dir,
                            "--output-file",
                            output_file,
                        ]
                    ),
                ]
            )

    def install_cold(self, requirements_file: str, *, verbose: bool) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            cache_dir = os.path.join(temp_dir, ".cache")
            venv_dir = os.path.join(temp_dir, ".venv")

            subprocess.check_call(
                [
                    "hyperfine",
                    *(["--show-output"] if verbose else []),
                    "--command-name",
                    f"{self.path or Tool.PIP_TOOLS.value} ({Benchmark.INSTALL_COLD.value})",
                    "--warmup",
                    str(WARMUP),
                    "--min-runs",
                    str(MIN_RUNS),
                    "--prepare",
                    f"rm -rf {cache_dir} && virtualenv --clear -p 3.10 {venv_dir}",
                    shlex.join(
                        [
                            f"VIRTUAL_ENV={venv_dir}",
                            self.path
                            or os.path.join(
                                os.path.dirname(
                                    os.path.dirname(
                                        os.path.dirname(os.path.abspath(__file__))
                                    )
                                ),
                                "target",
                                "release",
                                "puffin",
                            ),
                            "pip-sync",
                            os.path.abspath(requirements_file),
                            "--cache-dir",
                            cache_dir,
                        ]
                    ),
                ]
            )

    def install_warm(self, requirements_file: str, *, verbose: bool) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            cache_dir = os.path.join(temp_dir, ".cache")
            venv_dir = os.path.join(temp_dir, ".venv")

            subprocess.check_call(
                [
                    "hyperfine",
                    *(["--show-output"] if verbose else []),
                    "--command-name",
                    f"{self.path or Tool.PIP_TOOLS.value} ({Benchmark.INSTALL_WARM.value})",
                    "--warmup",
                    str(WARMUP),
                    "--min-runs",
                    str(MIN_RUNS),
                    "--prepare",
                    f"virtualenv --clear -p 3.10 {venv_dir}",
                    shlex.join(
                        [
                            f"VIRTUAL_ENV={venv_dir}",
                            self.path
                            or os.path.join(
                                os.path.dirname(
                                    os.path.dirname(
                                        os.path.dirname(os.path.abspath(__file__))
                                    )
                                ),
                                "target",
                                "release",
                                "puffin",
                            ),
                            "pip-sync",
                            os.path.abspath(requirements_file),
                            "--cache-dir",
                            cache_dir,
                        ]
                    ),
                ]
            )


class Poetry(Suite):
    def __init__(self, path: str | None = None) -> None:
        self.path = path

    def init(self, requirements_file: str, *, working_dir: str) -> None:
        """Initialize a Poetry project from a requirements file."""
        # Parse all dependencies from the requirements file.
        with open(requirements_file) as fp:
            requirements = [
                Requirement(line) for line in fp if not line.startswith("#")
            ]

        # Create a Poetry project.
        subprocess.check_call(
            [
                self.path or "poetry",
                "init",
                "--name",
                "bench",
                "--no-interaction",
                "--python",
                ">=3.10",
            ],
            cwd=working_dir,
        )

        # Parse the pyproject.toml.
        with open(os.path.join(working_dir, "pyproject.toml"), "rb") as fp:
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

        with open(os.path.join(working_dir, "pyproject.toml"), "wb") as fp:
            tomli_w.dump(pyproject, fp)

    def resolve_cold(self, requirements_file: str, *, verbose: bool) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            self.init(requirements_file, working_dir=temp_dir)

            poetry_lock = os.path.join(temp_dir, "poetry.lock")
            config_dir = os.path.join(temp_dir, "config", "pypoetry")
            cache_dir = os.path.join(temp_dir, "cache", "pypoetry")
            data_dir = os.path.join(temp_dir, "data", "pypoetry")

            subprocess.check_call(
                [
                    "hyperfine",
                    *(["--show-output"] if verbose else []),
                    "--command-name",
                    f"{self.path or Tool.POETRY.value} ({Benchmark.RESOLVE_COLD.value})",
                    "--warmup",
                    str(WARMUP),
                    "--min-runs",
                    str(MIN_RUNS),
                    "--prepare",
                    (
                        f"rm -rf {config_dir} && "
                        f"rm -rf {cache_dir} && "
                        f"rm -rf {data_dir} &&"
                        f"rm -rf {poetry_lock}"
                    ),
                    shlex.join(
                        [
                            f"POETRY_CONFIG_DIR={config_dir}",
                            f"POETRY_CACHE_DIR={cache_dir}",
                            f"POETRY_DATA_DIR={data_dir}",
                            self.path or "poetry",
                            "lock",
                        ]
                    ),
                ],
                cwd=temp_dir,
            )

    def resolve_warm(self, requirements_file: str, *, verbose: bool) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            self.init(requirements_file, working_dir=temp_dir)

            poetry_lock = os.path.join(temp_dir, "poetry.lock")
            config_dir = os.path.join(temp_dir, "config", "pypoetry")
            cache_dir = os.path.join(temp_dir, "cache", "pypoetry")
            data_dir = os.path.join(temp_dir, "data", "pypoetry")

            subprocess.check_call(
                [
                    "hyperfine",
                    *(["--show-output"] if verbose else []),
                    "--command-name",
                    f"{self.path or Tool.POETRY.value} ({Benchmark.RESOLVE_WARM.value})",
                    "--warmup",
                    str(WARMUP),
                    "--min-runs",
                    str(MIN_RUNS),
                    "--prepare",
                    f"rm -rf {poetry_lock}",
                    shlex.join(
                        [
                            f"POETRY_CONFIG_DIR={config_dir}",
                            f"POETRY_CACHE_DIR={cache_dir}",
                            f"POETRY_DATA_DIR={data_dir}",
                            self.path or "poetry",
                            "lock",
                        ]
                    ),
                ],
                cwd=temp_dir,
            )

    def install_cold(self, requirements_file: str, *, verbose: bool) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            self.init(requirements_file, working_dir=temp_dir)

            poetry_lock = os.path.join(temp_dir, "poetry.lock")
            assert not os.path.exists(
                poetry_lock
            ), f"Lock file already exists at: {poetry_lock}"

            # Run a resolution, to ensure that the lock file exists.
            subprocess.check_call(
                [self.path or "poetry", "lock"],
                cwd=temp_dir,
                stdout=subprocess.DEVNULL,
                stderr=subprocess.DEVNULL,
            )
            assert os.path.exists(
                poetry_lock
            ), f"Lock file doesn't exist at: {poetry_lock}"

            config_dir = os.path.join(temp_dir, "config", "pypoetry")
            cache_dir = os.path.join(temp_dir, "cache", "pypoetry")
            data_dir = os.path.join(temp_dir, "data", "pypoetry")
            venv_dir = os.path.join(temp_dir, ".venv")

            subprocess.check_call(
                [
                    "hyperfine",
                    *(["--show-output"] if verbose else []),
                    "--command-name",
                    f"{self.path or Tool.POETRY.value} ({Benchmark.INSTALL_COLD.value})",
                    "--warmup",
                    str(WARMUP),
                    "--min-runs",
                    str(MIN_RUNS),
                    "--prepare",
                    (
                        f"rm -rf {config_dir} && "
                        f"rm -rf {cache_dir} && "
                        f"rm -rf {data_dir} &&"
                        f"virtualenv --clear -p 3.10 {venv_dir} --no-seed"
                    ),
                    shlex.join(
                        [
                            f"POETRY_CONFIG_DIR={config_dir}",
                            f"POETRY_CACHE_DIR={cache_dir}",
                            f"POETRY_DATA_DIR={data_dir}",
                            f"VIRTUAL_ENV={venv_dir}",
                            self.path or "poetry",
                            "install",
                            "--no-root",
                            "--sync",
                        ]
                    ),
                ],
                cwd=temp_dir,
            )

    def install_warm(self, requirements_file: str, *, verbose: bool) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            self.init(requirements_file, working_dir=temp_dir)

            poetry_lock = os.path.join(temp_dir, "poetry.lock")
            assert not os.path.exists(
                poetry_lock
            ), f"Lock file already exists at: {poetry_lock}"

            # Run a resolution, to ensure that the lock file exists.
            subprocess.check_call(
                [self.path or "poetry", "lock"],
                cwd=temp_dir,
                stdout=subprocess.DEVNULL,
                stderr=subprocess.DEVNULL,
            )
            assert os.path.exists(
                poetry_lock
            ), f"Lock file doesn't exist at: {poetry_lock}"

            config_dir = os.path.join(temp_dir, "config", "pypoetry")
            cache_dir = os.path.join(temp_dir, "cache", "pypoetry")
            data_dir = os.path.join(temp_dir, "data", "pypoetry")
            venv_dir = os.path.join(temp_dir, ".venv")

            subprocess.check_call(
                [
                    "hyperfine",
                    *(["--show-output"] if verbose else []),
                    "--command-name",
                    f"{self.path or Tool.POETRY.value} ({Benchmark.INSTALL_WARM.value})",
                    "--warmup",
                    str(WARMUP),
                    "--min-runs",
                    str(MIN_RUNS),
                    "--prepare",
                    f"virtualenv --clear -p 3.10 {venv_dir} --no-seed",
                    shlex.join(
                        [
                            f"POETRY_CONFIG_DIR={config_dir}",
                            f"POETRY_CACHE_DIR={cache_dir}",
                            f"POETRY_DATA_DIR={data_dir}",
                            f"VIRTUAL_ENV={venv_dir}",
                            self.path or "poetry",
                            "install",
                            "--no-root",
                            "--sync",
                        ]
                    ),
                ],
                cwd=temp_dir,
            )


def main():
    """Run the benchmark."""
    logging.basicConfig(
        level=logging.INFO,
        format="%(asctime)s %(levelname)s %(message)s",
        datefmt="%Y-%m-%d %H:%M:%S",
    )

    parser = argparse.ArgumentParser(
        description="Benchmark Puffin against other packaging tools."
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
    parser.add_argument(
        "--tool",
        "-t",
        type=str,
        help="The tool(s) to benchmark (typically, `puffin`, `pip-tools` or `poetry`).",
        choices=[tool.value for tool in Tool],
        action="append",
    )
    parser.add_argument(
        "--path",
        "-p",
        type=str,
        help="Optionally, the path to the path, for each tool provided with `--tool`.",
        action="append",
    )
    parser.add_argument(
        "--benchmark",
        "-b",
        type=str,
        help="The benchmark(s) to run.",
        choices=[benchmark.value for benchmark in Benchmark],
        action="append",
    )

    args = parser.parse_args()

    verbose = args.verbose

    requirements_file = os.path.abspath(args.file)
    if not os.path.exists(requirements_file):
        raise ValueError(f"File not found: {requirements_file}")

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

    # Determine the tools to benchmark, based on user input. If no tools were specified,
    # default to the most common choices.
    tools = [Tool(tool) for tool in args.tool] if args.tool is not None else list(Tool)

    # If paths were specified, apply them to the tools.
    paths = args.path or []

    logging.info("Reading requirements from: {}".format(requirements_file))
    logging.info("```")
    with open(args.file, "r") as f:
        for line in f:
            logging.info(line.rstrip())
    logging.info("```")

    for benchmark in benchmarks:
        for tool, path in zip_longest(tools, paths):
            match tool:
                case Tool.PIP_TOOLS:
                    suite = PipTools(path=path)
                case Tool.PUFFIN:
                    suite = Puffin(path=path)
                case Tool.POETRY:
                    suite = Poetry(path=path)
                case _:
                    raise ValueError(f"Invalid tool: {tool}")

            suite.run_benchmark(benchmark, requirements_file, verbose=verbose)


if __name__ == "__main__":
    main()
