# Using uv in Docker

## Running in Docker

A Docker image is published with a built version of uv available. To run a uv command in a
container:

```console
$ docker run ghcr.io/astral-sh/uv --help
```

## Installing uv

uv can be installed by copying from the official Docker image:

```dockerfile title="Dockerfile"
FROM python:3.12-slim-bullseye
COPY --from=ghcr.io/astral-sh/uv:latest /uv /bin/uv
```

Or with the standalone installer:

```dockerfile title="Dockerfile"
FROM python:3.12-slim-bullseye
RUN apt-get update && apt-get install -y curl --no-install-recommends
RUN curl -LsSf https://astral.sh/uv/install.sh | sh
ENV PATH="/root/.cargo/bin/:$PATH"
```

Note this requires `curl` to be available.

In either case, it is best practice to pin to a specific uv version.

## Installing a package

Once uv is installed in an image, it can be used to install some packages.

The system Python environment is safe to use this context, since a container is already isolated.
The `--system` flag can be used to install in the system environment:

```dockerfile title="Dockerfile"
RUN uv pip install --system ruff
```

To use the system Python environment by default, set the `UV_SYSTEM_PYTHON` variable:

```dockerfile title="Dockerfile"
ENV UV_SYSTEM_PYTHON=1
```

Alternatively, a virtual environment can be created and activated:

```dockerfile title="Dockerfile"
RUN uv venv /opt/venv
# Use the virtual environment automatically
ENV VIRTUAL_ENV=/opt/venv
# Place entry points in the environment at the front of the path
ENV PATH="/opt/venv/bin:$PATH"
```

When using a virtual environment, the `--system` flag should be omitted from uv invocations:

```dockerfile title="Dockerfile"
RUN uv pip install ruff
```

## Installing requirements

To install requirements files, copy them into the container:

```dockerfile title="Dockerfile"
COPY requirements.txt .
RUN uv pip install -r requirements.txt
```

## Installing a project

When installing a project alongside requirements, it is prudent to separate copying the requirements
from the rest of the source code. This allows the dependencies of the project (which do not change
often) to be cached separately from the project itself (which changes very frequently).

```dockerfile title="Dockerfile"
COPY pyproject.toml .
RUN uv pip install -r pyproject.toml
COPY . .
RUN uv pip install -e .
```

## Optimizations

### Using uv temporarily

If uv isn't needed in the final image, the binary can be mounted in each invocation:

```dockerfile title="Dockerfile"
RUN --mount=from=uv,source=/uv,target=/bin/uv \
    uv pip install --system ruff
```

### Caching

A [cache mount](https://docs.docker.com/build/guide/mounts/#add-a-cache-mount) can be used to
improve performance across builds:

```dockerfile title="Dockerfile"
RUN --mount=type=cache,target=/root/.cache/uv \
 ./uv pip install -r requirements.txt -->
```

Note the cache directory's location can be determined with the `uv cache dir` command.
Alternatively, the cache can be set to a constant location:

```dockerfile title="Dockerfile"
ENV UV_CACHE_DIR=/opt/uv-cache/
```

If not mounting the cache, image size can be reduced with `--no-cache` flag.
