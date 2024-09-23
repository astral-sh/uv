# Using uv in GitLab CI/CD

uv offers images with shells, you can choose your preferred tag from the
[ghcr.io](https://github.com/astral-sh/uv/pkgs/container/uv)

```yaml title="gitlab-ci.yml
variables:
  UV_VERSION: 0.4
  PYTHON_VERSION: 3.12
  BASE_LAYER: alpine

stages:
  - analysis

UV:
  stage: analysis
  image:
    name: ghcr.io/astral-sh/uv:$UV_VERSION-python$PYTHON_VERSION-$BASE_LAYER
  script: >
    cd $CI_PROJECT_DIR
    # your `uv` commands
```

## Caching

You can speed up your pipeline by re-using cache files between runs. You can read more on
[GitLab's caching here](https://docs.gitlab.com/ee/ci/caching/)

```yaml
UV Install:
  variables:
    UV_CACHE_DIR: /tmp/.uv-cache
  cache:
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

The `UV_SYSTEM_PYTHON` variable can be defined in at different scopes. You can read more about
how [variables and their precedence works in GitLab here](https://docs.gitlab.com/ee/ci/variables/)

Opt-in for the entire workflow by defining it at the top level:

```yaml title="gitlab-ci.yml"
variables:
  UV_SYSTEM_PYTHON: 1

# [...]
```

To opt-out again, the `--no-system` flag can be used in any uv invocation.
