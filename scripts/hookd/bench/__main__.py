import subprocess
import sys
import os
import time
from pathlib import Path

PROJECT_DIR = Path(__file__).parent.parent


def new() -> subprocess.Popen:
    env = os.environ.copy()
    # Add the test backends to the Python path
    env["PYTHONPATH"] = PROJECT_DIR / "backends"

    return subprocess.Popen(
        [sys.executable, str(PROJECT_DIR / "hookd.py")],
        stdin=subprocess.PIPE,
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
        text=True,
        env=env,
    )


def send(process, lines):
    process.stdin.write("\n".join(lines) + "\n")


def run(n: int):
    """
    Run a hook N times
    """
    daemon = new()
    for _ in range(n):
        send(daemon, ["run", "ok_backend", "build_wheel", "foo", "", ""])
    daemon.communicate(input="shutdown\n")


def run_no_daemon(n: int):
    """
    Run a hook N times without a daemon
    """
    env = os.environ.copy()
    # Add the test backends to the Python path
    env["PYTHONPATH"] = PROJECT_DIR / "backends"

    for _ in range(n):
        subprocess.run(
            [
                sys.executable,
                "-c",
                'import ok_backend; ok_backend.build_wheel("foo", "", "")',
            ],
            stdin=None,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
            env=env,
        )


if __name__ == "__main__":
    if len(sys.argv) != 2:
        print(
            "Invalid usage. Expected one argument specifying the number of times to execute the hook.",
            file=sys.stderr,
        )
        sys.exit(1)
    try:
        times = int(sys.argv[1])
    except ValueError:
        print(
            "Invalid usage. Expected integer argument specifying the number of times to execute the hook..",
            file=sys.stderr,
        )
        sys.exit(1)

    print(f"Running {times} times", file=sys.stderr)
    start = time.perf_counter()
    run(times)
    end = time.perf_counter()

    print("daemon")
    print(f"\t{(end-start)*1000:.2f}ms total")
    print(f"\t{(end-start)*1000/times:.2f}ms per hook call")

    start = time.perf_counter()
    run_no_daemon(times)
    end = time.perf_counter()

    print("no daemon")
    print(f"\t{(end-start)*1000:.2f}ms total")
    print(f"\t{(end-start)*1000/times:.2f}ms per hook call")
