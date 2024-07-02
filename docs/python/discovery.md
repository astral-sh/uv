# Python discovery

uv itself does not depend on Python, but it does need to locate a Python environment to (1)
install dependencies into the environment and (2) build source distributions.

## Environment mutating commands

When running a command that mutates an environment such as `uv pip sync` or `uv pip install`,
uv will search for a virtual environment in the following order:

- An activated virtual environment based on the `VIRTUAL_ENV` environment variable.
- An activated Conda environment based on the `CONDA_PREFIX` environment variable.
- A virtual environment at `.venv` in the current directory, or in the nearest parent directory.

If no virtual environment is found, uv will prompt the user to create one in the current
directory via `uv venv`.

If the `--system` flag is included, uv will skip virtual environments and search for:

- The Python interpreter available as `python3` on macOS and Linux, or `python.exe` on Windows.
- On Windows, the Python interpreter returned by `py --list-paths` that matches the requested
  version.

## Commands that need an interpreter

When running a command that needs an interpreter but does not mutate the environment such as `uv pip compile`,
uv does not _require_ a virtual environment and will search for a Python interpreter in the following order:

- An activated virtual environment based on the `VIRTUAL_ENV` environment variable.
- An activated Conda environment based on the `CONDA_PREFIX` environment variable.
- A virtual environment at `.venv` in the current directory, or in the nearest parent directory.
- The Python interpreter available as `python3` on macOS and Linux, or `python.exe` on Windows.

If a `--python-version` is provided to `uv pip compile` (e.g., `--python-version=3.7`), uv will
search for a Python interpreter matching that version in the following order:

- An activated virtual environment based on the `VIRTUAL_ENV` environment variable.
- An activated Conda environment based on the `CONDA_PREFIX` environment variable.
- A virtual environment at `.venv` in the current directory, or in the nearest parent directory.
- The Python interpreter available as, e.g., `python3.7` on macOS and Linux.
- The Python interpreter available as `python3` on macOS and Linux, or `python.exe` on Windows.
- On Windows, the Python interpreter returned by `py --list-paths` that matches the requested
  version.

These commands may create ephemeral virtual environments with the interpreter.
