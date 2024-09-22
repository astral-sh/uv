# Using uv with Jupyter

There are a few critical considerations:

- Are you working with a project (in which case, you want `uv run`)? Or are you working with a
  standalone notebook (in which case, you want `uvx`)?
- Do you need to install packages from within Jupyter? Or is the environment read-only? The latter
  is way easier; the former requires some extra work.
- Are you trying to run Jupyter directly? Or through an editor, like VS Code?

## As a standalone tool...

If you're working within a [project](../../concepts/projects.md), you can kick off a Jupyter server
with access to the project's virtual environment via the following:

```console
$ uv run --with jupyter jupyter notebook
```

Within the notebook, you can then import your project's modules as you would in any other `uv run`
invocation. For example, if your project depends on `requests`, `import requests` will import
`requests` from the project's virtual environment.

While `jupyter` itself is installed in an isolated environment when used via
`uv run --with jupyter`, within the notebook, `!uv add` and related commands will modify the
_project's_ environment.

For example, running `!uv add pydantic` from within a notebook will add `pydantic` to the project's
dependencies and virtual environment, such that `import pydantic` will work immediately, without
further configuration or a server restart.

!!! note

    Since the Jupyter server is running in an isolated virtual environment, `!uv pip install` will install package's
    into _Jupyter's_ environment, not the project environment. Such dependencies may disappear on subsequent `jupyter`
    invocations. To install packages into the project environment, use `!uv add`.

If you're working with a notebook that relies on pip (e.g., via the `%pip` magic), you can include
pip in your project's virtual environment by running `uv venv --seed` prior to starting the Jupyter
server. For example, given:

```console
$ uv venv --seed
$ uv run --with jupyter jupyter notebook
```

Subsequent `%pip install` invocations within the notebook will install packages into the project's
virtual environment. However, such modifications will _not_ be reflected in the project's
`pyproject.toml` or `uv.lock` files.

## As a project dependency...

## Within VS Code

```console
# Create a new notebook project
# and open the directory in code
$ uv init my-notebook
$ cd my-notebook
$ uv add ipykernel
$ code .
```

- Now that the new project directory is open in code, use the action "Create: New Jupyter Notebook"
- Click Select Kernel -> Python Environments
- Select the virtual environment that uv created. It will be named .venv/bin/python in this dropdown
  (or maybe .venv\Scripts\python on windows)

If you don't `uv add ipykernel`, the notebook will fail to execute with an error.

## Notes

- If you run `uv add --dev ipykernel`, then `uv run ipython kernel install --user --name=uv`, you
  can then run within the project environment even if Jupyter is installed elsewhere. This is great,
  because `!uv pip install` will install packages into the project environment, not the Jupyter
  environment. The downside is you need to add these as dev dependencies.
