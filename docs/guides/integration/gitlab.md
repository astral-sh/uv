# Using uv in GitHub Actions

## Installation

uv offers images with shells, you can choose your preferred tag from the [ghcr.io](https://github.com/astral-sh/uv/pkgs/container/uv)

```yaml
variables:
  REPO: 
  UV_VERSION: 0.4
  PYTHON_VERSION: 3.12
  BASE_LAYER: alpine

UV:
  stage: analysis
  image:
    name: ghcr.io/astral-sh/uv:$UV_VERSION-python$PYTHON_VERSION-$BASE_LAYER
  script: >
    cd $CI_PROJECT_DIR
    # your `uv` commands
```


<!-- 
### Using a matrix

If you need to support multiple platforms, you can use a matrix:

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
``` -->


## Syncing and running

<!-- Once uv and Python are installed, the project can be installed with `uv sync` and commands can be
run in the environment with `uv run`:

```yaml title="example.yml"
steps:
  # ... setup up Python and uv ...

  - name: Install the project
    run: uv sync --all-extras --dev

  - name: Run tests
    # For example, using `pytest`
    run: uv run pytest tests
``` -->

## Caching
You can speed up your pipeline by re-using cache files between runs. You can read more on [GitLab's caching here](https://docs.gitlab.com/ee/ci/caching/)
```yaml
UV Install:
  variables:
    UV_CACHE_DIR: /tmp/.uv-cache
  - key:
      files:
        - uv.lock
      paths:
        - $UV_CACHE_DIR
  steps: >
    # Your uv commands
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
variables:
  UV_SYSTEM_PYTHON: 1
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
