# Locking and syncing

Locking is the process of resolving your project's dependencies into a
[lockfile](./layout.md#the-lockfile). Syncing is the process of installing a subset of packages from
the lockfile into the [project environment](./layout.md#the-project-environment).

## Automatic lock and sync

Locking and syncing are _automatic_ in uv. For example, when `uv run` is used, the project is locked
and synced before invoking the requested command. This ensures the project environment is always
up-to-date. Similarly, commands which read the lockfile, such as `uv tree`, will automatically
update it before running.

To disable automatic locking, use the `--locked` option:

```console
$ uv run --locked ...
```

If the lockfile is not up-to-date, uv will raise an error instead of updating the lockfile.

To use the lockfile without checking if it is up-to-date, use the `--frozen` option:

```console
$ uv run --frozen ...
```

Similarly, to run a command without checking if the environment is up-to-date, use the `--no-sync`
option:

```console
$ uv run --no-sync ...
```

## Checking if the lockfile is up-to-date

When considering if the lockfile is up-to-date, uv will check if it matches the project metadata.
For example, if you add a dependency to your `pyproject.toml`, the lockfile will be considered
outdated. Similarly, if you change the version constraints for a dependency such that the locked
version is excluded, the lockfile will be considered outdated. However, if you change the version
constraints such that the existing locked version is still included, the lockfile will still be
considered up-to-date.

You can check if the lockfile is up-to-date by passing the `--check` flag to `uv lock`:

```console
$ uv lock --check
```

This is equivalent to the `--locked` flag for other commands.

!!! important

    uv will not consider lockfiles outdated when new versions of packages are released — the lockfile
    needs to be explicitly updated if you want to upgrade dependencies. See the documentation on
    [upgrading locked package versions](#upgrading-locked-package-versions) for details.

## Creating the lockfile

While the lockfile is created [automatically](#automatic-lock-and-sync), the lockfile may also be
explicitly created or updated using `uv lock`:

```console
$ uv lock
```

## Syncing the environment

While the environment is synced [automatically](#automatic-lock-and-sync), it may also be explicitly
synced using `uv sync`:

```console
$ uv sync
```

Syncing the environment manually is especially useful for ensuring your editor has the correct
versions of dependencies.

### Editable installation

When the environment is synced, uv will install the project (and other workspace members) as
_editable_ packages, such that re-syncing is not necessary for changes to be reflected in the
environment.

To opt-out of this behavior, use the `--no-editable` option.

!!! note

    If the project does not define a build system, it will not be installed.
    See the [build systems](./config.md#build-systems) documentation for details.

### Retaining extraneous packages

Syncing is "exact" by default, which means it will remove any packages that are not present in the
lockfile.

To retain extraneous packages, use the `--inexact` option:

```console
$ uv sync --inexact
```

### Syncing optional dependencies

uv reads optional dependencies from the `[project.optional-dependencies]` table. These are
frequently referred to as "extras".

uv does not sync extras by default. Use the `--extra` option to include an extra.

```console
$ uv sync --extra foo
```

To quickly enable all extras, use the `--all-extras` option.

See the [optional dependencies](./dependencies.md#optional-dependencies) documentation for details
on how to manage optional dependencies.

### Syncing development dependencies

uv reads development dependencies from the `[dependency-groups]` table (as defined in
[PEP 735](https://peps.python.org/pep-0735/)).

The `dev` group is special-cased and synced by default. See the
[default groups](./dependencies.md#default-groups) documentation for details on changing the
defaults.

The `--no-dev` flag can be used to exclude the `dev` group.

The `--only-dev` flag can be used to install the `dev` group _without_ the project and its
dependencies.

Additional groups can be included or excluded with the `--all-groups`, `--no-default-groups`,
`--group <name>`, `--only-group <name>`, and `--no-group <name>` options. The semantics of
`--only-group` are the same as `--only-dev`, the project will not be included. However,
`--only-group` will also exclude default groups.

Group exclusions always take precedence over inclusions, so given the command:

```
$ uv sync --no-group foo --group foo
```

The `foo` group would not be installed.

See the [development dependencies](./dependencies.md#development-dependencies) documentation for
details on how to manage development dependencies.

## Upgrading locked package versions

With an existing `uv.lock` file, uv will prefer the previously locked versions of packages when
running `uv sync` and `uv lock`. Package versions will only change if the project's dependency
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

These flags can also be provided to `uv sync` or `uv run` to update the lockfile _and_ the
environment.

## Exporting the lockfile

If you need to integrate uv with other tools or workflows, you can export `uv.lock` to the
`requirements.txt` format with `uv export --format requirements-txt`. The generated
`requirements.txt` file can then be installed via `uv pip install`, or with other tools like `pip`.

In general, we recommend against using both a `uv.lock` and a `requirements.txt` file. If you find
yourself exporting a `uv.lock` file, consider opening an issue to discuss your use case.

## Partial installations

Sometimes it's helpful to perform installations in multiple steps, e.g., for optimal layer caching
while building a Docker image. `uv sync` has several flags for this purpose.

- `--no-install-project`: Do not install the current project
- `--no-install-workspace`: Do not install any workspace members, including the root project
- `--no-install-package <NO_INSTALL_PACKAGE>`: Do not install the given package(s)

When these options are used, all of the dependencies of the target are still installed. For example,
`--no-install-project` will omit the _project_ but not any of its dependencies.

If used improperly, these flags can result in a broken environment since a package can be missing
its dependencies.
