# Using uv in Docker

## Getting started

!!! tip

    Check out the [`uv-docker-example`](https://github.com/astral-sh/uv-docker-example) project for
    an example of best practices when using uv to build an application in Docker.

### Running uv in a container

A Docker image is published with a built version of uv available. To run a uv command in a
container:

```console
$ docker run ghcr.io/astral-sh/uv --help
```

### Installing uv

uv can be installed by copying from the official Docker image:

```dockerfile title="Dockerfile"
FROM python:3.12-slim-bookworm
COPY --from=ghcr.io/astral-sh/uv:latest /uv /bin/uv
```

Or, with the installer:

```dockerfile title="Dockerfile"
FROM python:3.12-slim-bookworm

# The installer requires curl (and certificates) to download the release archive
RUN apt-get update && apt-get install -y --no-install-recommends curl ca-certificates

# Download the latest installer
ADD https://astral.sh/uv/install.sh /uv-installer.sh

# Run the installer then remove it
RUN sh /uv-installer.sh && rm /uv-installer.sh

# Ensure the installed binary is on the `PATH`
ENV PATH="/root/.cargo/bin/:$PATH"
```

Note this requires `curl` to be available.

In either case, it is best practice to pin to a specific uv version, e.g., with:

```dockerfile
COPY --from=ghcr.io/astral-sh/uv:0.3.4 /uv /bin/uv
```

Or, with the installer:

```dockerfile
ADD https://astral.sh/uv/0.3.4/install.sh /uv-installer.sh
```

### Installing a project

If you're using uv to manage your project, you can copy it into the image and install it:

```dockerfile title="Dockerfile"
# Copy the project into the image
ADD . /app

# Sync the project into a new environment, using the frozen lockfile
WORKDIR /app
RUN uv sync --frozen
```

!!! important

    It is best practice to add `.venv` to a [`.dockerignore` file](https://docs.docker.com/build/concepts/context/#dockerignore-files)
    in your repository to prevent it from being included in image builds. The project virtual
    environment is dependent on your local platform and should be created from scratch in the image.

!!! note

    uv does not yet support installing Python on musl-based distributions. For example, if you are
    using an Alpine Linux base image that doesn't have Python installed, you need to add it with
    the system package manager:

    ```shell
    apk add --no-cache python3~=3.12
    ```

Then, to start your application by default:

```dockerfile title="Dockerfile"
# Presuming there is a `my_app` command provided by the project
CMD ["uv", "run", "my_app"]
```

!!! tip

    It is best practice to use [intermediate layers](#intermediate-layers) separating installation
    of dependencies and the project itself to improve Docker image build times.

See a complete example in the
[`uv-docker-example` project](https://github.com/astral-sh/uv-docker-example/blob/main/Dockerfile).

### Using the environment

Once the project is installed, you can either _activate_ the project virtual environment by placing
its binary directory at the front of the path:

```dockerfile title="Dockerfile"
ENV PATH="/app/.venv/bin:$PATH"
```

Or, you can use `uv run` for any commands that require the environment:

```dockerfile title="Dockerfile"
RUN uv run some_script.py
```

### Using installed tools

To use installed tools, ensure the [tool bin directory](../../concepts/tools.md#the-bin-directory)
is on the path:

```dockerfile title="Dockerfile"
ENV PATH=/root/.local/bin:$PATH
RUN uv tool install cowsay
```

```console
$ docker run -it $(docker build -q .) /bin/bash -c "cowsay -t hello"
  _____
| hello |
  =====
     \
      \
        ^__^
        (oo)\_______
        (__)\       )\/\
            ||----w |
            ||     ||
```

!!! note

    The tool bin directory's location can be determined by running the `uv tool dir --bin` command
    in the container.

    Alternatively, it can be set to a constant location:

    ```dockerfile title="Dockerfile"
    ENV UV_TOOL_BIN_DIR=/opt/uv-bin/
    ```

## Developing in a container

When developing, it's useful to mount the project directory into a container. With this setup,
changes to the project can be immediately reflected in a containerized service without rebuilding
the image. However, it is important _not_ to include the project virtual environment (`.venv`) in
the mount, because the virtual environment is platform specific and the one built for the image
should be kept.

### Mounting the project with `docker run`

Bind mount the project (in the working directory) to `/app` while retaining the `.venv` directory
with an [anonymous volume](https://docs.docker.com/engine/storage/#volumes):

```console
$ docker run --rm --volume .:/app --volume /app/.venv [...]
```

!!! tip

    The `--rm` flag is included to ensure the container and anonymous volume are cleaned up when the
    container exits.

See a complete example in the
[`uv-docker-example` project](https://github.com/astral-sh/uv-docker-example/blob/main/run.sh).

### Configuring `watch` with `docker compose`

When using Docker compose, more sophisticated tooling is available for container development. The
[`watch`](https://docs.docker.com/compose/file-watch/#compose-watch-versus-bind-mounts) option
allows for greater granularity than is practical with a bind mount and supports triggering updates
to the containerized service when files change.

!!! note

    This feature requires Compose 2.22.0 which is bundled with Docker Desktop 4.24.

Configure `watch` in your
[Docker compose file](https://docs.docker.com/compose/compose-application-model/#the-compose-file)
to mount the project directory without syncing the project virtual environment and to rebuild the
image when the configuration changes:

```yaml title="compose.yaml"
services:
  example:
    build: .

    # ...

    develop:
      # Create a `watch` configuration to update the appl
      #
      watch:
        # Sync the working directory with the `/app` directory in the container
        - action: sync
          path: .
          target: /app
          # Exclude the project virtual environment
          ignore:
            - .venv/

        # Rebuild the image on changes to the `pyproject.toml`
        - action: rebuild
          path: ./pyproject.toml
```

Then, run `docker compose watch` to run the container with the development setup.

See a complete example in the
[`uv-docker-example` project](https://github.com/astral-sh/uv-docker-example/blob/main/compose.yml).

## Optimizations

### Compiling bytecode

Compiling Python source files to bytecode is typically desirable for production images as it tends
to improve startup time (at the cost of increased installation time).

To enable bytecode compilation, use the `--compile-bytecode` flag:

```dockerfile title="Dockerfile"
RUN uv sync --compile-bytecode
```

Alternatively, you can set the `UV_COMPILE_BYTECODE` environment variable to ensure that all
commands within the Dockerfile compile bytecode:

```dockerfile title="Dockerfile"
ENV UV_COMPILE_BYTECODE=1
```

### Caching

A [cache mount](https://docs.docker.com/build/guide/mounts/#add-a-cache-mount) can be used to
improve performance across builds:

```dockerfile title="Dockerfile"
RUN --mount=type=cache,target=/root/.cache/uv \
    uv sync
```

If you're not mounting the cache, image size can be reduced by using the `--no-cache` flag or
setting `UV_NO_CACHE`.

!!! note

    The cache directory's location can be determined by running the `uv cache dir` command in the
    container.

    Alternatively, the cache can be set to a constant location:

    ```dockerfile title="Dockerfile"
    ENV UV_CACHE_DIR=/opt/uv-cache/
    ```

### Intermediate layers

If you're using uv to manage your project, you can improve build times by moving your transitive
dependency installation into its own layer via the `--no-install` options.

`uv sync --no-install-project` will install the dependencies of the project but not the project
itself. Since the project changes frequently, but its dependencies are generally static, this can be
a big time saver.

```dockerfile title="Dockerfile"
# Install uv
FROM python:3.12-slim
COPY --from=ghcr.io/astral-sh/uv:latest /uv /bin/uv

# Change the working directory to the `app` directory
WORKDIR /app

# Copy the lockfile and `pyproject.toml` into the image
ADD uv.lock /app/uv.lock
ADD pyproject.toml /app/pyproject.toml

# Install dependencies
RUN uv sync --frozen --no-install-project

# Copy the project into the image
ADD . /app

# Sync the project
RUN uv sync --frozen
```

Note that the `pyproject.toml` is required to identify the project root and name, but the project
_contents_ are not copied into the image until the final `uv sync` command.

!!! tip

    If you're using a [workspace](../../concepts/workspaces.md), then use the
    `--no-install-workspace` flag which excludes the project _and_ any workspace members.

    If you want to remove specific packages from the sync, use `--no-install-package <name>`.

### Using uv temporarily

If uv isn't needed in the final image, the binary can be mounted in each invocation:

```dockerfile title="Dockerfile"
RUN --mount=from=ghcr.io/astral-sh/uv,source=/uv,target=/bin/uv \
    uv sync
```

## Using the pip interface

### Installing a package

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

### Installing requirements

To install requirements files, copy them into the container:

```dockerfile title="Dockerfile"
COPY requirements.txt .
RUN uv pip install -r requirements.txt
```

### Installing a project

When installing a project alongside requirements, it is best practice to separate copying the
requirements from the rest of the source code. This allows the dependencies of the project (which do
not change often) to be cached separately from the project itself (which changes very frequently).

```dockerfile title="Dockerfile"
COPY pyproject.toml .
RUN uv pip install -r pyproject.toml
COPY . .
RUN uv pip install -e .
```
