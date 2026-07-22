# Preview features

uv includes opt-in preview features to provide an opportunity for community feedback and increase
confidence that changes are a net-benefit before enabling them for everyone.

## Enabling preview features

To enable all preview features, use the `--preview` flag:

```console
$ uv run --preview ...
```

Or, set the `UV_PREVIEW` environment variable:

```console
$ UV_PREVIEW=1 uv run ...
```

To enable specific preview features, use the `--preview-features` flag:

```console
$ uv run --preview-features foo ...
```

The `--preview-features` flag can be repeated to enable multiple features:

```console
$ uv run --preview-features foo --preview-features bar ...
```

Or, features can be provided in a comma separated list:

```console
$ uv run --preview-features foo,bar ...
```

The `UV_PREVIEW_FEATURES` environment variable can be used similarly, e.g.:

```console
$ UV_PREVIEW_FEATURES=foo,bar uv run ...
```

Preview features can also be enabled in `uv.toml`, or under `[tool.uv]` in `pyproject.toml` and PEP
723 metadata:

```toml
preview-features = ["foo", "bar"]
```

Set `preview-features = true` to enable all preview features.

Some preview features take effect before configuration files are loaded and cannot be enabled from
configuration.

For backwards compatibility, enabling preview features that do not exist will warn, but not error,
regardless of the source.

## Using preview features

Often, preview features can be used without changing any preview settings if the behavior change is
gated by some sort of user interaction, For example, while `pylock.toml` support is in preview, you
can use `uv pip install` with a `pylock.toml` file without additional configuration because
specifying the `pylock.toml` file indicates you want to use the feature. However, a warning will be
displayed that the feature is in preview. The preview feature can be enabled to silence the warning.

## Available preview features

The following preview features are available:

- `add-bounds`: Allows configuring the
  [default bounds for `uv add`](../reference/settings.md#add-bounds) invocations.
- `centralized-project-envs`: Stores
  [project virtual environments](./projects/layout.md#centralized-project-environments) in the uv
  cache.
- `no-distutils-patch`: Stops installing the `_virtualenv.py` / `_virtualenv.pth` distutils
  configuration monkeypatch in virtual environments for Python 3.10 and later.
- `json-output`: Allows `--output-format json` for various uv commands.
- `package-conflicts`: Allows defining workspace conflicts at the package level.
- `pylock`: Allows installing from `pylock.toml` files.
- `python-install-default`: Allows
  [installing `python` and `python3` executables](./python-versions.md#installing-python-executables).
- `format`: Allows using `uv format`.
- `index-exclude-newer`: Allows setting `exclude-newer` on configured package indexes.
- `index-hash-algorithm`: Allows requiring a hash algorithm for configured package indexes.
- `azure-endpoint`: Allows signing requests to Azure Blob Storage endpoints with Azure credentials.
- `native-auth`: Enables storage of credentials in a
  [system-native location](../concepts/authentication/http.md#the-uv-credentials-store).
- `auth-helper`: Allows using `uv auth helper` as a credential helper for external tools.
- `workspace-metadata`: Allows using `uv workspace metadata`.
- `workspace-dir`: Allows using `uv workspace dir`.
- `workspace-list`: Allows using `uv workspace list`.
- `target-workspace-discovery`: Uses the directory containing a local `uv run` target, rather than
  the current working directory, as the starting point for project and workspace discovery. This
  feature takes effect before configuration is loaded.
- `project-directory-must-exist`: Rejects an invalid `--project` path instead of warning and
  continuing. Except for `uv init`, the path must already exist as a directory or point to a
  `pyproject.toml` file. This feature takes effect before configuration is loaded.
- `malware-check`: Allows `uv sync` and other commands to check for malware using
  [OSV](https://osv.dev) before installing packages.

## Disabling preview features

The `--no-preview` option can be used to disable preview features.
