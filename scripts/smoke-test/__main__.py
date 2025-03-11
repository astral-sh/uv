import os
import pathlib
import subprocess
import sys
from typing import Generator

SELF_FILE = pathlib.Path(__file__)


def find_test_scripts() -> Generator[pathlib.Path, None, None]:
    for child in SELF_FILE.parent.iterdir():
        if child.suffix == ".sh":
            yield child


def run_script(script: pathlib.Path) -> subprocess.CompletedProcess:
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
    return subprocess.run(
        ["sh", str(script.absolute())], capture_output=True, text=True, env=env
    )


def report_result(result: subprocess.CompletedProcess):
    print("=============================================")
    print(f"script: {result.args[-1].rsplit(os.path.sep, 1)[-1]}")
    print(f"exit code: {result.returncode}")
    print()
    print("------- stdout -------")
    print(result.stdout)
    print()
    print("------- stderr -------")
    print(result.stderr)


def main():
    results = [run_script(script) for script in find_test_scripts()]
    failed = sum(result.returncode != 0 for result in results)
    for result in results:
        report_result(result)

    print("=============================================")
    if failed:
        print(f"FAILURE - {failed}/{len(results)} scripts failed")
        sys.exit(1)
    else:
        print(f"SUCCESS - {len(results)}/{len(results)} scripts succeeded")


main()
