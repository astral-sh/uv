---
title: RepoForge.io
description: Using uv with RepoForge.io for installing and publishing Python packages.
---

# RepoForge.io

uv can install packages from [RepoForge.io](https://repoforge.io), using an access token.

To use it, add the index to your project:

```toml title="pyproject.toml"
[[tool.uv.index]]
name = "repoforge"
url = "https://api.repoforge.io/<hash-id>/"
authenticate = "always"
```

Replace `<hash-id>` with the unique identifier shown in your RepoForge.io repository URL.

Setting `authenticate = "always"` ensures uv searches for credentials before making a request to
RepoForge.io. See the [index authentication](../../concepts/indexes.md#authentication) documentation
for details.

## Authenticate with an access token

Create an access token in the RepoForge.io dashboard with the `Python - Read only` role, then pass
it as the password with `__token__` as the username:

```console
$ export UV_INDEX_REPOFORGE_USERNAME="__token__"
$ export UV_INDEX_REPOFORGE_PASSWORD="$REPOFORGE_TOKEN"
```

!!! note

    Replace `REPOFORGE` in the environment variable names with the actual index name defined in your `pyproject.toml`.

## Publishing packages

Add a `publish-url` to your index definition:

```toml title="pyproject.toml"
[[tool.uv.index]]
name = "repoforge"
url = "https://api.repoforge.io/<hash-id>/"
publish-url = "https://api.repoforge.io/<hash-id>/"
authenticate = "always"
```

Publishing requires an access token with write permissions. With the index credentials set as shown
above, publish by index name:

```console
$ uv publish --index repoforge
```

!!! important

    When publishing with `--index`, uv uses the index credentials to check for files that have
    already been uploaded. If you pass `--token` or set `UV_PUBLISH_PASSWORD` without also setting
    the index credentials, uv will fail with a missing credentials error.

Alternatively, publish without an index definition by passing the URL and token directly:

```console
$ uv publish --publish-url https://api.repoforge.io/<hash-id>/ --token "$REPOFORGE_TOKEN"
```
