import os
import pathlib
import shlex
import subprocess
import sys

SELF_FILE = pathlib.Path(__file__)
COMMANDS_FILE = SELF_FILE.parent / "commands.sh"


def read_commands() -> list[list[str]]:
    return [
        shlex.split(line)
        for line in COMMANDS_FILE.read_text().splitlines()
        # Skip empty lines and comments
        if line.strip() and not line.strip().startswith("#")
    ]


def run_command(command: list[str]) -> subprocess.CompletedProcess:
    env = os.environ.copy()
    # Prepend either the parent uv path to the PATH or the current directory
    env = {
        **env,
        "PATH": str(
            pathlib.Path(env.get("UV")).parent if "UV" in env else pathlib.Path.cwd()
        )
        + os.pathsep
        + env.get("PATH"),
    }
    return subprocess.run(command, capture_output=True, text=True, env=env)


def report_result(result: subprocess.CompletedProcess):
    print("=============================================")
    print(f"command: {' '.join(result.args)}")
    print(f"exit code: {result.returncode}")
    print()
    print("------- stdout -------")
    print(result.stdout)
    print()
    print("------- stderr -------")
    print(result.stderr)


def main():
    results = [run_command(command) for command in read_commands()]
    failed = sum(result.returncode != 0 for result in results)
    for result in results:
        report_result(result)

    print("=============================================")
    if failed:
        print(f"FAILURE - {failed}/{len(results)} commands failed")
        sys.exit(1)
    else:
        print(f"SUCCESS - {len(results)}/{len(results)} commands succeeded")


main()
