# Using tools

Many Python packages provide applications that can be used as tools. uv has specialized support for
easily invoking and installing tools.

## Running tools

The `uvx` command invokes a tool without installing it.

For example, to run `ruff`:

```console
$ uvx ruff
```

!!! note

    This is exactly equivalent to:

    ```console
    $ uv tool run ruff
    ```

    `uvx` is provided as an alias for convenience.

Arguments can be provided after the tool name:

```console
$ uvx pycowsay hello from uv

  -------------
< hello from uv >
  -------------
   \   ^__^
    \  (oo)\_______
       (__)\       )\/\
           ||----w |
           ||     ||

```

Tools are installed into temporary, isolated environments when using `uvx`.

## Commands with different package names

When `uvx ruff` is invoked, uv installs the `ruff` package which provides the `ruff` command.
However, sometimes the package and command names differ.

The `--from` option can be used to invoke a command from a specific package, e.g. `http` which is
provided by `httpie`:

```console
$ uvx --from httpie http
```

## Requesting specific versions

To run a tool at a specific version, use `command@<version>`:

```console
$ uvx ruff@0.3.0 check
```

The `--from` option can also be used to specify package versions, as above:

```console
$ uvx --from 'ruff==0.3.0' ruff check
```

Or, to constrain to a range of versions:

```console
$ uvx --from 'ruff>0.2.0,<0.3.0' ruff check
```

Note the `@` syntax cannot be used for anything other than an exact version.

## Requesting different sources

The `--from` option can also be used to install from alternative sources.

For example, to pull from git:

```console
$ uvx --from git+https://github.com/httpie/cli httpie
```

## Commands with plugins

Additional dependencies can be included, e.g., to include `mkdocs-material` when running `mkdocs`:

```console
$ uvx --with mkdocs-material mkdocs --help
```

## Installing tools

If a tool is used often, it is useful to install it to a persistent environment and add it to the
`PATH` instead of invoking `uvx` repeatedly.

To install `ruff`:

```console
$ uv tool install ruff
```

When a tool is installed, its executables are placed in a `bin` directory in the `PATH` which allows
the tool to be run without uv. If it's not on the `PATH`, a warning will be displayed and
`uv tool update-shell` can be used to add it to the `PATH`.

After installing `ruff`, it should be available:

```console
$ ruff --version
```

Unlike `uv pip install`, installing a tool does not make its modules available in the current
environment. For example, the following command will fail:

```console
$ python -c "import ruff"
```

This isolation is important for reducing interactions and conflicts between dependencies of tools,
scripts, and projects.

Unlike `uvx`, `uv tool install` operates on a _package_ and will install all executables provided by
the tool.

For example, the following will install the `http`, `https`, and `httpie` executables:

```console
$ uv tool install httpie
```

Additionally, package versions can be included without `--from`:

```console
$ uv tool install 'httpie>0.1.0'
```

And, similarly, for package sources:

```console
$ uv tool install git+https://github.com/httpie/cli
```

As with `uvx`, installations can include additional packages:

```console
$ uv tool install mkdocs --with mkdocs-material
```

## Upgrading tools

To upgrade a tool, use `uv tool upgrade`:

```console
$ uv tool upgrade ruff
```

Tool upgrades will respect the version constraints provided when installing the tool. For example,
`uv tool install ruff >=0.3,<0.4` followed by `uv tool upgrade ruff` will upgrade Ruff to the latest
version in the range `>=0.3,<0.4`.

To instead replace the version constraints, re-install the tool with `uv tool install`:

```console
$ uv tool install ruff>=0.4
```

To instead upgrade all tools:

```console
$ uv tool upgrade --all
```

## Next steps

To learn more about managing tools with uv, see the [Tools concept](../concepts/tools.md) page and
the [command reference](../reference/cli.md#uv-tool).

Or, read on to learn how to to [work on projects](./projects.md).
