# Using tools

Many Python packages provide command-line tools. uv has specialized support for invoking tools provided by these packages.

## Using `uvx`

The `uvx` command is an alias for `uv tool run` which can be used to invoke a tool without installing it.

For example, to run `ruff`:

```console
$ uvx ruff
```

Note this is exactly equivalent to:

```console
$ uv tool run ruff
```

Arguments can be passed to the tools:

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

## Commands with different package names

In `uvx ruff`, the `ruff` package is installed to provide the `ruff` command. However, sometimes the package name differs from the command name.

To invoke a command from a specific package, e.g., `http` which is provided by `httpie`:

```console
$ uvx --from httpie http
```

## Requesting specific versions

To run a tool at a specific version, use `command@<version>`:

```console
$ uvx ruff@0.3.0 check
```

The `--from` option can also be used to specify package versions:

To constrain to a range of versions:

```console
$ uvx --from 'ruff>0.2.0,<0.3.0' ruff check
```

## Requesting different sources

The `--from` option can also be used to install from alternative sources.

To pull from git:

```console
$ uvx --from git+https://github.com/httpie/cli httpie
```

## Commands with plugins

Additional dependencies can be included, e.g., to include `mkdocs-material` when running `mkdocs`:

```console
$ uvx --with mkdocs-material mkdocs --help
```

## Relationship to `uv run`

The invocation `uv tool run ruff` is nearly equivalent to:

```console
$ uv run --isolated --with ruff -- ruff
```

However, there are a couple notable differences when using uv's tool interface:

- The `--with` is not needed — the required package is inferred from the command name.
- The temporary environment is cached in a dedicated location.
- The `--isolated` flag is not needed — tools are always run isolated from the project.
- If a tool is already installed, `uv tool run` will use the installed version but `uv run` will not.

## Installing tools

If a tool is used often, it can be useful to install it to a persistent environment instead of invoking `uvx` repeatedly. 

To install `ruff`:

```console
$ uv tool install ruff
```

When a tool is installed, its executables are placed in a `bin` directory on the `PATH` which allows the tool to be run without uv (if it's not on the `PATH`, we'll warn you).

After installing `ruff`, it should be available:

```console
$ ruff --version
```

Unlike `uvx`, `uv tool install` operates on a _package_ and will install all executables provided by the tool.

For example, the following will install `http`, `https`, and `httpie`:

```console
$ uv tool install httpie
```

Additionally, package versions can be included without `--from`:

```console
$ uv tool install 'httpie>0.1.0'
```

And similarly for package sources:

```console
$ uv tool install git+https://github.com/httpie/cli
```
