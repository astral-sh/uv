"""Benchmark the uv `tool` interface against other packaging tools.

For example, to benchmark uv against pipx, run the following from the
`scripts/benchmark` directory:

    uv run tools --uv --pipx
"""

import abc
import argparse
import enum
import logging
import os.path
import tempfile

from benchmark import Command, Hyperfine

TOOL = "flask"


class Benchmark(enum.Enum):
    """Enumeration of the benchmarks to run."""

    INSTALL_COLD = "install-cold"
    INSTALL_WARM = "install-warm"
    RUN = "run"


class Suite(abc.ABC):
    """Abstract base class for packaging tools."""

    def command(self, benchmark: Benchmark, *, cwd: str) -> Command | None:
        """Generate a command to benchmark a given tool."""
        match benchmark:
            case Benchmark.INSTALL_COLD:
                return self.install_cold(cwd=cwd)
            case Benchmark.INSTALL_WARM:
                return self.install_warm(cwd=cwd)
            case Benchmark.RUN:
                return self.run(cwd=cwd)
            case _:
                raise ValueError(f"Invalid benchmark: {benchmark}")

    @abc.abstractmethod
    def install_cold(self, *, cwd: str) -> Command | None:
        """Resolve a set of dependencies using pip-tools, from a cold cache.

        The resolution is performed from scratch, i.e., without an existing lockfile,
        and the cache directory is cleared between runs.
        """

    @abc.abstractmethod
    def install_warm(self, *, cwd: str) -> Command | None:
        """Resolve a set of dependencies using pip-tools, from a warm cache.

        The resolution is performed from scratch, i.e., without an existing lockfile;
        however, the cache directory is _not_ cleared between runs.
        """

    @abc.abstractmethod
    def run(self, *, cwd: str) -> Command | None:
        """Resolve a modified lockfile using pip-tools, from a warm cache.

        The resolution is performed with an existing lockfile, and the cache directory
        is _not_ cleared between runs. However, a new dependency is added to the set
        of input requirements, which does not appear in the lockfile.
        """


class Pipx(Suite):
    def __init__(self, path: str | None = None) -> None:
        self.name = path or "pipx"
        self.path = path or "pipx"

    def install_cold(self, *, cwd: str) -> Command | None:
        home_dir = os.path.join(cwd, "home")
        bin_dir = os.path.join(cwd, "bin")
        man_dir = os.path.join(cwd, "man")

        # pipx uses a shared virtualenv directory in `${PIPX_HOME}/shared`, which
        # contains pip. If we remove `${PIPX_HOME}/shared`, we're simulating the _first_
        # pipx invocation on a machine, rather than `pipx run` with a cold cache. So,
        # instead, we only remove the installed tools, rather than the shared
        # dependencies.
        venvs_dir = os.path.join(home_dir, "venvs")

        return Command(
            name=f"{self.name} ({Benchmark.INSTALL_COLD.value})",
            prepare=f"rm -rf {venvs_dir} && rm -rf {bin_dir} && rm -rf {man_dir}",
            command=[
                f"PIPX_HOME={home_dir}",
                f"PIPX_BIN_DIR={bin_dir}",
                f"PIPX_MAN_DIR={man_dir}",
                self.path,
                "install",
                "--pip-args=--no-cache-dir",
                TOOL,
            ],
        )

    def install_warm(self, *, cwd: str) -> Command | None:
        home_dir = os.path.join(cwd, "home")
        bin_dir = os.path.join(cwd, "bin")
        man_dir = os.path.join(cwd, "man")

        # pipx uses a shared virtualenv directory in `${PIPX_HOME}/shared`, which
        # contains pip. If we remove `${PIPX_HOME}/shared`, we're simulating the _first_
        # pipx invocation on a machine, rather than `pipx run` with a cold cache. So,
        # instead, we only remove the installed tools, rather than the shared
        # dependencies.
        venvs_dir = os.path.join(home_dir, "venvs")

        return Command(
            name=f"{self.name} ({Benchmark.INSTALL_WARM.value})",
            prepare=f"rm -rf {venvs_dir} && rm -rf {bin_dir} && rm -rf {man_dir}",
            command=[
                f"PIPX_HOME={home_dir}",
                f"PIPX_BIN_DIR={bin_dir}",
                f"PIPX_MAN_DIR={man_dir}",
                self.path,
                "install",
                TOOL,
            ],
        )

    def run(self, *, cwd: str) -> Command | None:
        home_dir = os.path.join(cwd, "home")
        bin_dir = os.path.join(cwd, "bin")
        man_dir = os.path.join(cwd, "man")

        return Command(
            name=f"{self.name} ({Benchmark.RUN.value})",
            prepare="",
            command=[
                f"PIPX_HOME={home_dir}",
                f"PIPX_BIN_DIR={bin_dir}",
                f"PIPX_MAN_DIR={man_dir}",
                self.path,
                "install",
                TOOL,
            ],
        )


