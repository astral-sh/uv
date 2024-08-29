# Declaring dependencies

It is best practice to declare dependencies in a static file instead of modifying environments with
ad-hoc installations. Once dependencies are defined, they can be [locked](./compile.md) to create a
consistent, reproducible environment.

## Using `pyproject.toml`

The `pyproject.toml` file is the Python standard for defining configuration for a project.

To define project dependencies in a `pyproject.toml` file:

```toml title="pyproject.toml"
[project]
dependencies = [
  "httpx",
  "ruff>=0.3.0"
]
```

To define optional dependencies in a `pyproject.toml` file:

```toml title="pyproject.toml"
[project.optional-dependencies]
cli = [
  "rich",
  "click",
]
```

Each of the keys defines an "extra", which can be installed using the `--extra` and `--all-extras`
flags or `package[<extra>]` syntax. See the documentation on
[installing packages](./packages.md#installing-packages-from-files) for more details.

See the official
[`pyproject.toml` guide](https://packaging.python.org/en/latest/guides/writing-pyproject-toml/) for
more details on getting started with a `pyproject.toml`.

## Using `requirements.in`

It is also common to use a lightweight `requirements.txt` format to declare the dependencies for the
project. Each requirement is defined on its own line. Commonly, this file is called
`requirements.in` to distinguish it from `requirements.txt` which is used for the locked
dependencies.

To define dependencies in a `requirements.in` file:

```python title="requirements.in"
httpx
ruff>=0.3.0
```

Optional dependencies groups are not supported in this format.
