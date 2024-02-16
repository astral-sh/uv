import os
import sys
import sysconfig


def detect_virtualenv() -> str:
    """
    Find the virtual environment path for the current Python executable.
    """

    # If it's already set, then just use it
    value = os.getenv("VIRTUAL_ENV")
    if value:
        return value

    # Otherwise, check if we're in a venv
    venv_marker = os.path.join(sys.prefix, "pyvenv.cfg")

    if os.path.exists(venv_marker):
        return sys.prefix

    return ""


def find_uv_bin() -> str:
    """Return the uv binary path."""

    uv_exe = "uv" + sysconfig.get_config_var("EXE")

    path = os.path.join(sysconfig.get_path("scripts"), uv_exe)
    if os.path.isfile(path):
        return path

    if sys.version_info >= (3, 10):
        user_scheme = sysconfig.get_preferred_scheme("user")
    elif os.name == "nt":
        user_scheme = "nt_user"
    elif sys.platform == "darwin" and sys._framework:
        user_scheme = "osx_framework_user"
    else:
        user_scheme = "posix_user"

    path = os.path.join(sysconfig.get_path("scripts", scheme=user_scheme), uv_exe)
    if os.path.isfile(path):
        return path

    raise FileNotFoundError(path)


if __name__ == "__main__":
    uv = os.fsdecode(find_uv_bin())

    env = {}
    venv = detect_virtualenv()
    if venv:
        env["VIRTUAL_ENV"] = venv

    if sys.platform == "win32":
        import subprocess

        completed_process = subprocess.run([uv, *sys.argv[1:]], env=env)
        sys.exit(completed_process.returncode)
    else:
        os.execvpe(uv, [uv, *sys.argv[1:]], env=env)
