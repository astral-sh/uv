# Using alternative package indexes

While uv uses the official Python Package Index (PyPI) by default, it also supports alternative
package indexes. Most alternative indexes require various forms of authentication, which requires
some initial setup.

!!! important

    Please read the documentation on [using multiple indexes](../../pip/compatibility.md#packages-that-exist-on-multiple-indexes)
    in uv — the default behavior is different from pip to prevent dependency confusion attacks, but
    this means that uv may not find the versions of a package as you'd expect.

## Azure Artifacts

uv can install packages from
[Azure DevOps Artifacts](https://learn.microsoft.com/en-us/azure/devops/artifacts/start-using-azure-artifacts?view=azure-devops&tabs=nuget%2Cnugetserver).
Authenticate to a feed using a
[Personal Access Token](https://learn.microsoft.com/en-us/azure/devops/organizations/accounts/use-personal-access-tokens-to-authenticate?view=azure-devops&tabs=Windows)
(PAT) or interactively using the [`keyring`](https://github.com/jaraco/keyring) package.

### Using a PAT

If there is a PAT available (eg
[ `$(System.AccessToken)` in an Azure pipeline](https://learn.microsoft.com/en-us/azure/devops/pipelines/build/variables?view=azure-devops&tabs=yaml#systemaccesstoken)),
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

## AWS CodeArtifact

uv can install packages from
[AWS CodeArtifact](https://docs.aws.amazon.com/codeartifact/latest/ug/using-python.html).

The authorization token can be retrieved using the `awscli` tool.

!!! note

    This guide assumes the AWS CLI has previously been authenticated.

First, declare some constants for your CodeArtifact repository:

```bash
export AWS_DOMAIN="<your-domain>"
export AWS_ACCOUNT_ID="<your-account-id>"
export AWS_REGION="<your-region>"
export AWS_CODEARTIFACT_REPOSITORY="<your-repository>"
```

Then, retrieve a token from the `awscli`:

```bash
export AWS_CODEARTIFACT_TOKEN="$(
    aws codeartifact get-authorization-token \
    --domain $AWS_DOMAIN \
    --domain-owner $AWS_ACCOUNT_ID \
    --query authorizationToken \
    --output text
)"
```

And configure the index URL:

```bash
export UV_EXTRA_INDEX_URL="https://aws:${AWS_CODEARTIFACT_TOKEN}@${AWS_DOMAIN}-${AWS_ACCOUNT_ID}.d.codeartifact.${AWS_REGION}.amazonaws.com/pypi/${AWS_CODEARTIFACT_REPOSITORY}/simple/"
```

### Publishing packages

If you also want to publish your own packages to AWS CodeArtifact, you can use `uv publish` as
described in the [publishing guide](../publish.md). You will need to set `UV_PUBLISH_URL` separately
from the credentials:

```bash
# Configure uv to use AWS CodeArtifact
export UV_PUBLISH_URL="https://${AWS_CODEARTIFACT_TOKEN}@${AWS_DOMAIN}-${AWS_ACCOUNT_ID}.d.codeartifact.${AWS_REGION}.amazonaws.com/pypi/${AWS_CODEARTIFACT_REPOSITORY}/"
export UV_PUBLISH_USERNAME=aws
export UV_PUBLISH_PASSWORD="$AWS_CODEARTIFACT_TOKEN"

# Publish the package
uv publish
```

## Other indexes

uv is also known to work with JFrog's Artifactory and the Google Cloud Artifact Registry.
