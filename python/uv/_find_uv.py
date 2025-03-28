from __future__ import annotations

import os
import sys
import sysconfig


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

    # Search in `bin` adjacent to package root (as created by `pip install --target`).
    pkg_root = os.path.dirname(os.path.dirname(__file__))
    # Search for `bin` when inside a venv created with use system-site-packages and uv is installed in the system.
    pkg_root2 = os.path.dirname(os.path.dirname(os.path.dirname(pkg_root)))
    for path in (pkg_root, pkg_root2):
        target_path = os.path.join(path, "bin", uv_exe)
        if os.path.isfile(target_path):
            return target_path

    raise FileNotFoundError(path)
