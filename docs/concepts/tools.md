# Tools

Tools are Python packages that provide command-line interfaces. Tools can be invoked without
installation using `uvx`, in which case their dependencies are installed in a temporary virtual
environment isolated from the current project. Tools can also be installed with `uv tool install`,
in which case their executables are [available on the `PATH`](#the-path) — an isolated virtual
environment is still used but it is not treated as disposable.

!!! note

    See the [tools guide](../guides/tools.md) for an introduction to working with the tools
    interface — this document discusses details of tool management.

## Tool environments

When running a tool with `uvx` or `uv tool run`, a virtual environment is stored in the uv cache
directory and is treated as disposable. The environment is cached to reduce the overhead of
invocations.

When installing a tool with `uv tool install`, a virtual environment is created in the uv tools
directory.

### Tools directory

By default, the uv tools directory is named `tools` and is in the uv application state directory,
e.g., `~/.local/share/uv/tools`. The location may be customized with the `UV_TOOL_DIR` environment
variable.

To display the path to the tool installation directory:

```console
$ uv tool dir
```

Tool environments are placed in a directory with the same name as the tool package, e.g.,
`.../tools/<name>`.

### Mutating tool environments

Tool environments are _not_ intended to be mutated directly. It is strongly recommended never to
mutate a tool environment manually with a `pip` operation.

Tool environments may be upgraded via `uv tool upgrade`, or re-created entirely via subsequent
`uv tool install` operations.

To upgrade all packages in a tool environment

```console
$ uv tool upgrade black
```

To upgrade a single package in a tool environment:

```console
$ uv tool upgrade black --upgrade-package click
```

To reinstall all packages in a tool environment

```console
$ uv tool upgrade black --reinstall
```

To reinstall a single package in a tool environment:

```console
$ uv tool upgrade black --reinstall-package click
```

Tool upgrades will respect the version constraints provided when installing the tool. For example,
`uv tool install black >=23,<24` followed by `uv tool upgrade black` will upgrade Black to the
latest version in the range `>=23,<24`.

To instead replace the version constraints, re-install the tool with `uv tool install`:

```console
$ uv tool install black>=24
```

Similarly, tool upgrades will retain the settings provided when installing the tool. For example,
`uv tool install black --prelease allow` followed by `uv tool upgrade black` will retain the
`--prelease allow` setting.

Tool upgrades will reinstall the tool executables, even if they have not changed.

### Including additional dependencies

Additional packages can be included during tool invocations:

```console
$ uvx --with <extra-package> <tool>
```

And installations:

```console
$ uv tool install --with <extra-package> <tool-package>
```

The `--with` option can be provided multiple times to include additional packages.

The `--with` option supports package specifications, so a specific version can be requested:

```console
$ uvx --with <extra-package>==<version> <tool-package>
```

If the requested version conflicts with the requirements of the tool package, package resolution
will fail and the command will error.

## Tool executables

Tool executables include all console entry points, script entry points, and binary scripts provided
by a Python package. Tool executables are symlinked into the `bin` directory on Unix and copied on
Windows.

### The `bin` directory

Executables are installed into the user `bin` directory following the XDG standard, e.g.,
`~/.local/bin`. Unlike other directory schemes in uv, the XDG standard is used on _all platforms_
notably including Windows and macOS — there is no clear alternative location to place executables on
these platforms. The installation directory is determined from the first available environment
variable:

- `$XDG_BIN_HOME`
- `$XDG_DATA_HOME/../bin`
- `$HOME/.local/bin`

Executables provided by dependencies of tool packages are not installed.

### The `PATH`

The `bin` directory must be in the `PATH` variable for tool executables to be available from the
shell. If it is not in the `PATH`, a warning will be displayed. The `uv tool update-shell` command
can be used to add the `bin` directory to the `PATH` in common shell configuration files.

### Overwriting executables

Installation of tools will not overwrite executables in the `bin` directory that were not previously
installed by uv. For example, if `pipx` has been used to install a tool, `uv tool install` will
fail. The `--force` flag can be used to override this behavior.

## Relationship to `uv run`

The invocation `uv tool run <name>` is nearly equivalent to:

```console
$ uv run --no-project --with <name> -- <name>
```

However, there are a couple notable differences when using uv's tool interface:

- The `--with` option is not needed — the required package is inferred from the command name.
- The temporary environment is cached in a dedicated location.
- The `--no-project` flag is not needed — tools are always run isolated from the project.
- If a tool is already installed, `uv tool run` will use the installed version but `uv run` will
  not.
- The project will be built and installed instead of using an editable installation
