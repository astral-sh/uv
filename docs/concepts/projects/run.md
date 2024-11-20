# Running commands in projects

When working on a project, it is installed into virtual environment at `.venv`. This environment is
isolated from the current shell by default, so invocations that require the project, e.g.,
`python -c "import example"`, will fail. Instead, use `uv run` to run commands in the project
environment:

```console
$ uv run python -c "import example"
```

When using `run`, uv will ensure that the project environment is up-to-date before running the given
command.

The given command can be provided by the project environment or exist outside of it, e.g.:

```console
$ # Presuming the project provides `example-cli`
$ uv run example-cli foo

$ # Running a `bash` script that requires the project to be available
$ uv run bash scripts/foo.sh
```

## Requesting additional dependencies

Additional dependencies or different versions of dependencies can be requested per invocation.

The `--with` option is used to include a dependency for the invocation, e.g., to request a different
version of `httpx`:

```console
$ uv run --with httpx==0.26.0 python -c "import httpx; print(httpx.__version__)"
0.26.0
$ uv run --with httpx==0.25.0 python -c "import httpx; print(httpx.__version__)"
0.25.0
```

The requested version will be respected regardless of the project's requirements. For example, even
if the project requires `httpx==0.24.0`, the output above would be the same.

## Running scripts

Scripts that declare inline metadata are automatically executed in environments isolated from the
project. See the [scripts guide](../../guides/scripts.md#declaring-script-dependencies) for more
details.

For example, given a script:

```python title="example.py"
# /// script
# dependencies = [
#   "httpx",
# ]
# ///

import httpx

resp = httpx.get("https://peps.python.org/api/peps.json")
data = resp.json()
print([(k, v["title"]) for k, v in data.items()][:10])
```

The invocation `uv run example.py` would run _isolated_ from the project with only the given
dependencies listed.
