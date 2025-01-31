# Locking and syncing

### Creating the lockfile

The lockfile is created and updated during uv invocations that use the project environment, i.e.,
`uv sync` and `uv run`. The lockfile may also be explicitly created or updated using `uv lock`:

```console
$ uv lock
```

### Exporting the lockfile

If you need to integrate uv with other tools or workflows, you can export `uv.lock` to
`requirements.txt` format with `uv export --format requirements-txt`. The generated
`requirements.txt` file can then be installed via `uv pip install`, or with other tools like `pip`.

In general, we recommend against using both a `uv.lock` and a `requirements.txt` file. If you find
yourself exporting a `uv.lock` file, consider opening an issue to discuss your use case.

### Checking if the lockfile is up-to-date

To avoid updating the lockfile during `uv sync` and `uv run` invocations, use the `--frozen` flag.

To avoid updating the environment during `uv run` invocations, use the `--no-sync` flag.

To assert the lockfile matches the project metadata, use the `--locked` flag. If the lockfile is not
up-to-date, an error will be raised instead of updating the lockfile.

You can also check if the lockfile is up-to-date by passing the `--check` flag to `uv lock`:

```console
$ uv lock --check
```

This is equivalent to the `--locked` flag for other commands.

### Upgrading locked package versions

By default, uv will prefer the locked versions of packages when running `uv sync` and `uv lock` with
an existing `uv.lock` file. Package versions will only change if the project's dependency
constraints exclude the previous, locked version.

To upgrade all packages:

```console
$ uv lock --upgrade
```

To upgrade a single package to the latest version, while retaining the locked versions of all other
packages:

```console
$ uv lock --upgrade-package <package>
```

To upgrade a single package to a specific version:

```console
$ uv lock --upgrade-package <package>==<version>
```

In all cases, upgrades are limited to the project's dependency constraints. For example, if the
project defines an upper bound for a package then an upgrade will not go beyond that version.

!!! note

    uv applies similar logic to Git dependencies. For example, if a Git dependency references
    the `main` branch, uv will prefer the locked commit SHA in an existing `uv.lock` file over
    the latest commit on the `main` branch, unless the `--upgrade` or `--upgrade-package` flags
    are used.
