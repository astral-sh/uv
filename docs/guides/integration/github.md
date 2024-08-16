# Using uv in GitHub Actions

## Installation

uv installation differs depending on the platform.

### Unix

```yaml title="example.yml"
name: Example on Unix

jobs:
  uv-example-linux:
    name: python-linux
    runs-on: ubuntu-latest

    steps:
      - uses: actions/checkout@v4

      - name: Set up uv
        # Install uv using the standalone installer
        run: curl -LsSf https://astral.sh/uv/install.sh | sh
```

### Windows

```yaml title="example.yml"
name: Example on Windows

jobs:
  uv-example-windows:
    name: python-windows
    runs-on: windows-latest

    steps:
      - uses: actions/checkout@v4

      - name: Set up uv
        # Install uv using the standalone installer
        run: irm https://astral.sh/uv/install.ps1 | iex
        shell: powershell
```

### Using a matrix

```yaml title="example.yml"
name: Example

jobs:
  uv-example-multiplatform:
    name: python-${{ matrix.os }}

    strategy:
      matrix:
        os:
          - ubuntu-latest
          - windows-latest
          - macos-latest

      fail-fast: false

    runs-on: ${{ matrix.os }}

    steps:
      - uses: actions/checkout@v4

      - name: Set up uv
        if: ${{ matrix.os == 'ubuntu-latest' || matrix.os == 'macos-latest' }}
        run: curl -LsSf https://astral.sh/uv/install.sh | sh

      - name: Set up uv
        if: ${{ matrix.os == 'windows-latest' }}
        run: irm https://astral.sh/uv/install.ps1 | iex
        shell: powershell
```

## Setting up Python

Python can be installed with the `python install` command:

```yaml title="example.yml"
steps:
  # ... setup up uv ...

  - name: Set up Python
    run: uv python install
```

This will respect the Python version pinned in the project.

Or, when using a matrix, as in:

```yaml title="example.yml"
strategy:
  matrix:
    python-version:
      - "3.10"
      - "3.11"
      - "3.12"
```

Provide the version to the `python install` invocation:

```yaml title="example.yml"
steps:
  # ... setup up uv ...

  - name: Set up Python ${{ matrix.python-version }}
    run: uv python install ${{ matrix.python-version }}
```

Alternatively, the official GitHub `setup-python` action can be used. This can be faster, because
GitHub caches the Python versions alongside the runner.

Set the
[`python-version-file`](https://github.com/actions/setup-python/blob/main/docs/advanced-usage.md#using-the-python-version-file-input)
option to use the pinned version for the project:

```yaml title="example.yml"
steps:
  - name: "Set up Python"
    uses: actions/setup-python@v5
    with:
      python-version-file: ".python-version"
```

Or, specify the `pyproject.toml` file to ignore the pin and use the latest version compatible with
the project's `requires-python` constraint:

```yaml title="example.yml"
steps:
  - name: "Set up Python"
    uses: actions/setup-python@v5
    with:
      python-version-file: "pyproject.toml"
```

## Syncing and running

Once uv and Python are installed, the project can be installed with `uv sync` and commands can be
run in the environment with `uv run`:

```yaml title="example.yml"
steps:
  # ... setup up Python and uv ...

  - name: Install the project
    run: uv sync --all-extras --dev

  - name: Run tests
    # For example, using `pytest`
    run: uv run pytest tests
```

## Caching

It may improve CI times to store uv's cache across workflow runs.

The cache can be saved and restored with the official GitHub `cache` action:

```yaml title="example.yml"
jobs:
  install_job:
    env:
      # Configure a constant location for the uv cache
      UV_CACHE_DIR: /tmp/.uv-cache

    steps:
      # ... setup up Python and uv ...

      - name: Restore uv cache
        uses: actions/cache@v4
        with:
          path: /tmp/.uv-cache
          key: uv-${{ runner.os }}-${{ hashFiles('uv.lock') }}
          restore-keys: |
            uv-${{ runner.os }}-${{ hashFiles('uv.lock') }}
            uv-${{ runner.os }}

      # ... install packages, run tests, etc ...

      - name: Minimize uv cache
        run: uv cache prune --ci
```

The `uv cache prune --ci` command is used to reduce the size of the cache and is optimized for CI.
Its effect on performance is dependent on the packages being installed.

!!! tip

    If using `uv pip`, use `requirements.txt` instead of `uv.lock` in the cache key.

## Using `uv pip`

If using the `uv pip` interface instead of the uv project interface, uv requires a virtual
environment by default. To allow installing packages into the system environment, use the `--system`
flag on all `uv` invocations or set the `UV_SYSTEM_PYTHON` variable.

The `UV_SYSTEM_PYTHON` variable can be defined in at different scopes.

Opt-in for the entire workflow by defining it at the top level:

```yaml title="example.yml"
env:
  UV_SYSTEM_PYTHON: 1

jobs: ...
```

Or, opt-in for a specific job in the workflow:

```yaml title="example.yml"
jobs:
  install_job:
    env:
      UV_SYSTEM_PYTHON: 1
    ...
```

Or, opt-in for a specific step in a job:

```yaml title="example.yml"
steps:
  - name: Install requirements
    run: uv pip install -r requirements.txt
    env:
      UV_SYSTEM_PYTHON: 1
```

To opt-out again, the `--no-system` flag can be used in any uv invocation.
