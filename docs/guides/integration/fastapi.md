# Using uv with FastAPI

[FastAPI](https://github.com/fastapi/fastapi) is a modern, high-performance Python web framework.
You can use uv to manage your FastAPI project, including installing dependencies, managing
environments, running FastAPI applications, and more.

!!! note

    You can view the source code for this guide in the [uv-fastapi-example](https://github.com/astral-sh/uv-fastapi-example) repository.

As an example, consider the sample application defined in the
[FastAPI documentation](https://fastapi.tiangolo.com/tutorial/bigger-applications/), structured as
follows:

```plaintext
.
├── app
│   ├── __init__.py
│   ├── main.py
│   ├── dependencies.py
│   └── routers
│   │   ├── __init__.py
│   │   ├── items.py
│   │   └── users.py
│   └── internal
│       ├── __init__.py
│       └── admin.py
```

To migrate this project to uv, add a `pyproject.toml` file to the root directory of the project with
`uv init`:

```console
$ uv init
$ uv add fastapi --extra standard
```

!!! tip

    If you have an existing `pyproject.toml`, `uv init` cannot be used — but you may not need to
    make any changes to the file.

You should now have the following structure:

```plaintext
.
├── pyproject.toml
├── app
│   ├── __init__.py
│   ├── main.py
│   ├── dependencies.py
│   └── routers
│   │   ├── __init__.py
│   │   ├── items.py
│   │   └── users.py
│   └── internal
│       ├── __init__.py
│       └── admin.py
```

And the contents of the `pyproject.toml` file should look something like this:

```toml title="pyproject.toml"
[project]
name = "uv-fastapi-example"
version = "0.1.0"
description = "FastAPI project"
readme = "README.md"
requires-python = ">=3.12"
dependencies = [
    "fastapi[standard]",
]
```

From there, you can run the FastAPI application with:

```console
$ uv run fastapi dev
```

`uv run` will automatically resolve and lock the project dependencies (i.e., create a `uv.lock`
alongside the `pyproject.toml`), create a virtual environment, and run the command in that
environment.

## Initializing a FastAPI project

We could reach a similar result to the above by creating a project from scratch with `uv init` and
installing FastAPI with `uv add fastapi`, as in:

```console
$ uv init app
$ cd app
$ uv add fastapi --extra standard
```

By default, uv uses a `src` layout for newly-created projects, so the `app` directory will be nested
within a `src` directory. If you copied over the source code from the FastAPI tutorial, the project
structure would look like this:

```plaintext
.
├── pyproject.toml
└── src
    └── app
        ├── __init__.py
        ├── main.py
        ├── dependencies.py
        └── routers
        │   ├── __init__.py
        │   ├── items.py
        │   └── users.py
        └── internal
            ├── __init__.py
            └── admin.py
```

In this case, you would run the FastAPI application with:

```console
$ uv run fastapi dev src/app/main.py
```

## Deployment

To deploy the FastAPI application with Docker, you can use the following `Dockerfile`:

```dockerfile title="Dockerfile"
FROM python:3.12-slim

# Install uv.
COPY --from=ghcr.io/astral-sh/uv:latest /uv /bin/uv

# Copy the application into the container.
COPY . /app

# Install the application dependencies.
WORKDIR /app
RUN uv sync --frozen --no-cache

# Run the application.
CMD ["/app/.venv/bin/fastapi", "run", "app/main.py", "--port", "80"]
```

For more on using uv with Docker, see the [Docker guide](./docker.md).
