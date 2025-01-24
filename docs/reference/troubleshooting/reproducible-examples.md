# Reproducible examples

A minimal reproducible example (MRE) is essential for fixing bugs. Without an example that can be
used to reproduce the problem, a maintainer cannot debug it or test if it is fixed. If the example
is not minimal, i.e., if it includes lots of content which is not related to the issue, it can take
a maintainer much longer to identify the root cause of the problem.

There's a great guide to the basics of creating MREs on
[Stack Overflow](https://stackoverflow.com/help/minimal-reproducible-example). Here, we'll discuss a
couple strategies for creating MREs specific to uv.

## Docker image

Writing a Docker image is often the best way to share a reproducible example because it is entirely
self-contained. This means that the state from the reproducer's system does not affect the problem.

!!! note

    Using a Docker image is only feasible if the issue is reproducible on Linux. When using macOS,
    it's prudent to ensure your image is not reproducible on Linux but some bugs _are_ specific
    to the operating system. While using Docker to run Windows containers is feasible, it's not
    commonplace. These sorts of bugs are expected to be reported as a [script](#script) instead.

When writing a Docker MRE with uv, it's best to start with one of
[uv's Docker images](../../guides/integration/docker.md#available-images). When doing so, be sure to
pin to a specific version of uv.

```Dockerfile
FROM ghcr.io/astral-sh/uv:0.5.24-debian-slim
```

While Docker images are isolated from the system, the build will use your system's architecture by
default. When sharing a reproduction, you can explicitly set the platform to ensure a reproducer
gets the expected behavior. uv publishes images for `linux/amd64` (e.g., Intel or AMD) and
`linux/arm64` (e.g., Apple M Series or ARM)

```Dockerfile
FROM --platform=linux/amd64 ghcr.io/astral-sh/uv:0.5.24-debian-slim
```

Docker images are best for reproducing issues that can be constructed with commands, e.g.:

```Dockerfile
FROM --platform=linux/amd64 ghcr.io/astral-sh/uv:0.5.24-debian-slim

RUN uv init /mre
WORKDIR /mre
RUN uv add pydantic
RUN uv sync
RUN uv run -v python -c "import pydantic"
```

However, you can also write files into the image inline:

```Dockerfile
FROM --platform=linux/amd64 ghcr.io/astral-sh/uv:0.5.24-debian-slim

COPY <<EOF /mre/pyproject.toml
[project]
name = "example"
version = "0.1.0"
description = "Add your description here"
readme = "README.md"
requires-python = ">=3.12"
dependencies = ["pydantic"]
EOF

WORKDIR /mre
RUN uv lock
```

If you need to write many files, it's better to create and publish a
[Git repository](#git-repository). You can combine these approaches and include a `Dockerfile` in
the repository.

When sharing a Docker reproduction, it's helpful to include the build logs. You can see more output
from the build steps by disabling caching and the fancy output:

```console
docker build . --progress plain --no-cache
```

## Git repository

When sharing a Git repository reproduction, it's helpful to include a [script](#script) that
reproduces the problem or, even better, a [Dockerfile](#docker-image).

You can quickly create a new repository in the [GitHub UI](https://github.com/new) or with the `gh`
CLI:

```console
$ gh repo create uv-mre-1234 --clone
```

When using a Git repository for a reproduction, please remember to _minimize_ the contents by
excluding files or configuration that are not required to reproduce your problem.

## Script

When reporting platform-specific bugs that cannot be reproduced in a [container](#docker-image),
it's best practice to include a script showing the commands that can be used to reproduce the bug,
e.g.:

```bash
uv init
uv add pydantic
uv sync
uv run -v python -c "import pydantic"
```

In addition to the script, include _verbose_ logs (i.e., with the `-v` flag) of the failure and the
complete error message.

Whenever a script relies on external state, be sure to share that information. For example, if you
wrote the script on Windows and it uses a Python version that you installed with `choco` and runs on
PowerShell 6.2, please include that in the report.
