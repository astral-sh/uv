# 依存関係の指定

uvでは、プロジェクトの依存関係は2つの`pyproject.toml`テーブルに宣言されます：`project.dependencies`と`tool.uv.sources`です。

`project.dependencies`は、PyPIにアップロードする際やホイールをビルドする際に伝播される、標準に準拠した依存関係のメタデータを定義します。

`tool.uv.sources`は、開発中に組み込まれる追加のソースで依存関係のメタデータを強化します。依存関係のソースは、Gitリポジトリ、URL、ローカルパス、または代替レジストリである可能性があります。

`tool.uv.sources`は、uvが`project.dependencies`標準でサポートされていない編集可能なインストールや相対パスなどの一般的なパターンをサポートすることを可能にします。例えば：

```toml title="pyproject.toml"
[project]
name = "albatross"
version = "0.1.0"
dependencies = [
  "bird-feeder",
]

[tool.uv.sources]
bird-feeder = { path = "./packages/bird-feeder" }
```

## プロジェクトの依存関係

`project.dependencies`テーブルは、PyPIにアップロードする際やホイールをビルドする際に使用される依存関係を表します。個々の依存関係は[PEP 508](#pep-508)構文を使用して指定され、テーブルは[PEP 621](https://packaging.python.org/en/latest/specifications/pyproject-toml/)標準に従います。

`project.dependencies`は、プロジェクトに必要なパッケージのリストと、それらをインストールする際に使用するバージョン制約を定義します。各エントリには依存関係の名前とバージョンが含まれます。エントリには、プラットフォーム固有のパッケージのためのエクストラや環境マーカーが含まれる場合があります。例えば：

```toml title="pyproject.toml"
[project]
name = "albatross"
version = "0.1.0"
dependencies = [
  # この範囲内の任意のバージョン
  "tqdm >=4.66.2,<5",
  # 正確にこのバージョンのtorch
  "torch ==2.2.2",
  # torchエクストラ付きのtransformersをインストール
  "transformers[torch] >=4.39.3,<5",
  # 古いPythonバージョンでのみこのパッケージをインストール
  # 詳細は「環境マーカー」を参照
  "importlib_metadata >=7.1.0,<8; python_version < '3.10'",
  "mollymawk ==0.1.0"
]
```

プロジェクトが標準パッケージインデックスからのパッケージのみを必要とする場合、`project.dependencies`で十分です。プロジェクトがGit、リモートURL、またはローカルソースからのパッケージに依存している場合、`tool.uv.sources`を使用して、標準に準拠した`project.dependencies`テーブルからの逸脱なしに依存関係のメタデータを強化できます。

!!! tip

    CLIから`pyproject.toml`に依存関係を追加、削除、または更新するには、[プロジェクト](./projects.md#managing-dependencies)のドキュメントを参照してください。

## 依存関係のソース

開発中に、プロジェクトはPyPIで利用できないパッケージに依存することがあります。uvがサポートする追加のソースは次のとおりです：

- インデックス：特定のパッケージインデックスから解決されたパッケージ。
- Git：Gitリポジトリ。
- URL：リモートホイールまたはソースディストリビューション。
- パス：ローカルホイール、ソースディストリビューション、またはプロジェクトディレクトリ。
- ワークスペース：現在のワークスペースのメンバー。

非uvプロジェクトがGitまたはパス依存関係としてソースを持つプロジェクトを使用する場合、`project.dependencies`および`project.optional-dependencies`のみが尊重されることに注意してください。ソーステーブルに提供された情報は、他のパッケージマネージャーに固有の形式で再指定する必要があります。

uvに`tool.uv.sources`テーブルを無視するよう指示するには（例：パッケージの公開されたメタデータで解決をシミュレートするため）、`--no-sources`フラグを使用します：

```console
$ uv lock --no-sources
```

`--no-sources`の使用は、uvが特定の依存関係を満たす可能性のある[ワークスペースメンバー](#workspace-member)を発見するのを防ぎます。

### インデックス

Pythonパッケージを特定のインデックスに固定するには、`pyproject.toml`に名前付きインデックスを追加します：

```toml title="pyproject.toml"
[project]
dependencies = [
  "torch",
]

[tool.uv.sources]
torch = { index = "pytorch" }

[[tool.uv.index]]
name = "pytorch"
url = "https://download.pytorch.org/whl/cpu"
explicit = true
```

`explicit`フラグはオプションであり、インデックスが`tool.uv.sources`で明示的に指定されたパッケージにのみ使用されるべきことを示します。`explicit`が設定されていない場合、他のパッケージが他の場所で見つからない場合にインデックスから解決されることがあります。

### Git

Git依存関係のソースを追加するには、クローンするGit互換URLに`git+`をプレフィックスします。

例えば：

```console
$ uv add git+https://github.com/encode/httpx
```

は、次のような`pyproject.toml`になります：

```toml title="pyproject.toml"
[project]
dependencies = [
    "httpx",
]

[tool.uv.sources]
httpx = { git = "https://github.com/encode/httpx" }
```

リビジョン、タグ、またはブランチも含めることができます：

```console
$ uv add git+https://github.com/encode/httpx --tag 0.27.0
$ uv add git+https://github.com/encode/httpx --branch main
$ uv add git+https://github.com/encode/httpx --rev 326b943
```

Git依存関係は、`{ git = <url> }`構文を使用して`pyproject.toml`に手動で追加または編集することもできます。ターゲットリビジョンは、`rev`、`tag`、または`branch`のいずれかで指定できます。パッケージがリポジトリのルートにない場合は、`subdirectory`を指定できます。

### URL

URLソースを追加するには、ホイール（`.whl`で終わる）またはソースディストリビューション（通常は`.tar.gz`または`.zip`で終わる）への`https://` URLを提供します（サポートされているすべての形式については[こちら](../concepts/resolution.md#source-distribution)を参照）。

例えば：

```console
$ uv add "https://files.pythonhosted.org/packages/5c/2d/3da5bdf4408b8b2800061c339f240c1802f2e82d55e50bd39c5a881f47f0/httpx-0.27.0.tar.gz"
```

は、次のような`pyproject.toml`になります：

```toml title="pyproject.toml"
[project]
dependencies = [
    "httpx",
]

[tool.uv.sources]
httpx = { url = "https://files.pythonhosted.org/packages/5c/2d/3da5bdf4408b8b2800061c339f240c1802f2e82d55e50bd39c5a881f47f0/httpx-0.27.0.tar.gz" }
```

URL依存関係は、`{ url = <url> }`構文を使用して`pyproject.toml`に手動で追加または編集することもできます。ソースディストリビューションがアーカイブのルートにない場合は、`subdirectory`を指定できます。

### パス

パスソースを追加するには、ホイール（`.whl`で終わる）、ソースディストリビューション（通常は`.tar.gz`または`.zip`で終わる）、または`pyproject.toml`を含むディレクトリのパスを提供します（サポートされているすべての形式については[こちら](../concepts/resolution.md#source-distribution)を参照）。

例えば：

```console
$ uv add /example/foo-0.1.0-py3-none-any.whl
```

は、次のような`pyproject.toml`になります：

```toml title="pyproject.toml"
[project]
dependencies = [
    "foo",
]

[tool.uv.sources]
foo = { path = "/example/foo-0.1.0-py3-none-any.whl" }
```

パスは相対パスでもかまいません：

```console
$ uv add ./foo-0.1.0-py3-none-any.whl
```

または、プロジェクトディレクトリへのパス：

```console
$ uv add ~/projects/bar/
```

!!! important

    パス依存関係にはデフォルトで[編集可能なインストール](#editable-dependencies)は使用されません。プロジェクトディレクトリに対して編集可能なインストールを要求することができます：

    ```console
    $ uv add --editable ~/projects/bar/
    ```

    ただし、手動のパス依存関係の代わりに[_ワークスペース_](./workspaces.md)を使用することをお勧めします。

### ワークスペースメンバー

ワークスペースメンバーに依存関係を宣言するには、メンバー名を`{ workspace = true }`で追加します。すべてのワークスペースメンバーは明示的に記載する必要があります。ワークスペースメンバーは常に[編集可能](#editable-dependencies)です。ワークスペースの詳細については、[ワークスペース](./workspaces.md)のドキュメントを参照してください。

```toml title="pyproject.toml"
[project]
dependencies = [
  "mollymawk ==0.1.0"
]

[tool.uv.sources]
mollymawk = { workspace = true }

[tool.uv.workspace]
members = [
  "packages/mollymawk"
]
```

### プラットフォーム固有のソース

ソースを特定のプラットフォームやPythonバージョンに限定するには、ソースに[PEP 508](https://peps.python.org/pep-0508/#environment-markers)互換の環境マーカーを提供します。

例えば、macOSでのみGitHubから`httpx`を取得するには、次のようにします：

```toml title="pyproject.toml"
[project]
dependencies = [
  "httpx",
]

[tool.uv.sources]
httpx = { git = "https://github.com/encode/httpx", tag = "0.27.2", marker = "sys_platform == 'darwin'" }
```

ソースにマーカーを指定することで、uvはすべてのプラットフォームで`httpx`を含めますが、macOSではGitHubからソースをダウンロードし、他のすべてのプラットフォームではPyPIにフォールバックします。

### 複数のソース

単一の依存関係に対して複数のソースを指定するには、[PEP 508](https://peps.python.org/pep-0508/#environment-markers)互換の環境マーカーで区別されたソースのリストを提供します。

例えば、macOSとLinuxで異なる`httpx`コミットを取得するには：

```toml title="pyproject.toml"
[project]
dependencies = [
  "httpx",
]

[tool.uv.sources]
httpx = [
  { git = "https://github.com/encode/httpx", tag = "0.27.2", marker = "sys_platform == 'darwin'" },
  { git = "https://github.com/encode/httpx", tag = "0.24.1", marker = "sys_platform == 'linux'" },
]
```

この戦略は、環境マーカーに基づいて異なるインデックスからパッケージを取得することにも拡張されます。例えば、プラットフォームに基づいて異なるPyTorchインデックスから`torch`を取得するには：

```toml title="pyproject.toml"
[project]
dependencies = ["torch"]

[tool.uv.sources]
torch = [
  { index = "torch-cu118", marker = "sys_platform == 'darwin'"},
  { index = "torch-cu124", marker = "sys_platform != 'darwin'"},
]

[[tool.uv.index]]
name = "torch-cu118"
url = "https://download.pytorch.org/whl/cu118"

[[tool.uv.index]]
name = "torch-cu124"
url = "https://download.pytorch.org/whl/cu124"

```

## オプションの依存関係

ライブラリとして公開されるプロジェクトでは、デフォルトの依存関係ツリーを減らすためにいくつかの機能をオプションにすることが一般的です。例えば、Pandasには、Excelパーサーや`matplotlib`を明示的に必要としない限りインストールしないようにするための[`excel`エクストラ](https://pandas.pydata.org/docs/getting_started/install.html#excel-files)や[`plot`エクストラ](https://pandas.pydata.org/docs/getting_started/install.html#visualization)があります。エクストラは`package[<extra>]`構文を使用して要求されます。例えば、`pandas[plot, excel]`です。

オプションの依存関係は、[PEP 508](#pep-508)構文に従って、エクストラ名からその依存関係へのマッピングを行うTOMLテーブルである`[project.optional-dependencies]`に指定されます。

オプションの依存関係は、通常の依存関係と同様に`tool.uv.sources`にエントリを持つことができます。

```toml title="pyproject.toml"
[project]
name = "pandas"
version = "1.0.0"

[project.optional-dependencies]
plot = [
  "matplotlib>=3.6.3"
]
excel = [
  "odfpy>=1.4.1",
  "openpyxl>=3.1.0",
  "python-calamine>=0.1.7",
  "pyxlsb>=1.0.10",
  "xlrd>=2.0.1",
  "xlsxwriter>=3.0.5"
]
```

オプションの依存関係を追加するには、`--optional <extra>`オプションを使用します：

```console
$ uv add httpx --optional network
```

## 開発依存関係

オプションの依存関係とは異なり、開発依存関係はローカルのみであり、PyPIや他のインデックスに公開される際にはプロジェクトの要件に含まれません。そのため、開発依存関係は`[project]`ではなく`[tool.uv]`に含まれます。

開発依存関係は、通常の依存関係と同様に`tool.uv.sources`にエントリを持つことができます。

```toml title="pyproject.toml"
[tool.uv]
dev-dependencies = [
  "pytest >=8.1.1,<9"
]
```

開発依存関係を追加するには、`--dev`フラグを含めます：

```console
$ uv add ruff --dev
```

## ビルド依存関係

プロジェクトが[Pythonパッケージ](./projects.md#build-systems)として構成されている場合、プロジェクトをビルドするために必要ですが、実行するためには必要ない依存関係を宣言することができます。これらの依存関係は、[PEP 518](https://peps.python.org/pep-0518/)に従って、`build-system.requires`の下の`[build-system]`テーブルに指定されます。

例えば、プロジェクトがビルドバックエンドとして`setuptools`を使用する場合、ビルド依存関係として`setuptools`を宣言する必要があります：

```toml title="pyproject.toml"
[project]
name = "pandas"
version = "0.1.0"

[build-system]
requires = ["setuptools>=42"]
build-backend = "setuptools.build_meta"
```

デフォルトでは、uvはビルド依存関係を解決する際に`tool.uv.sources`を尊重します。例えば、ローカルバージョンの`setuptools`をビルドに使用するには、ソースを`tool.uv.sources`に追加します：

```toml title="pyproject.toml"
[project]
name = "pandas"
version = "0.1.0"

[build-system]
requires = ["setuptools>=42"]
build-backend = "setuptools.build_meta"

[tool.uv.sources]
setuptools = { path = "./packages/setuptools" }
```

パッケージを公開する際には、`tool.uv.sources`が無効になっている場合（他のビルドツール、例えば[`pypa/build`](https://github.com/pypa/build)を使用する場合など）にパッケージが正しくビルドされることを確認するために、`uv build --no-sources`を実行することをお勧めします。

## 編集可能な依存関係

ディレクトリ内のPythonパッケージの通常のインストールは、最初にホイールをビルドし、そのホイールを仮想環境にインストールし、すべてのソースファイルをコピーします。パッケージのソースファイルが編集されると、仮想環境には古いバージョンが含まれます。

編集可能なインストールは、仮想環境内にプロジェクトへのリンク（`.pth`ファイル）を追加することでこの問題を解決し、インタープリタにソースファイルを直接含めるよう指示します。

編集可能にはいくつかの制限があります（主に：ビルドバックエンドがそれをサポートする必要があり、ネイティブモジュールはインポート前に再コンパイルされません）が、仮想環境が常にパッケージの最新の変更を使用するため、開発には便利です。

uvはデフォルトでワークスペースパッケージに対して編集可能なインストールを使用します。

編集可能な依存関係を追加するには、`--editable`フラグを使用します：

```console
$ uv add --editable ./path/foo
```

または、ワークスペースで編集可能な依存関係の使用をオプトアウトするには：

```console
$ uv add --no-editable ./path/foo
```

## PEP 508

[PEP 508](https://peps.python.org/pep-0508/)は依存関係の指定のための構文を定義しています。それは順に：

- 依存関係の名前
- 必要なエクストラ（オプション）
- バージョン指定子
- 環境マーカー（オプション）

バージョン指定子はカンマで区切られ、追加されます。例えば、`foo >=1.2.3,<2,!=1.4.0`は「`foo`のバージョンが1.2.3以上で、2未満で、1.4.0ではない」という意味です。

指定子は必要に応じて末尾にゼロを追加してパディングされるため、`foo ==2`はfoo 2.0.0も一致します。

等号で最後の桁に星を使用できます。例えば、`foo ==2.1.*`は2.1シリーズの任意のリリースを受け入れます。同様に、`~=`は最後の桁が等しいか高い場合に一致します。例えば、`foo ~=1.2`は`foo >=1.2,<2`と等しく、`foo ~=1.2.3`は`foo >=1.2.3,<1.3`と等しいです。

エクストラは名前とバージョンの間の角括弧内でカンマ区切りで指定されます。例えば、`pandas[excel,plot] ==2.2`です。エクストラ名の間の空白は無視されます。

一部の依存関係は特定の環境でのみ必要です。例えば、特定のPythonバージョンやオペレーティングシステムです。例えば、`importlib.metadata`モジュールのために`importlib-metadata`バックポートをインストールするには、`importlib-metadata >=7.1.0,<8; python_version < '3.10'`を使用します。Windowsで`colorama`をインストールするには（他のプラットフォームでは省略）、`colorama >=0.4.6,<5; platform_system == "Windows"`を使用します。

マーカーは`and`、`or`、および括弧で組み合わされます。例えば、`aiohttp >=3.7.4,<4; (sys_platform != 'win32' or implementation_name != 'pypy') and python_version >= '3.10'`です。マーカー内のバージョンは引用符で囲む必要がありますが、マーカーの外側のバージョンは引用符で囲む必要はありません。
