# Using alternative package indexes

While uv uses the official Python Package Index (PyPI) by default, it also supports alternative
package indexes. Most alternative indexes require various forms of authentication, which requires
some initial setup.

## Azure Artifacts

uv can install packages from
[Azure DevOps Artifacts](https://learn.microsoft.com/en-us/azure/devops/artifacts/start-using-azure-artifacts?view=azure-devops&tabs=nuget%2Cnugetserver).
Authenticate to a feed using a
[Personal Access Token](https://learn.microsoft.com/en-us/azure/devops/organizations/accounts/use-personal-access-tokens-to-authenticate?view=azure-devops&tabs=Windows)
(PAT) or interactively using the [`keyring`](https://github.com/jaraco/keyring) package.

### Using a PAT

If there is a PAT available (eg
[`$(System.AccessToken)` in an Azure pipeline](https://learn.microsoft.com/en-us/azure/devops/pipelines/build/variables?view=azure-devops&tabs=yaml#systemaccesstoken)),
credentials can be provided via the "Basic" HTTP authentication scheme. Include the PAT in the
password field of the URL. A username must be included as well, but can be any string.

For example, with the token stored in the `$ADO_PAT` environment variable, set the index URL with:

```console
$ export UV_EXTRA_INDEX_URL=https://dummy:$ADO_PAT@pkgs.dev.azure.com/{organisation}/{project}/_packaging/{feedName}/pypi/simple/
```

### Using `keyring`

If there is not a PAT available, authenticate to Artifacts using the
[`keyring`](https://github.com/jaraco/keyring) package with
[the `artifacts-keyring` plugin](https://github.com/Microsoft/artifacts-keyring). Because these two
packages are required to authenticate to Azure Artifacts, they must be pre-installed from a source
other than Artifacts.

The `artifacts-keyring` plugin wraps
[the Azure Artifacts Credential Provider tool](https://github.com/microsoft/artifacts-credprovider).
The credential provider supports a few different authentication modes including interactive login —
see [the tool's documentation](https://github.com/microsoft/artifacts-credprovider) for information
on configuration.

uv only supports using the `keyring` package in
[subprocess mode](https://github.com/astral-sh/uv/blob/main/PIP_COMPATIBILITY.md#registry-authentication).
The `keyring` executable must be in the `PATH`, i.e., installed globally or in the active
environment. The `keyring` CLI requires a username in the URL, so the index URL must include the
default username `VssSessionToken`.

```console
$ # Pre-install keyring and the Artifacts plugin from the public PyPI
$ uv tool install keyring --with artifacts-keyring

$ # Enable keyring authentication
$ export UV_KEYRING_PROVIDER=subprocess

$ # Configure the index URL with the username
$ export UV_EXTRA_INDEX_URL=https://VssSessionToken@pkgs.dev.azure.com/{organisation}/{project}/_packaging/{feedName}/pypi/simple/
```

## Other indexes

uv is also known to work with JFrog's Artifactory, the Google Cloud Artifact Registry, and AWS Code
Artifact.
