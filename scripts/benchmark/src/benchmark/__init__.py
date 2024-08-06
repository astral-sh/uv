import shlex
import subprocess
import typing


class Command(typing.NamedTuple):
    name: str
    """The name of the command to benchmark."""

    prepare: str | None
    """The command to run before each benchmark run."""

    command: list[str]
    """The command to benchmark."""


class Hyperfine(typing.NamedTuple):
    name: str
    """The benchmark to run."""

    commands: list[Command]
    """The commands to benchmark."""

    warmup: int | None
    """The number of warmup runs to perform."""

    min_runs: int | None
    """The minimum number of runs to perform."""

    runs: int | None
    """The number of runs to perform."""

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
            args.append(f"{self.name}.json")

        # Preamble: benchmark-wide setup.
        if self.verbose:
            args.append("--show-output")
        if self.warmup is not None:
            args.append("--warmup")
            args.append(str(self.warmup))
        if self.min_runs is not None:
            args.append("--min-runs")
            args.append(str(self.min_runs))
        if self.runs is not None:
            args.append("--runs")
            args.append(str(self.runs))

        # Add all command names,
        for command in self.commands:
            args.append("--command-name")
            args.append(command.name)

        # Add all prepare statements.
        for command in self.commands:
            args.append("--prepare")
            args.append(command.prepare or "")

        # Add all commands.
        for command in self.commands:
            args.append(shlex.join(command.command))

        subprocess.check_call(args)
