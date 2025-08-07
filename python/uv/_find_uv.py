from __future__ import annotations

import os
import sys
import sysconfig


def find_uv_bin() -> str:
    """Return the uv binary path."""

    uv_exe = "uv" + sysconfig.get_config_var("EXE")

    # Search in the scripts directory for the current prefix
    path = os.path.join(sysconfig.get_path("scripts"), uv_exe)
    if os.path.isfile(path):
        return path

    # If in a virtual environment, also search in the base prefix's scripts directory
    if sys.prefix != sys.base_prefix:
        path = os.path.join(
            sysconfig.get_path("scripts", vars={"base": sys.base_prefix}), uv_exe
        )
        if os.path.isfile(path):
            return path

    # Search in the user scheme scripts directory, e.g., `~/.local/bin`
    path = os.path.join(sysconfig.get_path("scripts", scheme=_user_scheme()), uv_exe)
    if os.path.isfile(path):
        return path

    # Search in `bin` adjacent to package root (as created by `pip install --target`).
    pkg_root = os.path.dirname(os.path.dirname(__file__))
    target_path = os.path.join(pkg_root, "bin", uv_exe)
    if os.path.isfile(target_path):
        return target_path

    raise FileNotFoundError(path)


def _user_scheme() -> str:
    if sys.version_info >= (3, 10):
        user_scheme = sysconfig.get_preferred_scheme("user")
    elif os.name == "nt":
        user_scheme = "nt_user"
    elif sys.platform == "darwin" and sys._framework:
        user_scheme = "osx_framework_user"
    else:
        user_scheme = "posix_user"
    return user_scheme
