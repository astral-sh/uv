"""Benchmark Puffin against other packaging tools.

This script assumes that `pip`, `pip-tools`, `virtualenv`, and `hyperfine` are
installed, and that a Puffin release builds exists at `./target/release/puffin`
(relative to the repository root).

Example usage: python bench.py -f requirements.in -t puffin -t pip-tools
"""
import abc
import argparse
import enum
import logging
import os.path
import shlex
import subprocess
import tempfile

WARMUP = 3
MIN_RUNS = 10


class Tool(enum.Enum):
    """Enumeration of the tools to benchmark."""

    PIP_TOOLS = "pip-tools"
    PUFFIN = "puffin"


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
    def resolve_cold(self, requirements_file: str, *, verbose: bool) -> None:
        with tempfile.mkdtemp() as temp_dir:
            cache_dir = os.path.join(temp_dir, ".cache")
            output_file = os.path.join(temp_dir, "requirements.txt")

            subprocess.check_call(
                [
                    "hyperfine",
                    *(["--show-output"] if verbose else []),
                    "--command-name",
                    f"{Tool.PIP_TOOLS.value} ({Benchmark.RESOLVE_COLD.value})",
                    "--warmup",
                    str(WARMUP),
                    "--min-runs",
                    str(MIN_RUNS),
                    "--prepare",
                    f"rm -rf {temp_dir} && rm -f {output_file}",
                    shlex.join(
                        [
                            "pip-compile",
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
        with tempfile.mkdtemp() as temp_dir:
            cache_dir = os.path.join(temp_dir, ".cache")
            output_file = os.path.join(temp_dir, "requirements.txt")

            subprocess.check_call(
                [
                    "hyperfine",
                    *(["--show-output"] if verbose else []),
                    "--command-name",
                    f"{Tool.PIP_TOOLS.value} ({Benchmark.RESOLVE_WARM.value})",
                    "--warmup",
                    str(WARMUP),
                    "--min-runs",
                    str(MIN_RUNS),
                    "--prepare",
                    f"rm -f {output_file}",
                    shlex.join(
                        [
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
        with tempfile.mkdtemp() as temp_dir:
            cache_dir = os.path.join(temp_dir, ".cache")
            venv_dir = os.path.join(temp_dir, ".venv")

            subprocess.check_call(
                [
                    "hyperfine",
                    *(["--show-output"] if verbose else []),
                    "--command-name",
                    f"{Tool.PIP_TOOLS.value} ({Benchmark.INSTALL_COLD.value})",
                    "--warmup",
                    str(WARMUP),
                    "--min-runs",
                    str(MIN_RUNS),
                    "--prepare",
                    f"rm -rf {cache_dir} && virtualenv --clear -p 3.10 {venv_dir}",
                    shlex.join(
                        [
                            "pip-sync",
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
        with tempfile.mkdtemp() as temp_dir:
            cache_dir = os.path.join(temp_dir, ".cache")
            venv_dir = os.path.join(temp_dir, ".venv")

            subprocess.check_call(
                [
                    "hyperfine",
                    *(["--show-output"] if verbose else []),
                    "--command-name",
                    f"{Tool.PIP_TOOLS.value} ({Benchmark.INSTALL_WARM.value})",
                    "--warmup",
                    str(WARMUP),
                    "--min-runs",
                    str(MIN_RUNS),
                    "--prepare",
                    f"virtualenv --clear -p 3.10 {venv_dir}",
                    shlex.join(
                        [
                            "pip-sync",
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
    def resolve_cold(self, requirements_file: str, *, verbose: bool) -> None:
        with tempfile.mkdtemp() as temp_dir:
            cache_dir = os.path.join(temp_dir, ".cache")
            output_file = os.path.join(temp_dir, "requirements.txt")

            subprocess.check_call(
                [
                    "hyperfine",
                    *(["--show-output"] if verbose else []),
                    "--command-name",
                    f"{Tool.PUFFIN.value} ({Benchmark.RESOLVE_COLD.value})",
                    "--warmup",
                    str(WARMUP),
                    "--min-runs",
                    str(MIN_RUNS),
                    "--prepare",
                    f"rm -rf {temp_dir} && rm -f {output_file}",
                    shlex.join(
                        [
                            os.path.join(
                                os.path.dirname(
                                    os.path.dirname(os.path.abspath(__file__))
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
        with tempfile.mkdtemp() as temp_dir:
            cache_dir = os.path.join(temp_dir, ".cache")
            output_file = os.path.join(temp_dir, "requirements.txt")

            subprocess.check_call(
                [
                    "hyperfine",
                    *(["--show-output"] if verbose else []),
                    "--command-name",
                    f"{Tool.PUFFIN.value} ({Benchmark.RESOLVE_WARM.value})",
                    "--warmup",
                    str(WARMUP),
                    "--min-runs",
                    str(MIN_RUNS),
                    "--prepare",
                    f"rm -f {output_file}",
                    shlex.join(
                        [
                            os.path.join(
                                os.path.dirname(
                                    os.path.dirname(os.path.abspath(__file__))
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
        with tempfile.mkdtemp() as temp_dir:
            cache_dir = os.path.join(temp_dir, ".cache")
            venv_dir = os.path.join(temp_dir, ".venv")

            subprocess.check_call(
                [
                    "hyperfine",
                    *(["--show-output"] if verbose else []),
                    "--command-name",
                    f"{Tool.PUFFIN.value} ({Benchmark.INSTALL_COLD.value})",
                    "--warmup",
                    str(WARMUP),
                    "--min-runs",
                    str(MIN_RUNS),
                    "--prepare",
                    f"rm -rf {cache_dir} && virtualenv --clear -p 3.10 {venv_dir}",
                    shlex.join(
                        [
                            f"VIRTUAL_ENV={venv_dir}",
                            os.path.join(
                                os.path.dirname(
                                    os.path.dirname(os.path.abspath(__file__))
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
        with tempfile.mkdtemp() as temp_dir:
            cache_dir = os.path.join(temp_dir, ".cache")
            venv_dir = os.path.join(temp_dir, ".venv")

            subprocess.check_call(
                [
                    "hyperfine",
                    *(["--show-output"] if verbose else []),
                    "--command-name",
                    f"{Tool.PUFFIN.value} ({Benchmark.INSTALL_WARM.value})",
                    "--warmup",
                    str(WARMUP),
                    "--min-runs",
                    str(MIN_RUNS),
                    "--prepare",
                    f"virtualenv --clear -p 3.10 {venv_dir}",
                    shlex.join(
                        [
                            f"VIRTUAL_ENV={venv_dir}",
                            os.path.join(
                                os.path.dirname(
                                    os.path.dirname(os.path.abspath(__file__))
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
        "-f", "--file", type=str, help="The file to read the dependencies from."
    )
    parser.add_argument(
        "--verbose", "-v", action="store_true", help="Print verbose output."
    )
    parser.add_argument(
        "--tool",
        "-t",
        type=str,
        help="The tool(s) to benchmark.",
        choices=[tool.value for tool in Tool],
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

    requirements_file = os.path.abspath(args.file)
    verbose = args.verbose
    tools = [Tool(tool) for tool in args.tool] if args.tool is not None else list(Tool)
    benchmarks = (
        [Benchmark(benchmark) for benchmark in args.benchmark]
        if args.benchmark is not None
        else [Benchmark.RESOLVE_COLD, Benchmark.RESOLVE_WARM]
        if requirements_file.endswith(".in")
        else [Benchmark.INSTALL_COLD, Benchmark.INSTALL_WARM]
        if requirements_file.endswith(".txt")
        else list(Benchmark)
    )

    logging.info(
        "Benchmarks: {}".format(
            ", ".join([benchmark.value for benchmark in benchmarks])
        )
    )
    logging.info("Tools: {}".format(", ".join([tool.value for tool in tools])))

    logging.info("Reading requirements from: {}".format(requirements_file))
    logging.info("```")
    with open(args.file, "r") as f:
        for line in f:
            logging.info(line.rstrip())
    logging.info("```")

    for benchmark in benchmarks:
        for tool in tools:
            match tool:
                case Tool.PIP_TOOLS:
                    suite = PipTools()
                case Tool.PUFFIN:
                    suite = Puffin()
                case _:
                    raise ValueError(f"Unknown tool: {tool}")

            suite.run_benchmark(benchmark, requirements_file, verbose=verbose)


if __name__ == "__main__":
    main()
