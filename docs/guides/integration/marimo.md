---
title: Using uv with marimo
description:
  A complete guide to using uv with marimo notebooks for interactive computing, script
  execution, and data apps.
---

# Using uv with marimo

[marimo](https://github.com/marimo-team/marimo) is an open-source Python notebook that blends
interactive computing with the reproducibility and reusability of traditional software, letting you
version with Git, run as scripts, and share as apps. Because marimo notebooks are stored as pure
Python scripts, they are able to integrate tightly with uv.

You can readily use marimo in uv projects, as standalone scripts that contain their own
dependencies, in non-project environments, and as a standalone tool.

## Using marimo within a project

If you're working within a [project](../../concepts/projects/index.md), you can start a marimo
notebook with access to the project's virtual environment with the following command (assuming marimo is
a project dependency):

```console
$ uv run marimo edit my_notebook.py
```

To make additional packages available to your notebook, either add them to your project with `uv
add`, or use marimo's built-in package installation UI, which will invoke `uv add` on your
behalf.

If marimo is not a project dependency, you can still run a notebook with

```console
$ uv run --with marimo marimo edit my_notebook.py
```

which will let you import your project's modules. However, packages installed via marimo's
UI when running in this way will not be added to your project, and may
disappear on subsequent marimo invocations.

## Using marimo with inline script metadata

Because marimo notebooks are stored as Python scripts, they can encapsulate their own dependencies
using inline script metadata, using uv's [support for scripts](../../guides/scripts.md). For
example:

```console
$ uv add --script my_notebook.py numpy
```

To run a notebook containing script metadata, use

```console
$ uvx marimo edit --sandbox my_notebook.py
```

and marimo will automatically use uv to start your notebook in an isolated virtual environment with
your script's dependencies. Packages installed from the marimo UI will automatically be added to
the notebook's script metadata.

## Using marimo in a non-project environment

To run marimo in a virtual environment that isn't associated with a
[project](../../concepts/projects/index.md), add marimo to the environment directly:

=== "macOS and Linux"

    ```console
    $ uv venv
    $ uv pip install numpy
    $ uv pip install marimo
    $ .venv/bin/marimo edit
    ```

=== "Windows"

    ```pwsh-session
    PS> uv venv
    PS> uv pip install numpy
    PS> uv pip install marimo
    PS> .venv\Scripts\marimo edit
    ```

From here, `import numpy` will work within the notebook, and marimo's UI installer will add
packages to the environment with `uv pip install` on your behalf.

## Running marimo notebooks as scripts

Run your notebooks as scripts with

```console
$ uv run my_notebook.py
```

## Using marimo as a standalone tool

For adhoc access to marimo notebooks, start a marimo server at any time, in an isolated environment, with
`uvx marimo edit`.
