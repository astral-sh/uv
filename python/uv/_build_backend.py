"""
Python shims for the PEP 517 and PEP 660 build backend.

Major imports in this module are required to be lazy:
```
$ hyperfine \
     "/usr/bin/python3 -c \"print('hi')\"" \
     "/usr/bin/python3 -c \"from subprocess import check_call; print('hi')\""
Base: Time (mean ± σ):      11.0 ms ±   1.7 ms    [User: 8.5 ms, System: 2.5 ms]
With import: Time (mean ± σ):      15.2 ms ±   2.0 ms    [User: 12.3 ms, System: 2.9 ms]
Base 1.38 ± 0.28 times faster than with import
```

The same thing goes for the typing module, so we use Python 3.10 type annotations that
don't require importing typing but then quote them so earlier Python version ignore
them while IDEs and type checker can see through the quotes.
"""

TYPE_CHECKING = False
if TYPE_CHECKING:
    from collections.abc import Mapping, Sequence  # noqa:I001
    from typing import Any  # noqa:I001


def warn_config_settings(config_settings: "Mapping[Any, Any] | None" = None) -> None:
    import sys

    if config_settings:
        print("Warning: Config settings are not supported", file=sys.stderr)


def call(
    args: "Sequence[str]", config_settings: "Mapping[Any, Any] | None" = None
) -> str:
    """Invoke a uv subprocess and return the filename from stdout."""
    import shutil
    import subprocess
    import sys

    warn_config_settings(config_settings)
    # Unlike `find_uv_bin`, this mechanism must work according to PEP 517
    uv_bin = shutil.which("uv")
    if uv_bin is None:
        raise RuntimeError("uv was not properly installed")
    # Forward stderr, capture stdout for the filename
    result = subprocess.run([uv_bin, *args], stdout=subprocess.PIPE)
    if result.returncode != 0:
        sys.exit(result.returncode)
    # If there was extra stdout, forward it (there should not be extra stdout)
    stdout = result.stdout.decode("utf-8").strip().splitlines(keepends=True)
    sys.stdout.writelines(stdout[:-1])
    # Fail explicitly instead of an irrelevant stacktrace
    if not stdout:
        print("uv subprocess did not return a filename on stdout", file=sys.stderr)
        sys.exit(1)
    return stdout[-1].strip()


def build_sdist(
    sdist_directory: str, config_settings: "Mapping[Any, Any] | None" = None
) -> str:
    """PEP 517 hook `build_sdist`."""
    args = ["build-backend", "build-sdist", sdist_directory]
    return call(args, config_settings)


def build_wheel(
    wheel_directory: str,
    config_settings: "Mapping[Any, Any] | None" = None,
    metadata_directory: "str | None" = None,
) -> str:
    """PEP 517 hook `build_wheel`."""
    args = ["build-backend", "build-wheel", wheel_directory]
    if metadata_directory:
        args.extend(["--metadata-directory", metadata_directory])
    return call(args, config_settings)


def get_requires_for_build_sdist(
    config_settings: "Mapping[Any, Any] | None" = None,
) -> "Sequence[str]":
    """PEP 517 hook `get_requires_for_build_sdist`."""
    warn_config_settings(config_settings)
    return []


def get_requires_for_build_wheel(
    config_settings: "Mapping[Any, Any] | None" = None,
) -> "Sequence[str]":
    """PEP 517 hook `get_requires_for_build_wheel`."""
    warn_config_settings(config_settings)
    return []


def prepare_metadata_for_build_wheel(
    metadata_directory: str, config_settings: "Mapping[Any, Any] | None" = None
) -> str:
    """PEP 517 hook `prepare_metadata_for_build_wheel`."""
    args = ["build-backend", "prepare-metadata-for-build-wheel", metadata_directory]
    return call(args, config_settings)


def build_editable(
    wheel_directory: str,
    config_settings: "Mapping[Any, Any] | None" = None,
    metadata_directory: "str | None" = None,
) -> str:
    """PEP 660 hook `build_editable`."""
    args = ["build-backend", "build-editable", wheel_directory]
    if metadata_directory:
        args.extend(["--metadata-directory", metadata_directory])
    return call(args, config_settings)


def get_requires_for_build_editable(
    config_settings: "Mapping[Any, Any] | None" = None,
) -> "Sequence[str]":
    """PEP 660 hook `get_requires_for_build_editable`."""
    warn_config_settings(config_settings)
    return []


def prepare_metadata_for_build_editable(
    metadata_directory: str, config_settings: "Mapping[Any, Any] | None" = None
) -> str:
    """PEP 660 hook `prepare_metadata_for_build_editable`."""
    args = ["build-backend", "prepare-metadata-for-build-editable", metadata_directory]
    return call(args, config_settings)
