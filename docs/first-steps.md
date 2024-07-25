# First steps with uv

uv only provides a command-line interface and must be used from a terminal.

After [installing uv](./installation.md), you can check that uv is installed by running the `uv` command:

```console
$ uv
```

You should see a help menu listing the available commands.

## Viewing the version

To check the installed version:

```console
$ uv version
```

The following are also valid:

```console
$ uv --version      # Same output as `uv version`
$ uv -V             # Will not include the build commit and date
$ uv pip --version  # Can be used with a subcommand
```

## Help menus

The `--help` flag can be used to view the help menu for a command, e.g., for `uv`:

```console
$ uv --help
```

To view the help menu for a specific command, e.g., for `uv init`:

```console
$ uv init --help
```

When using the `--help` flag, uv displays a condensed help menu. To view a longer help menu for a command, use `uv help`:

```console
$ uv help
```

To view the long help menu for a specific command, e.g., for `uv init`:

```console
$ uv help init
```

When using the long help menu, uv will attempt to use `less` or `more` to "page" the output so it is not all displayed at once. To exit the pager, press `q`.

## Next steps

Now that you've confirmed uv is installed and know how to get help, check out an [overview of features](./features.md) or jump straight into the [guides](./guides/index.md) to start using uv.
