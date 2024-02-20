import os
import sys

from uv import find_uv_bin


def _detect_virtualenv() -> str:
    """
    Find the virtual environment path for the current Python executable.
    """

    # If it's already set, then use it
    if value := os.getenv("VIRTUAL_ENV"):
        return value

    # Otherwise, check if we're in a venv
    venv_marker = os.path.join(sys.prefix, "pyvenv.cfg")
    if os.path.exists(venv_marker):
        return sys.prefix

    return ""


def _run() -> None:
    uv = os.fsdecode(find_uv_bin())

    env = os.environ.copy()
    if venv := _detect_virtualenv():
        env.setdefault("VIRTUAL_ENV", venv)

    cmd = [uv, *sys.argv[1:]]

    if sys.platform == "win32":
        import subprocess

        completed_process = subprocess.run(cmd, env=env)
        raise SystemExit(completed_process.returncode)

    os.execvpe(uv, cmd, env=env)


if __name__ == "__main__":
    _run()
