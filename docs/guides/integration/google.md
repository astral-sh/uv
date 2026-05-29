---
title: Google Artifact Registry
description: Using uv with Google Artifact Registry for installing and publishing Python packages.
---

# Google Artifact Registry

uv can install packages from
[Google Artifact Registry](https://cloud.google.com/artifact-registry/docs) using credentials from
the environment or, on Unix, the [`gcloud`](https://cloud.google.com/sdk/gcloud) CLI. You can also
provide an access token explicitly.

!!! note

    The [`gcloud`](https://cloud.google.com/sdk/gcloud) CLI is only required when using `gcloud`
    credentials or generating an access token explicitly. It is not required when Application
    Default Credentials are otherwise available. Automatic lookup of active `gcloud` credentials is
    not yet supported on Windows.

To use Google Artifact Registry, add the index to your project:

```toml title="pyproject.toml"
[[tool.uv.index]]
name = "private-registry"
url = "https://<REGION>-python.pkg.dev/<PROJECT>/<REPOSITORY>/simple/"
```

## Authenticate with Google credentials

uv automatically searches for credentials when accessing Google Artifact Registry indexes over
HTTPS. It looks for credentials in the following order:

1. [Application Default Credentials](https://cloud.google.com/docs/authentication/application-default-credentials).
2. On Unix, credentials from the [`gcloud`](https://cloud.google.com/sdk/gcloud) CLI.

This uses the same credential lookup order as the
[`keyrings.google-artifactregistry-auth`](https://github.com/GoogleCloudPlatform/artifact-registry-python-tools)
keyring plugin, but does not require the plugin to be installed.

!!! note

    Built-in discovery does not yet support Workforce Identity Federation user credentials
    (`external_account_authorized_user`). Continue to use the Google keyring plugin with uv's
    subprocess keyring provider, active `gcloud` credentials on Unix, or an explicit access token
    for this credential type.

## Authenticate with `keyring` and `keyrings.google-artifactregistry-auth`

To use the Google keyring plugin, pre-install it from a source other than Artifact Registry:

```bash
uv tool install keyring --with keyrings.google-artifactregistry-auth

# Enable keyring authentication
export UV_KEYRING_PROVIDER=subprocess

# Set the username for the index
export UV_INDEX_PRIVATE_REGISTRY_USERNAME=oauth2accesstoken
```

The `keyring` executable must be in the `PATH`, i.e., installed globally or in the active
environment. The [`tool.uv.keyring-provider`](../../reference/settings.md#keyring-provider) setting
can also be used to enable the subprocess keyring provider in your `uv.toml` or `pyproject.toml`.

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

uv uses the same automatic Google credential discovery when publishing. To provide an access token
explicitly instead, configure credentials with:

```console
$ export UV_PUBLISH_USERNAME=oauth2accesstoken
$ export UV_PUBLISH_PASSWORD="$ARTIFACT_REGISTRY_TOKEN"
```

When using the subprocess keyring provider instead, set `UV_PUBLISH_USERNAME=oauth2accesstoken` so
the keyring lookup can find the Google credentials.

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
