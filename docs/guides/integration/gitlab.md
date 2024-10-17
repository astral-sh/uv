# Using uv in GitLab CI/CD

## Using the uv image

Astral provides [Docker images](docker.md#available-images) with uv preinstalled.
Select a variant that is suitable for your workflow.

```yaml title="gitlab-ci.yml"
variables:
  UV_VERSION: 0.4
  PYTHON_VERSION: 3.12
  BASE_LAYER: bookworm-slim

stages:
  - analysis

uv:
  stage: analysis
  image: ghcr.io/astral-sh/uv:$UV_VERSION-python$PYTHON_VERSION-$BASE_LAYER
  script:
    # your `uv` commands
```

## Caching

Persisting the uv cache between workflow runs can improve performance.

```yaml
uv-install:
  variables:
    UV_CACHE_DIR: .uv-cache
  cache:
    - key:
        files:
          - uv.lock
      paths:
        - $UV_CACHE_DIR
  script:
    # Your `uv` commands
    - uv cache prune --ci
```

See the [GitLab caching documentation](https://docs.gitlab.com/ee/ci/caching/) for more details on
configuring caching.

Using `uv cache prune --ci` at the end of the job is recommended to reduce cache size. See the [uv
cache documentation](../../concepts/cache.md#caching-in-continuous-integration) for more details.

## Using `uv pip`

If using the `uv pip` interface instead of the uv project interface, uv requires a virtual
environment by default. To allow installing packages into the system environment, use the `--system`
flag on all uv invocations or set the `UV_SYSTEM_PYTHON` variable.

The `UV_SYSTEM_PYTHON` variable can be defined in at different scopes. You can read more about
how [variables and their precedence works in GitLab here](https://docs.gitlab.com/ee/ci/variables/)

Opt-in for the entire workflow by defining it at the top level:

```yaml title="gitlab-ci.yml"
variables:
  UV_SYSTEM_PYTHON: 1

# [...]
```

To opt-out again, the `--no-system` flag can be used in any uv invocation.

When persisting the cache, you may want to use `requirement.txt` or `pyproject.toml` as
your cache key files instead of `uv.lock`.
