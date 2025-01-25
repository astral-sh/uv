# Using uv in Bitbucket Pipelines

## Installation

Most common way of setting up a bitbucket pipeline for python is to use a python docker image, which
is faster than installing python manually in any manner.

`uv` provides several docker images with both python and `uv` pre-installed.

So, in `bitbucket-pipelines.yml` file specify the image like following:

```yaml
image:
  name: ghcr.io/astral-sh/uv:<UV_VERSION>-python<PYTHON_VERSION>-<BASE_LAYER>
```

!!! note

    You can find all the available docker images here:
    https://docs.astral.sh/uv/guides/integration/docker/

## Running commands in steps

Now you just need to simply run `uv sync` to install packages.

For example:

```yaml
pipelines:
  default:
    - step:
        script:
          -  # install deps
          - uv sync --all-extras
          -  # run uv commands
          - ...
```

!!! note

    Each step is isolated, so installing packages is required in every step.


    ```yaml
    pipelines:
    default:
        - step:
            name: Lint
            script:
                - # install deps
                - uv sync --group lint
                - # run commands
                - uv run ruff app/
        - step:
            name: Run Tests
            script:
                - # install deps
                - uv sync --group test
                - # run commands
                - uv run pytest tests/
    ```

## Caching

Define a custom cache.

!!! important

    You need to specify the cache directory you choose to `uv` as well.
    Refer: [`uv` caching dir](https://docs.astral.sh/uv/concepts/cache/#cache-directory)

```yaml
image:
  name: ghcr.io/astral-sh/uv:<UV_VERSION>-python<PYTHON_VERSION>-<BASE_LAYER>

definitions:
  caches:
    uv:
      key:
        files:
          - uv.lock
      path: .uv-cache
```

And now add cache to the step.

```yaml
pipelines:
  default:
    - step:
        caches:
          - uv
        script:
          -  # uv commands
```

## Example

Here is a complete example with caching and parallel steps.

```yaml
image:
  name: ghcr.io/astral-sh/uv:latest

definitions:
  caches:
    uv:
      key:
        files:
          - uv.lock
      path: .uv-cache

pipelines:
  default:
    - parallel:
      fail-fast: true
      steps:
        - step:
            name: Lint
            caches:
              - uv
            script:
              -  # install deps
              - uv sync --group lint
              -  # run commands
              - uv run ruff app/
        - step:
            name: Run Tests
            caches:
              - uv
            script:
              -  # install deps
              - uv sync --group test
              -  # run commands
              - uv run pytest tests/
```
