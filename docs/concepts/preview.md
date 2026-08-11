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

--8<-- "docs/reference/.preview-features.md"

## Disabling preview features

The `--no-preview` option can be used to disable preview features.
