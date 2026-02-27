---
title: Using uv in GitLab CI/CD
description: A guide to using uv in GitLab CI/CD, including installation, setting up Python,
  installing dependencies, and more.
---

# Using uv in GitLab CI/CD

## Using the uv image

Astral provides [Docker images](docker.md#available-images) with uv preinstalled.
Select a variant that is suitable for your workflow.

```yaml title=".gitlab-ci.yml"
variables:
  UV_VERSION: "0.10.6"
  PYTHON_VERSION: "3.12"
  BASE_LAYER: trixie-slim
  # GitLab CI creates a separate mountpoint for the build directory,
  # so we need to copy instead of using hard links.
  UV_LINK_MODE: copy

uv:
  image: ghcr.io/astral-sh/uv:$UV_VERSION-python$PYTHON_VERSION-$BASE_LAYER
  script:
    # your `uv` commands
```

!!! note

    If you are using a distroless image, you have to specify the entrypoint:
    ```yaml
    uv:
      image:
        name: ghcr.io/astral-sh/uv:$UV_VERSION
        entrypoint: [""]
      # ...
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

```yaml title=".gitlab-ci.yml"
variables:
  UV_SYSTEM_PYTHON: 1

# [...]
```

To opt-out again, the `--no-system` flag can be used in any uv invocation.

When persisting the cache, you may want to use `requirements.txt` or `pyproject.toml` as
your cache key files instead of `uv.lock`.

## Publishing to the GitLab PyPI index

`uv publish` can be used to publish to the GitLab PyPI registry, but it will
not update your release with corresponding assets links to the uploaded wheel
files. It also does not provide assets links as they would be required by
`glab release create --assets-links` to connect the wheel files with their
corresponding release item.

The following example of a release job, triggered by a pushed tag, will publish
all wheel files in the GitLab PyPI registry of the project and create assets
links for the corresponding release item.

```yaml title="gitlab-ci.yml"
release-job:
  stage: release
  rules:
    - if: $CI_COMMIT_TAG
  variables:
    GLAB_CHECK_UPDATE: 'no'
    UV_PUBLISH_URL: "${CI_API_V4_URL}/projects/${CI_PROJECT_ID}/packages/pypi"
    UV_PUBLISH_USERNAME: "gitlab-ci-token"
    UV_PUBLISH_PASSWORD: "${CI_JOB_TOKEN}"
  script:
    - uv build --wheel --all-packages
  release:
    tag_name: '$CI_COMMIT_TAG'
    description: '$CI_PROJECT_NAME release $CI_COMMIT_TAG'
  after_script:
    - |
      for package in dist/*.whl; do
          uv publish "$package" && \
          GITLAB_HOST=$CI_SERVER_URL glab release create "$CI_COMMIT_TAG" \
            --repo "$CI_PROJECT_PATH" \
            --assets-links='[{"name": "'$(basename "${package%-*-*-*}")'",
                              "url": "'"$UV_PUBLISH_URL/files/$(sha256sum "$package" | cut --delimiter=' ' --fields=1)/$(basename "$package")"'",
                              "link_type": "package"}]'
      done
```

The items in the PyPI registry of the GitLab project will also be available via
the PyPI registry of the corresponding GitLab group.

Note also the [package request forwarding behaviour](https://docs.gitlab.com/user/packages/pypi_repository/#package-request-forwarding-security-notice)
of GitLab, which might forward your request automatically to `pypi.org`, even
when using the `--default-index` flag.
