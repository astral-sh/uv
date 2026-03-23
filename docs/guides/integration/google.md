---
title: Google Artifact Registry
description: Using uv with Google Artifact Registry for installing and publishing Python packages.
---

# Google Artifact Registry

uv can install packages from
[Google Artifact Registry](https://cloud.google.com/artifact-registry/docs), either by using an
access token, or using the [`keyring`](https://github.com/jaraco/keyring) package.

!!! note

    This guide assumes that [`gcloud`](https://cloud.google.com/sdk/gcloud) CLI is installed and
    authenticated.

To use Google Artifact Registry, add the index to your project:

```toml title="pyproject.toml"
[[tool.uv.index]]
name = "private-registry"
url = "https://<REGION>-python.pkg.dev/<PROJECT>/<REPOSITORY>/simple/"
```

## Authenticate with a Google access token

Credentials can be provided via "Basic" HTTP authentication scheme. Include access token in the
password field of the URL. Username must be `oauth2accesstoken`, otherwise authentication will fail.

Generate a token with `gcloud`:

```bash
export ARTIFACT_REGISTRY_TOKEN=$(
    gcloud auth application-default print-access-token
)
```

!!! note

    You might need to pass extra parameters to properly generate the token (like `--project`), this
    is a basic example.

Then set credentials for the index with:

```bash
export UV_INDEX_PRIVATE_REGISTRY_USERNAME=oauth2accesstoken
export UV_INDEX_PRIVATE_REGISTRY_PASSWORD="$ARTIFACT_REGISTRY_TOKEN"
```

!!! note

    `PRIVATE_REGISTRY` should match the name of the index defined in your `pyproject.toml`.

## Authenticate with `keyring` and `keyrings.google-artifactregistry-auth`

You can also authenticate to Artifact Registry using [`keyring`](https://github.com/jaraco/keyring)
package with the
[`keyrings.google-artifactregistry-auth` plugin](https://github.com/GoogleCloudPlatform/artifact-registry-python-tools).
Because these two packages are required to authenticate to Artifact Registry, they must be
pre-installed from a source other than Artifact Registry.

The `keyrings.google-artifactregistry-auth` plugin wraps
[gcloud CLI](https://cloud.google.com/sdk/gcloud) to generate short-lived access tokens, securely
store them in system keyring, and refresh them when they are expired.

uv only supports using the `keyring` package in
[subprocess mode](../../reference/settings.md#keyring-provider). The `keyring` executable must be in
the `PATH`, i.e., installed globally or in the active environment. The `keyring` CLI requires a
username in the URL and it must be `oauth2accesstoken`.

```bash
# Pre-install keyring and Artifact Registry plugin from the public PyPI
uv tool install keyring --with keyrings.google-artifactregistry-auth

# Enable keyring authentication
export UV_KEYRING_PROVIDER=subprocess

# Set the username for the index
export UV_INDEX_PRIVATE_REGISTRY_USERNAME=oauth2accesstoken
```

!!! note

    The [`tool.uv.keyring-provider`](../../reference/settings.md#keyring-provider)
    setting can be used to enable keyring in your `uv.toml` or `pyproject.toml`.

    Similarly, the username for the index can be added directly to the index URL.

## Publishing packages

If you also want to publish your own packages to Google Artifact Registry, you can use `uv publish`
as described in the [Building and publishing guide](../package.md).

First, add a `publish-url` to the index you want to publish packages to. For example:

```toml title="pyproject.toml" hl_lines="4"
[[tool.uv.index]]
name = "private-registry"
url = "https://<REGION>-python.pkg.dev/<PROJECT>/<REPOSITORY>/simple/"
publish-url = "https://<REGION>-python.pkg.dev/<PROJECT>/<REPOSITORY>/"
```

Then, configure credentials (if not using keyring):

```console
$ export UV_PUBLISH_USERNAME=oauth2accesstoken
$ export UV_PUBLISH_PASSWORD="$ARTIFACT_REGISTRY_TOKEN"
```

And publish the package:

```console
$ uv publish --index private-registry
```

To use `uv publish` without adding the `publish-url` to the project, you can set `UV_PUBLISH_URL`:

```console
$ export UV_PUBLISH_URL=https://<REGION>-python.pkg.dev/<PROJECT>/<REPOSITORY>/
$ uv publish
```

Note this method is not preferable because uv cannot check if the package is already published
before uploading artifacts.
