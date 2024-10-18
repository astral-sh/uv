# Pythonバージョン

Pythonバージョンは、Pythonインタープリタ（つまり、`python`実行ファイル）、標準ライブラリ、およびその他のサポートファイルで構成されます。

## 管理されたPythonインストールとシステムPythonインストール

システムに既存のPythonインストールがあることが一般的であるため、uvはPythonバージョンの[検出](#discovery-of-python-versions)をサポートしています。ただし、uvはPythonバージョンを自分で[インストール](#installing-a-python-version)することもサポートしています。これらの2つのタイプのPythonインストールを区別するために、uvは自分でインストールしたPythonバージョンを_管理された_ Pythonインストールと呼び、その他のすべてのPythonインストールを_システム_ Pythonインストールと呼びます。

!!! note

    uvは、オペレーティングシステムによってインストールされたPythonバージョンと他のツールによってインストールおよび管理されたPythonバージョンを区別しません。たとえば、Pythonインストールが`pyenv`で管理されている場合でも、uvでは_システム_ Pythonバージョンと見なされます。

## バージョンのリクエスト

特定のPythonバージョンは、ほとんどのuvコマンドで`--python`フラグを使用してリクエストできます。たとえば、仮想環境を作成する場合：

```console
$ uv venv --python 3.11.6
```

uvはPython 3.11.6が利用可能であることを確認し、必要に応じてダウンロードおよびインストールし、そのバージョンで仮想環境を作成します。

次のPythonバージョンリクエスト形式がサポートされています：

- `<version>` 例：`3`、`3.12`、`3.12.3`
- `<version-specifier>` 例：`>=3.12,<3.13`
- `<implementation>` 例：`cpython`または`cp`
- `<implementation>@<version>` 例：`cpython@3.12`
- `<implementation><version>` 例：`cpython3.12`または`cp312`
- `<implementation><version-specifier>` 例：`cpython>=3.12,<3.13`
- `<implementation>-<version>-<os>-<arch>-<libc>` 例：`cpython-3.12.3-macos-aarch64-none`

さらに、特定のシステムPythonインタープリタを次の形式でリクエストできます：

- `<executable-path>` 例：`/opt/homebrew/bin/python3`
- `<executable-name>` 例：`mypython3`
- `<install-dir>` 例：`/some/environment/`

デフォルトでは、uvはシステム上で見つからない場合にPythonバージョンを自動的にダウンロードします。この動作は[`python-downloads`オプション](#disabling-automatic-python-downloads)で無効にできます。

## Pythonバージョンのインストール

uvはmacOS、Linux、およびWindows向けのダウンロード可能なCPythonおよびPyPyディストリビューションのリストをバンドルしています。

!!! tip

    デフォルトでは、Pythonバージョンは`uv python install`を使用せずに必要に応じて自動的にダウンロードされます。

特定のバージョンのPythonバージョンをインストールするには：

```console
$ uv python install 3.12.3
```

最新のパッチバージョンをインストールするには：

```console
$ uv python install 3.12
```

制約を満たすバージョンをインストールするには：

```console
$ uv python install '>=3.8,<3.10'
```

複数のバージョンをインストールするには：

```console
$ uv python install 3.9 3.10 3.11
```

特定の実装をインストールするには：

```console
$ uv python install pypy
```

すべての[Pythonバージョンリクエスト](#requesting-a-version)形式がサポートされていますが、ファイルパスなどのローカルインタープリタをリクエストするために使用される形式は除きます。

## プロジェクトのPythonバージョン

デフォルトでは、`uv python install`は管理されたPythonバージョンがインストールされていることを確認するか、最新バージョンをインストールします。

ただし、プロジェクトにはデフォルトのPythonバージョンを指定する`.python-version`ファイルが含まれている場合があります。存在する場合、uvはファイルに記載されたPythonバージョンをインストールします。

また、複数のPythonバージョンを必要とするプロジェクトは、`.python-versions`ファイルを定義することもできます。存在する場合、uvはファイルに記載されたすべてのPythonバージョンをインストールします。このファイルは`.python-version`ファイルよりも優先されます。

uvはまた、プロジェクトコマンドの呼び出し中に`pyproject.toml`ファイルに定義されたPython要件を尊重します。

## 利用可能なPythonバージョンの表示

インストール済みおよび利用可能なPythonバージョンを一覧表示するには：

```console
$ uv python list
```

デフォルトでは、他のプラットフォームおよび古いパッチバージョンのダウンロードは非表示になります。

すべてのバージョンを表示するには：

```console
$ uv python list --all-versions
```

他のプラットフォームのPythonバージョンを表示するには：

```console
$ uv python list --all-platforms
```

ダウンロードを除外し、インストール済みのPythonバージョンのみを表示するには：

```console
$ uv python list --only-installed
```

## Python実行ファイルの検索

Python実行ファイルを検索するには、`uv python find`コマンドを使用します：

```console
$ uv python find
```

デフォルトでは、最初に利用可能なPython実行ファイルのパスが表示されます。実行ファイルがどのように検出されるかの詳細については、[検出ルール](#discovery-of-python-versions)を参照してください。

このインターフェースは多くの[リクエスト形式](#requesting-a-version)もサポートしています。たとえば、3.11以上のバージョンを持つPython実行ファイルを検索するには：

```console
$ uv python find >=3.11
```

デフォルトでは、`uv python find`は仮想環境からのPythonバージョンを含めます。作業ディレクトリまたは親ディレクトリのいずれかに`.venv`ディレクトリが見つかるか、`VIRTUAL_ENV`環境変数が設定されている場合、それは`PATH`上の他のPython実行ファイルよりも優先されます。

仮想環境を無視するには、`--system`フラグを使用します：

```console
$ uv python find --system
```

## Pythonバージョンの検出

Pythonバージョンを検索する場合、次の場所がチェックされます：

- `UV_PYTHON_INSTALL_DIR`にある管理されたPythonインストール。
- `PATH`上の`python`、`python3`、または`python3.x`としてのPythonインタープリタ（macOSおよびLinuxの場合）または`python.exe`（Windowsの場合）。
- Windowsでは、Windowsレジストリ内のPythonインタープリタおよびMicrosoft StoreのPythonインタープリタ（`py --list-paths`を参照）がリクエストされたバージョンに一致します。

一部のケースでは、uvは仮想環境からのPythonバージョンの使用を許可します。この場合、仮想環境のインタープリタはリクエストとの互換性があるかどうかを確認し、上記のようにインストールを検索する前にチェックされます。詳細については、[pip互換の仮想環境の検出](../pip/environments.md#discovery-of-python-environments)のドキュメントを参照してください。

検出を行う際、実行可能でないファイルは無視されます。検出された各実行ファイルはメタデータを照会して、[リクエストされたPythonバージョン](#requesting-a-version)を満たしていることを確認します。照会が失敗した場合、実行ファイルはスキップされます。実行ファイルがリクエストを満たす場合、追加の実行ファイルを検査せずに使用されます。

管理されたPythonバージョンを検索する場合、uvは新しいバージョンを優先します。システムPythonバージョンを検索する場合、uvは最も新しいバージョンではなく、最初に互換性のあるバージョンを使用します。

システム上でPythonバージョンが見つからない場合、uvは互換性のある管理されたPythonバージョンのダウンロードを確認します。

### Pythonプレリリース

デフォルトでは、Pythonプレリリースは選択されません。Pythonプレリリースは、リクエストに一致する他のインストールがない場合に使用されます。たとえば、プレリリースバージョンのみが利用可能な場合、それが使用されますが、通常は安定リリースバージョンが使用されます。同様に、プレリリースのPython実行ファイルのパスが提供された場合、他のPythonバージョンがリクエストに一致しないため、プレリリースバージョンが使用されます。

プレリリースのPythonバージョンが利用可能でリクエストに一致する場合、uvは代わりに安定したPythonバージョンをダウンロードしません。

## 自動Pythonダウンロードの無効化

デフォルトでは、uvは必要に応じてPythonバージョンを自動的にダウンロードします。

[`python-downloads`オプション](../reference/settings.md#python-downloads)を使用してこの動作を無効にできます。デフォルトでは`automatic`に設定されていますが、`manual`に設定すると、`uv python install`中にのみPythonのダウンロードが許可されます。

!!! tip

    `python-downloads`設定は、デフォルトの動作を変更するために[永続的な構成ファイル](../configuration/files.md)に設定できます。また、任意のuvコマンドに`--no-python-downloads`フラグを渡すこともできます。

## Pythonバージョンの優先順位の調整

デフォルトでは、uvはシステム上で見つかったPythonバージョンを使用し、必要に応じて管理されたインタープリタをダウンロードします。

[`python-preference`オプション](../reference/settings.md#python-preference)を使用してこの動作を調整できます。デフォルトでは`managed`に設定されており、システムPythonインストールよりも管理されたPythonインストールを優先します。ただし、システムPythonインストールは管理されたPythonバージョンのダウンロードよりも優先されます。

次の代替オプションが利用可能です：

- `only-managed`: 管理されたPythonインストールのみを使用し、システムPythonインストールは使用しない
- `system`: システムPythonインストールを管理されたPythonインストールよりも優先する
- `only-system`: システムPythonインストールのみを使用し、管理されたPythonインストールは使用しない

これらのオプションにより、uvの管理されたPythonバージョンを完全に無効にするか、常にそれらを使用し、既存のシステムインストールを無視することができます。

!!! note

    Pythonバージョンの自動ダウンロードは、優先順位を変更せずに[無効にする](#disabling-automatic-python-downloads)ことができます。

## Python実装のサポート

uvはCPython、PyPy、およびGraalPyのPython実装をサポートしています。Python実装がサポートされていない場合、uvはそのインタープリタを検出できません。

実装は長い名前または短い名前のいずれかでリクエストできます：

- CPython: `cpython`、`cp`
- PyPy: `pypy`、`pp`
- GraalPy: `graalpy`、`gp`

実装名のリクエストは大文字と小文字を区別しません。

サポートされている形式の詳細については、[Pythonバージョンリクエスト](#requesting-a-version)のドキュメントを参照してください。

## 管理されたPythonディストリビューション

uvはCPythonおよびPyPyディストリビューションのダウンロードとインストールをサポートしています。

### CPythonディストリビューション

Pythonは公式の配布可能なCPythonバイナリを公開していないため、uvは代わりに[`python-build-standalone`](https://github.com/indygreg/python-build-standalone)プロジェクトからの事前構築されたサードパーティディストリビューションを使用します。`python-build-standalone`は部分的にuvのメンテナによって維持されており、[Rye](https://github.com/astral-sh/rye)や[bazelbuild/rules_python](https://github.com/bazelbuild/rules_python)などの他の多くのPythonプロジェクトで使用されています。

uvのPythonディストリビューションは自己完結型で、高い移植性とパフォーマンスを備えています。Pythonをソースからビルドすることもできますが、`pyenv`などのツールで行う場合、事前にシステム依存関係が必要であり、最適化されたパフォーマンスの高いビルド（例：PGOおよびLTOが有効）を作成するには非常に時間がかかります。

これらのディストリビューションには、一般的に移植性の結果としていくつかの動作の癖があります。現在、uvはAlpine LinuxのようなmuslベースのLinuxディストリビューションへのインストールをサポートしていません。詳細については、[`python-build-standalone`の癖](https://gregoryszorc.com/docs/python-build-standalone/main/quirks.html)のドキュメントを参照してください。

### PyPyディストリビューション

PyPyディストリビューションはPyPyプロジェクトによって提供されています。
