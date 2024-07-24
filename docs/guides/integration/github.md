# Using uv in GitHub Actions

## Installation

uv installation differs depending on the platform.

### on Unix

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

### on Windows

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

Alternatively, the official GitHub `setup-python` action can be used. This is generally faster, but will not respect the project's pinned Python version.

```yaml title="example.yml"
steps:
  - name: "Set up Python"
    uses: actions/setup-python@v5
    with:
      python-version: 3.12
```

## Syncing and running

Once uv and Python are installed, the project can be installed with `uv sync` and commands can be run in the environment with `uv run`:

```yaml title="example.yml"
steps:
  # ... setup up Python and uv ...

  - name: Install the project
    run: uv sync --all-extras --dev

  - name: Run tests
    # For example, using `pytest`
    run: uv run -- pytest tests
```

## Using `uv pip`

If using the `uv pip` interface instead of the uv project interface, uv requires a virtual environment by default. To allow installing packages into the system environment, use the `--system` flag on all `uv` invocations or set the `UV_SYSTEM_PYTHON` variable.

### Setting `UV_SYSTEM_PYTHON`

`UV_SYSTEM_PYTHON` variable can be defined in various scopes:

i. Workflow-wide Environment Variables

Set the variable for the entire workflow by defining it at the top level:

```yaml title="example.yml"
env:
  UV_SYSTEM_PYTHON: 1

jobs: ...
```

ii. Job-specific Environment Variables

Set the variable for a specific job within the workflow:

```yaml title="example.yml"
jobs:
  install_job:
    env:
      UV_SYSTEM_PYTHON: 1
    ...
```

Or using a shell command:

```yaml title="example.yml"
steps:
  - name: Allow uv to use the system Python by default
    run: echo "UV_SYSTEM_PYTHON=1" >> $GITHUB_ENV
```

iii. Step-specific Environment Variables

Set the variable for a specific step:

```yaml title="example.yml"
steps:
  - name: Install requirements
    run: uv pip install -r requirements.txt
    env:
      UV_SYSTEM_PYTHON: 1
```

Now, `uv pip` can modify the system environment without creating and activating a virtual environment.

```yaml title="example.yml"
steps:
  # ... setup up Python and uv ...

  - name: Install requirements
    run: uv pip install -r requirements.txt

  - name: Run tests
    run: pytest tests
```
