---
title: Azure Artifacts
description: Using uv with Azure Artifacts for installing and publishing Python packages.
---

# Azure Artifacts

uv can install packages from
[Azure Artifacts](https://learn.microsoft.com/en-us/azure/devops/artifacts/start-using-azure-artifacts?view=azure-devops&tabs=nuget%2Cnugetserver),
either by using a
[Personal Access Token](https://learn.microsoft.com/en-us/azure/devops/organizations/accounts/use-personal-access-tokens-to-authenticate?view=azure-devops&tabs=Windows)
(PAT), or using the [`keyring`](https://github.com/jaraco/keyring) package.

To use Azure Artifacts, add the index to your project:

```toml title="pyproject.toml"
[[tool.uv.index]]
name = "private-registry"
url = "https://pkgs.dev.azure.com/<ORGANIZATION>/<PROJECT>/_packaging/<FEED>/pypi/simple/"
```

## Authenticate with an Azure access token

If there is a personal access token (PAT) available (e.g.,
[`$(System.AccessToken)` in an Azure pipeline](https://learn.microsoft.com/en-us/azure/devops/pipelines/build/variables?view=azure-devops&tabs=yaml#systemaccesstoken)),
credentials can be provided via "Basic" HTTP authentication scheme. Include the PAT in the password
field of the URL. A username must be included as well, but can be any string.

For example, with the token stored in the `$AZURE_ARTIFACTS_TOKEN` environment variable, set
credentials for the index with:

```bash
export UV_INDEX_PRIVATE_REGISTRY_USERNAME=dummy
export UV_INDEX_PRIVATE_REGISTRY_PASSWORD="$AZURE_ARTIFACTS_TOKEN"
```

!!! note

    `PRIVATE_REGISTRY` should match the name of the index defined in your `pyproject.toml`.

## Authenticate with `keyring` and `artifacts-keyring`

You can also authenticate to Artifacts using [`keyring`](https://github.com/jaraco/keyring) package
with the [`artifacts-keyring` plugin](https://github.com/Microsoft/artifacts-keyring). Because these
two packages are required to authenticate to Azure Artifacts, they must be pre-installed from a
source other than Artifacts.

The `artifacts-keyring` plugin wraps the
[Azure Artifacts Credential Provider tool](https://github.com/microsoft/artifacts-credprovider). The
credential provider supports a few different authentication modes including interactive login — see
the [tool's documentation](https://github.com/microsoft/artifacts-credprovider) for information on
configuration.

uv only supports using the `keyring` package in
[subprocess mode](../../reference/settings.md#keyring-provider). The `keyring` executable must be in
the `PATH`, i.e., installed globally or in the active environment. The `keyring` CLI requires a
username in the URL, and it must be `VssSessionToken`.

```bash
# Pre-install keyring and the Artifacts plugin from the public PyPI
uv tool install keyring --with artifacts-keyring

# Enable keyring authentication
export UV_KEYRING_PROVIDER=subprocess

# Set the username for the index
export UV_INDEX_PRIVATE_REGISTRY_USERNAME=VssSessionToken
```

!!! note

    The [`tool.uv.keyring-provider`](../../reference/settings.md#keyring-provider)
    setting can be used to enable keyring in your `uv.toml` or `pyproject.toml`.

    Similarly, the username for the index can be added directly to the index URL.

## Publishing packages

If you also want to publish your own packages to Azure Artifacts, you can use `uv publish` as
described in the [Building and publishing guide](../package.md).

First, add a `publish-url` to the index you want to publish packages to. For example:

```toml title="pyproject.toml" hl_lines="4"
[[tool.uv.index]]
name = "private-registry"
url = "https://pkgs.dev.azure.com/<ORGANIZATION>/<PROJECT>/_packaging/<FEED>/pypi/simple/"
publish-url = "https://pkgs.dev.azure.com/<ORGANIZATION>/<PROJECT>/_packaging/<FEED>/pypi/upload/"
```

Then, configure credentials (if not using keyring):

```console
$ export UV_PUBLISH_USERNAME=dummy
$ export UV_PUBLISH_PASSWORD="$AZURE_ARTIFACTS_TOKEN"
```

And publish the package:

```console
$ uv publish --index private-registry
```

To use `uv publish` without adding the `publish-url` to the project, you can set `UV_PUBLISH_URL`:

```console
$ export UV_PUBLISH_URL=https://pkgs.dev.azure.com/<ORGANIZATION>/<PROJECT>/_packaging/<FEED>/pypi/upload/
$ uv publish
```

Note this method is not preferable because uv cannot check if the package is already published
before uploading artifacts.
