# Using Azure Artifacts

uv can install packages from [Azure DevOps Artifacts](https://learn.microsoft.com/en-us/azure/devops/artifacts/start-using-azure-artifacts?view=azure-devops&tabs=nuget%2Cnugetserver). You can authenticate to a feed using a [Personal Access Token](https://learn.microsoft.com/en-us/azure/devops/organizations/accounts/use-personal-access-tokens-to-authenticate?view=azure-devops&tabs=Windows) or interactively using Keyring.


## Authenticate using a PAT
If you have a Personal Access Token available (eg [`$(System.AccessToken)` in an Azure pipeline](https://learn.microsoft.com/en-us/azure/devops/pipelines/build/variables?view=azure-devops&tabs=yaml#systemaccesstoken)), you can authenticate to Artifacts using Basic Auth. Simply include the PAT in the password field of the URL. The username can be any dummy string.

```bash
# if your token is in the ADO_PAT environment variable
export UV_EXTRA_INDEX_URL=https://dummy:$ADO_PAT@pkgs.dev.azure.com/{organisation}/{project}/_packaging/{feedName}/pypi/simple/

uv pip install my-private-package
```


## Authenticate using Keyring
If you don’t have a PAT handy, you can authenticate to Artifacts using [`keyring`](https://github.com/jaraco/keyring) with [the `artifacts-keyring` plugin](https://github.com/Microsoft/artifacts-keyring). Because you’ll be using these two packages to authenticate to Azure Artifacts, you should arrange to have them installed into your environment from a source other than Artifacts.

The `artifacts-keyring` plugin wraps [the Azure Artifacts Credential Provider tool](https://github.com/microsoft/artifacts-credprovider). The credential provider supports a few different authentication modes including interactive login — see [the tool's docs](https://github.com/microsoft/artifacts-credprovider) for information on how to configure it.

[uv only supports using Keyring in subprocess mode](https://github.com/astral-sh/uv/blob/main/PIP_COMPATIBILITY.md#registry-authentication). The `keyring` executable must be on the `PATH`, meaning it should be installed globally or into your currently-active virtual environment. Keyring’s CLI requires a username in the URL, so you should modify your index URL to include the default username `VssSessionToken`.

```bash
# preinstall keyring and the Artifacts plugin from the public PyPI
uv pip install keyring artifacts-keyring

# enable uv's keyring integration
export UV_KEYRING_PROVIDER=subprocess

# include default username in URL
export UV_EXTRA_INDEX_URL=https://VssSessionToken@pkgs.dev.azure.com/{organisation}/{project}/_packaging/{feedName}/pypi/simple/

uv pip install my-private-package
```
