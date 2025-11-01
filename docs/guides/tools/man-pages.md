# Man Pages for Tools

When installing Python tools with `uv tool install`, any man pages (manual pages) provided by the
package are automatically discovered, installed, and managed alongside the tool's executables.

## Overview

Man pages are traditional Unix/Linux documentation files that can be viewed with the `man` command.
Many command-line tools include man pages to provide detailed usage information, examples, and
reference material.

With uv's man page support:

- Man pages are automatically discovered during tool installation
- They are symlinked (Unix/Linux/macOS) or copied (Windows) to your local man page directory
- They appear in `uv tool list` output alongside executables
- They are removed automatically when the tool is uninstalled
- The installation directory can be customized via environment variables

## How it works

During `uv tool install`, uv:

1. **Discovers man pages** from the package's metadata (RECORD file)
2. **Installs them** to the appropriate directory, preserving section structure (man1/, man6/, etc.)
3. **Creates symlinks** (or copies files on Windows) to the tool's virtual environment
4. **Stores metadata** in the tool receipt for later removal

Man pages must be included in the package at paths like `data/<hash>/share/man/man<N>/<filename>` to
be recognized.

## Basic usage

### Installing a tool with man pages

Simply install the tool as normal:

```console
$ uv tool install pycowsay
Resolved 1 package in 0.5s
Installed 1 package in 0.2s
 + pycowsay==0.0.0.2
Installed 1 executable: pycowsay
Installed 1 manpage: man6/pycowsay.6
```

The man page is immediately available:

```console
$ man pycowsay
```

### Listing tools with man pages

Use `uv tool list` to see installed tools and their man pages:

```console
$ uv tool list
pycowsay v0.0.0.2
- pycowsay
- man6/pycowsay.6
```

### Uninstalling removes man pages

When uninstalling a tool, its man pages are removed automatically:

```console
$ uv tool uninstall pycowsay
Uninstalled pycowsay
```

### Upgrading preserves man pages

When upgrading a tool, man pages are updated if the package has changes:

```console
$ uv tool upgrade pycowsay
```

## Directory resolution

uv determines where to install man pages using the following priority order:

1. **`UV_TOOL_MAN_DIR`** - Explicit override (highest priority)
2. **`UV_TOOL_BIN_DIR/../share/man`** - Relative to tool bin directory
3. **`XDG_BIN_HOME/../share/man`** - Relative to XDG bin directory
4. **`XDG_DATA_HOME/man`** - XDG data directory (typically `~/.local/share/man`)
5. **`HOME/.local/share/man`** - Fallback default

The first directory that exists or can be created will be used.

## Environment variables

### `UV_TOOL_MAN_DIR`

Override the man page installation directory:

```console
$ UV_TOOL_MAN_DIR=/custom/path uv tool install pycowsay
Installed 1 manpage: man6/pycowsay.6

$ ls /custom/path/man6/
pycowsay.6
```

This is useful when:

- Installing to a non-standard location
- Testing tool installations in isolated directories
- Managing tools for multiple users with different configurations

### Other relevant variables

- **`UV_TOOL_BIN_DIR`** - If set, man pages install to `$UV_TOOL_BIN_DIR/../share/man`
- **`XDG_BIN_HOME`** - If set, man pages install to `$XDG_BIN_HOME/../share/man`
- **`XDG_DATA_HOME`** - If set, man pages install to `$XDG_DATA_HOME/man`

See the [environment variables reference](../../configuration/environment.md) for more details.

## Troubleshooting

### Man page not found after installation

If `man <tool>` reports "No manual entry", the man page directory may not be in your `MANPATH`.

Check the installation directory:

```console
$ uv tool list --show-paths
pycowsay v0.0.0.2
- pycowsay (/home/user/.local/share/uv/tools/pycowsay/bin/pycowsay)
- man6/pycowsay.6 (/home/user/.local/share/man/man6/pycowsay.6)
```

