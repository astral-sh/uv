# 代替パッケージインデックスの使用

uvはデフォルトで公式のPythonパッケージインデックス（PyPI）を使用しますが、代替パッケージインデックスもサポートしています。ほとんどの代替インデックスはさまざまな形式の認証を必要とし、初期設定が必要です。

!!! important

    uvで[複数のインデックスを使用する](../../pip/compatibility.md#packages-that-exist-on-multiple-indexes)ドキュメントを読んでください。デフォルトの動作は依存関係の混乱攻撃を防ぐためにpipとは異なりますが、これによりuvが期待通りにパッケージのバージョンを見つけられない場合があります。

## Azure Artifacts

uvは[Azure DevOps Artifacts](https://learn.microsoft.com/en-us/azure/devops/artifacts/start-using-azure-artifacts?view=azure-devops&tabs=nuget%2Cnugetserver)からパッケージをインストールできます。
[Personal Access Token](https://learn.microsoft.com/en-us/azure/devops/organizations/accounts/use-personal-access-tokens-to-authenticate?view=azure-devops&tabs=Windows)（PAT）を使用してフィードに認証するか、[`keyring`](https://github.com/jaraco/keyring)パッケージを使用して対話的に認証します。

### PATの使用

PATが利用可能な場合（例：[Azureパイプラインの`$(System.AccessToken)`](https://learn.microsoft.com/en-us/azure/devops/pipelines/build/variables?view=azure-devops&tabs=yaml#systemaccesstoken)）、資格情報は「Basic」HTTP認証スキームを介して提供できます。URLのパスワードフィールドにPATを含めます。ユーザー名も含める必要がありますが、任意の文字列で構いません。

たとえば、トークンが`$ADO_PAT`環境変数に格納されている場合、次のようにインデックスURLを設定します：

```console
$ export UV_EXTRA_INDEX_URL=https://dummy:$ADO_PAT@pkgs.dev.azure.com/{organisation}/{project}/_packaging/{feedName}/pypi/simple/
```

### `keyring`の使用

PATが利用できない場合、[`keyring`](https://github.com/jaraco/keyring)パッケージと[the `artifacts-keyring` plugin](https://github.com/Microsoft/artifacts-keyring)を使用してArtifactsに認証します。これらの2つのパッケージはAzure Artifactsに認証するために必要であり、Artifacts以外のソースから事前にインストールする必要があります。

`artifacts-keyring`プラグインは[Azure Artifacts Credential Providerツール](https://github.com/microsoft/artifacts-credprovider)をラップします。資格情報プロバイダーは、対話的なログインを含むいくつかの異なる認証モードをサポートしています。設定に関する情報は[ツールのドキュメント](https://github.com/microsoft/artifacts-credprovider)を参照してください。

uvは[subprocessモード](https://github.com/astral-sh/uv/blob/main/PIP_COMPATIBILITY.md#registry-authentication)で`keyring`パッケージを使用することのみをサポートしています。`keyring`実行可能ファイルは`PATH`に含まれている必要があります。つまり、グローバルにインストールするか、アクティブな環境にインストールする必要があります。`keyring` CLIはURLにユーザー名を必要とするため、インデックスURLにはデフォルトのユーザー名`VssSessionToken`を含める必要があります。

```console
$ # 公開PyPIからkeyringとArtifactsプラグインを事前インストール
$ uv tool install keyring --with artifacts-keyring

$ # keyring認証を有効にする
$ export UV_KEYRING_PROVIDER=subprocess

$ # ユーザー名でインデックスURLを設定
$ export UV_EXTRA_INDEX_URL=https://VssSessionToken@pkgs.dev.azure.com/{organisation}/{project}/_packaging/{feedName}/pypi/simple/
```

## AWS CodeArtifact

uvは[AWS CodeArtifact](https://docs.aws.amazon.com/codeartifact/latest/ug/using-python.html)からパッケージをインストールできます。

認証トークンは`awscli`ツールを使用して取得できます。

!!! note

    このガイドは、AWS CLIが事前に認証されていることを前提としています。

まず、CodeArtifactリポジトリの定数を宣言します：

```bash
export AWS_DOMAIN="<your-domain>"
export AWS_ACCOUNT_ID="<your-account-id>"
export AWS_REGION="<your-region>"
export AWS_CODEARTIFACT_REPOSITORY="<your-repository>"
```

次に、`awscli`からトークンを取得します：

```bash
export AWS_CODEARTIFACT_TOKEN="$(
    aws codeartifact get-authorization-token \
    --domain $AWS_DOMAIN \
    --domain-owner $AWS_ACCOUNT_ID \
    --query authorizationToken \
    --output text
)"
```

そして、インデックスURLを設定します：

```bash
export UV_EXTRA_INDEX_URL="https://aws:${AWS_CODEARTIFACT_TOKEN}@${AWS_DOMAIN}-${AWS_ACCOUNT_ID}.d.codeartifact.${AWS_REGION}.amazonaws.com/pypi/${AWS_CODEARTIFACT_REPOSITORY}/simple/"
```

### パッケージの公開

AWS CodeArtifactに独自のパッケージを公開する場合は、[公開ガイド](../publish.md)に記載されているように`uv publish`を使用できます。資格情報とは別に`UV_PUBLISH_URL`を設定する必要があります：

```bash
# uvをAWS CodeArtifactに設定
export UV_PUBLISH_URL="https://${AWS_DOMAIN}-${AWS_ACCOUNT_ID}.d.codeartifact.${AWS_REGION}.amazonaws.com/pypi/${AWS_CODEARTIFACT_REPOSITORY}/"
export UV_PUBLISH_USERNAME=aws
export UV_PUBLISH_PASSWORD="$AWS_CODEARTIFACT_TOKEN"

# パッケージを公開
uv publish
```

## その他のインデックス

uvはJFrogのArtifactoryおよびGoogle Cloud Artifact Registryとも連携することが知られています。
