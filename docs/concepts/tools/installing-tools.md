## When should tools be installed?

In most cases, executing a tool with `uvx` is more appropriate than installing the tool. Installing
the tool is useful if you need the tool to be available to other programs on your system, e.g., if
some script you do not control requires the tool, or if you are in a Docker image and want to make
the tool available to users.

## Installing specific versions

Unless a specific version is requested, `uv tool install` will install the latest available of the
requested tool.

`uv tool install` will also respect the `{package}@{version}` and `{package}@latest` specifiers, as
in:

```console
$ uv tool install ruff@latest
$ uv tool install ruff@0.6.0
```

## Including additional dependencies

Additional packages can be included during tool execution:

```console
$ uvx --with <extra-package> <tool>
```

And, during tool installation:

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

## Upgrading tools

To upgrade all packages in a tool environment

```console
$ uv tool upgrade black
```

To upgrade a single package in a tool environment:

```console
$ uv tool upgrade black --upgrade-package click
```

Tool upgrades will respect the version constraints provided when installing the tool. For example,
`uv tool install black >=23,<24` followed by `uv tool upgrade black` will upgrade Black to the
latest version in the range `>=23,<24`.

To instead replace the version constraints, re-install the tool with `uv tool install`:

```console
$ uv tool install black>=24
```

## Reinstalling tools

To reinstall all packages in a tool environment

```console
$ uv tool upgrade black --reinstall
```

To reinstall a single package in a tool environment:

```console
$ uv tool upgrade black --reinstall-package click
```

Similarly, tool upgrades will retain the settings provided when installing the tool. For example,
`uv tool install black --prerelease allow` followed by `uv tool upgrade black` will retain the
`--prerelease allow` setting.

Tool upgrades will reinstall the tool executables, even if they have not changed.

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

- `$UV_TOOL_BIN_DIR`
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


## Tool environments

When installing a tool with `uv tool install`, a virtual environment is created in the uv tools
directory. The environment will not be removed unless the tool is uninstalled. If the environment is
manually deleted, the tool will fail to run.

!!! warning

    Tool environments are _not_ intended to be mutated directly. It is strongly recommended never to
    mutate a tool environment manually with a `pip` operation.

    Tool environments may be upgraded via `uv tool upgrade`, or re-created entirely via subsequent
    `uv tool install` operations.

## The tool directory

By default, the uv tool directory is named `tools` and is in the uv application state directory,
e.g., `~/.local/share/uv/tools`. The location may be customized with the `UV_TOOL_DIR` environment
variable.

To display the path to the tool installation directory:

```console
$ uv tool dir
```

Tool environments are placed in a directory with the same name as the tool package, e.g.,
`.../tools/<name>`.
