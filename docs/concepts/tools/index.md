# Tools

Tools are Python packages that provide command-line interfaces. uv includes a dedicated interface for interacting with tools. 

!!! note

    See the [tools guide](../guides/tools.md) for an introduction to working with the tools
    interface — this section discusses details of tool management.

## Using tools

Tools can be run without installation using `uv tool run`, in which case their dependencies are
installed in a temporary, isolated virtual environment.

Because it is very common to run tools without installing them, a `uvx` alias is provided for
`uv tool run` — the two commands are exactly equivalent. For brevity, the documentation will mostly
refer to `uvx` instead of `uv tool run`.

See [Using tools](using-tools.md) for more details.

## Installing tools

Tools can also be installed with `uv tool install`, in which case their executables are
[available on the `PATH`](./installing-tools.md#the-path) — an isolated virtual environment is still used, but it is not
removed when the command completes.

See [Installing tools](installing-tools.md) for more details.