Add the directory to your `MANPATH`:

```bash
# In ~/.bashrc or ~/.zshrc
export MANPATH="$HOME/.local/share/man:$MANPATH"
```

Or use the absolute path:

```console
$ man /home/user/.local/share/man/man6/pycowsay.6
```

### Permission denied when installing

If you see permission errors during installation, check:

1. The man page directory exists and is writable:

   ```console
   $ ls -ld ~/.local/share/man
   ```

2. Parent directories have correct permissions:

   ```console
   $ ls -ld ~/.local/share
   ```

3. Consider using `UV_TOOL_MAN_DIR` to install to a directory you control.

### Tool has no man pages

Not all Python packages include man pages. To verify:

1. Check the package documentation for whether it provides man pages
2. Install the tool and look for "Installed N manpages" in the output
3. Run `uv tool list` to see if man pages are listed

If the package should have man pages but they're not detected, the package may not be structured
correctly. See the package developer guide below.

## Platform support

### Unix/Linux/macOS

Full support with symlinks. Man pages are symlinked from the tool's virtual environment to the man
page directory, preserving section structure.

### Windows

Basic support via file copy. Man pages are copied (not symlinked) to the man page directory. They
can be viewed with:

- Git Bash or MSYS2's `man` command
- WSL (Windows Subsystem for Linux)
- Cygwin
- Third-party man page viewers

## For package developers

To include man pages in your Python package that will be installed by uv, structure them as data
files:

### Directory structure

```
your-package/
  your_package/
    __init__.py
  share/man/
    man1/
      your-tool.1
    man5/
      your-tool-config.5
```

### Using hatchling

```toml
[build-system]
requires = ["hatchling"]
build-backend = "hatchling.build"

[tool.hatch.build.targets.wheel.shared-data]
"share/man" = "share/man"
```

### Using setuptools

```python
from setuptools import setup

setup(
    name="your-package",
    # ...
    data_files=[
        ('share/man/man1', ['share/man/man1/your-tool.1']),
        ('share/man/man5', ['share/man/man5/your-tool-config.5']),
    ],
)
```

### Using maturin

```toml
[tool.maturin]
data = { "share/man" = "share/man" }
```

### Verification

After building and installing locally, verify man pages appear in the wheel's RECORD file at paths
like:

```
../../data/<hash>/share/man/man1/your-tool.1
```

## Examples

### Tool with multiple man page sections

Some tools provide man pages in different sections:

```console
$ uv tool install example-tool
Installed 3 manpages: man1/example-tool.1, man5/example-tool.conf.5, man7/example-tool.7

$ man example-tool          # Shows man1 page (commands)
$ man 5 example-tool.conf   # Shows man5 page (config files)
$ man 7 example-tool        # Shows man7 page (overview)
```

### Custom installation directory

Install to a project-specific directory:

```console
$ UV_TOOL_MAN_DIR=/project/docs/man uv tool install sphinx
$ export MANPATH="/project/docs/man:$MANPATH"
$ man sphinx-build
```

### Reinstalling with --force

If a man page conflicts with an existing file, use `--force`:

```console
$ uv tool install pycowsay --force
```

This will replace any existing man page at the target location.

## Related commands

- [`uv tool install`](../../reference/cli.md#uv-tool-install) - Install a tool with automatic man
  page discovery
- [`uv tool list`](../../reference/cli.md#uv-tool-list) - List installed tools and their man pages
- [`uv tool uninstall`](../../reference/cli.md#uv-tool-uninstall) - Uninstall a tool and remove its
  man pages
- [`uv tool upgrade`](../../reference/cli.md#uv-tool-upgrade) - Upgrade a tool and update its man
  pages

## See also

- [Tools concept](../../concepts/tools.md) - Understanding uv's tool management
- [Environment variables](../../configuration/environment.md) - Complete reference
- [Publishing a package](../publish.md) - How to include man pages in your package