class Uv(Suite):
    def __init__(self, *, path: str | None = None) -> Command | None:
        """Initialize a uv benchmark."""
        self.name = path or "uv"
        self.path = path or os.path.join(
            os.path.dirname(
                os.path.dirname(
                    os.path.dirname(
                        os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
                    )
                )
            ),
            "target",
            "release",
            "uv",
        )

    def install_cold(self, *, cwd: str) -> Command | None:
        bin_dir = os.path.join(cwd, "bin")
        tool_dir = os.path.join(cwd, "tool")
        cache_dir = os.path.join(cwd, ".cache")

        return Command(
            name=f"{self.name} ({Benchmark.INSTALL_COLD.value})",
            prepare=f"rm -rf {bin_dir} && rm -rf {tool_dir} && rm -rf {cache_dir}",
            command=[
                f"XDG_BIN_HOME={bin_dir}",
                f"UV_TOOL_DIR={tool_dir}",
                self.path,
                "tool",
                "install",
                "--cache-dir",
                cache_dir,
                "--",
                TOOL,
            ],
        )

    def install_warm(self, *, cwd: str) -> Command | None:
        bin_dir = os.path.join(cwd, "bin")
        tool_dir = os.path.join(cwd, "tool")
        cache_dir = os.path.join(cwd, ".cache")

        return Command(
            name=f"{self.name} ({Benchmark.INSTALL_WARM.value})",
            prepare=f"rm -rf {bin_dir} && rm -rf {tool_dir}",
            command=[
                f"XDG_BIN_HOME={bin_dir}",
                f"UV_TOOL_DIR={tool_dir}",
                self.path,
                "tool",
                "install",
                "--cache-dir",
                cache_dir,
                "--",
                TOOL,
            ],
        )

    def run(self, *, cwd: str) -> Command | None:
        bin_dir = os.path.join(cwd, "bin")
        tool_dir = os.path.join(cwd, "tool")
        cache_dir = os.path.join(cwd, ".cache")

        return Command(
            name=f"{self.name} ({Benchmark.RUN.value})",
            prepare="",
            command=[
                f"XDG_BIN_HOME={bin_dir}",
                f"UV_TOOL_DIR={tool_dir}",
                self.path,
                "tool",
                "run",
                "--cache-dir",
                cache_dir,
                "--",
                TOOL,
                "--version",
            ],
        )


def main():
    """Run the benchmark."""
    parser = argparse.ArgumentParser(
        description="Benchmark uv against other packaging tools."
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
        "--runs",
        type=int,
        help="The number of runs to perform.",
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
        "--pipx",
        help="Whether to benchmark `pipx`.",
        action="store_true",
    )
    parser.add_argument(
        "--uv",
        help="Whether to benchmark uv (assumes a uv binary exists at `./target/release/uv`).",
        action="store_true",
    )
    parser.add_argument(
        "--pipx-path",
        type=str,
        help="Path(s) to the `pipx` binary to benchmark.",
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
    runs = args.runs

    # Determine the tools to benchmark, based on the user-provided arguments.
    suites = []
    if args.pipx:
        suites.append(Pipx())
    if args.uv:
        suites.append(Uv())
    for path in args.pipx_path or []:
        suites.append(Pipx(path=path))
    for path in args.uv_path or []:
        suites.append(Uv(path=path))

    # If no tools were specified, benchmark all tools.
    if not suites:
        suites = [
            Pipx(),
            Uv(),
        ]

    # Determine the benchmarks to run, based on user input.
    benchmarks = (
        [Benchmark(benchmark) for benchmark in args.benchmark]
        if args.benchmark is not None
        else list(Benchmark)
    )

    with tempfile.TemporaryDirectory() as cwd:
        for benchmark in benchmarks:
            # Generate the benchmark command for each tool.
            commands = [
                command
                for suite in suites
                if (command := suite.command(benchmark, cwd=tempfile.mkdtemp(dir=cwd)))
            ]

            if commands:
                hyperfine = Hyperfine(
                    name=str(benchmark.value),
                    commands=commands,
                    warmup=warmup,
                    min_runs=min_runs,
                    runs=runs,
                    verbose=verbose,
                    json=json,
                )
                hyperfine.run()


if __name__ == "__main__":
    main()
