# Using uv with FastAPI

[FastAPI](https://github.com/fastapi/fastapi) is a modern, high-performance Python web framework.
You can use uv to manage your FastAPI project, including installing dependencies, managing
environments, running FastAPI applications, and more.

!!! note

    You can view the source code for this guide in the [uv-fastapi-example](https://github.com/astral-sh/uv-fastapi-example) repository.

## Migrating an existing FastAPI project

As an example, consider the sample application defined in the
[FastAPI documentation](https://fastapi.tiangolo.com/tutorial/bigger-applications/), structured as
follows:

```plaintext
project
└── app
    ├── __init__.py
    ├── main.py
    ├── dependencies.py
    ├── routers
    │   ├── __init__.py
    │   ├── items.py
    │   └── users.py
    └── internal
        ├── __init__.py
        └── admin.py
```

To use uv with this application, inside the `project` directory run:

```console
$ uv init --app
```

This creates an [Application project](../../concepts/projects.md#applications) with a
`pyproject.toml` file.

Then, add a dependency on FastAPI:

```console
$ uv add fastapi --extra standard
```

You should now have the following structure:

```plaintext
project
├── pyproject.toml
└── app
    ├── __init__.py
    ├── main.py
    ├── dependencies.py
    ├── routers
    │   ├── __init__.py
    │   ├── items.py
    │   └── users.py
    └── internal
        ├── __init__.py
        └── admin.py
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

Test the app by opening http://127.0.0.1:8000/?token=jessica in a web browser.

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
CMD ["/app/.venv/bin/fastapi", "run", "app/main.py", "--port", "80", "--host", "0.0.0.0"]
```

Build the Docker image with:

```console
$ docker build -t fastapi-app .
```

Run the Docker container locally with:

```console
$ docker run -p 8000:80 fastapi-app
```

Navigate to http://127.0.0.1:8000/?token=jessica in your browser to verify that the app is running
correctly.

!!! tip

    For more on using uv with Docker, see the [Docker guide](./docker.md).
