# Using tools

The `uv tool run` command can be used to running tools and a `uvx` alias is provided to shorten invocations.

## Running a tool

Tool commands are run with `uvx <name>`. For example, to run Ruff:

```console
$ uvx ruff
```

## Tool versions

Unless a specific version is requested, `uvx` will use the latest available version of the requested tool _on the first
invocation_. After that, `uvx` will use the cached version of the tool unless a different version is
requested, the cache is pruned, or the cache is refreshed.

For example, to run a specific version of Ruff:

```console
$ uvx ruff@0.6.0 --version
ruff 0.6.0
```

A subsequent invocation of `uvx` will use the latest, not the cached, version.

```console
$ uvx ruff --version
ruff 0.6.2
```

But, if a new version of Ruff was released, it would not be used unless the cache was refreshed.

To request the latest version of Ruff and refresh the cache, use the `@latest` suffix:

```console
$ uvx ruff@latest --version
0.6.2
```

## Using installed tools

Once a tool is installed with `uv tool install`, `uvx` will use the installed version by default.

For example, after installing an older version of Ruff:

```console
$ uv tool install ruff==0.5.0
```

The version of `ruff` and `uvx ruff` is the same:

```console
$ ruff --version
ruff 0.5.0
$ uvx ruff --version
ruff 0.5.0
```

However, you can ignore the installed version by requesting the latest version explicitly, e.g.:

```console
$ uvx ruff@latest --version
0.6.2
```

Or, by using the `--isolated` flag, which will avoid refreshing the cache but ignore the installed
version:

```console
$ uvx --isolated ruff --version
0.6.2
```

## Specifying tool sources

The package that `uvx` installs to provide the requested command is presumed to have the same name
as the command. For example, when `uvx ruff` is invoked, uv installs the `ruff` package which
provides the `ruff` command.

However, sometimes the package and command names differ. The `--from` option can be used to request a different source package.

The `--from` option can be used to invoke a command from a specific package, e.g. `http` which is
provided by `httpie`:

```console
$ uvx --from httpie http
```

The `--from` option can also be used to install a tool from a specific package source, e.g., to
install `httpie` from GitHub instead of PyPI:

```console
$ uvx --from git+https://github.com/httpie/cli http
```

Or, to run it from a local source tree:

```console
$ gh repo clone httpie/cli httpie-cli
$ uvx --from ./httpie-cli http
```

## Relationship to `uv run`

The invocation `uv tool run <name>` (or `uvx <name>`) is nearly equivalent to:

```console
$ uv run --no-project --with <name> -- <name>
```

However, there are a couple notable differences when using uv's tool interface:

- The `--with` option is not needed — the required package is inferred from the command name.
- The temporary environment is cached in a dedicated location.
- The `--no-project` flag is not needed — tools are always run isolated from the project.
- If a tool is already installed, `uv tool run` will use the installed version but `uv run` will
  not.

If the tool should not be isolated from the project, e.g., when running `pytest` or `mypy`, then
`uv run` should be used instead of `uv tool run`.

## Cached environments

When running a tool with `uvx`, a virtual environment is stored in the uv cache directory and is
treated as disposable, i.e., if you run `uv cache clean` the environment will be deleted. The
environment is only cached to reduce the overhead of repeated invocations. If the environment is
removed, a new one will be created automatically.
