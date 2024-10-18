# プロジェクト

Pythonプロジェクトは、複数のファイルにまたがるPythonアプリケーションの管理を支援します。

!!! tip

    uvでプロジェクトを作成するための紹介をお探しですか？まず[プロジェクトガイド](../guides/projects.md)をご覧ください。

## プロジェクトメタデータ

Pythonプロジェクトのメタデータは`pyproject.toml`ファイルに定義されています。

!!! tip

    `uv init`を使用して新しいプロジェクトを作成できます。詳細は[プロジェクトの作成](#creating-projects)をご覧ください。

最小限のプロジェクト定義には、名前、バージョン、および説明が含まれます：

```toml title="pyproject.toml"
[project]
name = "example"
version = "0.1.0"
description = "Add your description here"
```

[project]セクションにPythonバージョンの要件を含めることをお勧めしますが、必須ではありません：

```toml title="pyproject.toml"
requires-python = ">=3.12"
```

Pythonバージョンの要件を含めることで、プロジェクトで許可されるPythonの構文が定義され、依存関係のバージョンの選択に影響を与えます（同じPythonバージョン範囲をサポートする必要があります）。

`pyproject.toml`には、`project.dependencies`および`project.optional-dependencies`フィールドにプロジェクトの依存関係もリストされています。uvは、`uv add`および`uv remove`を使用してコマンドラインからプロジェクトの依存関係を変更することをサポートしています。uvはまた、`tool.uv.sources`で[パッケージソース](./dependencies.md)を使用して標準の依存関係定義を拡張することもサポートしています。

!!! tip

    `pyproject.toml`の詳細については、公式の[`pyproject.toml`ガイド](https://packaging.python.org/en/latest/guides/writing-pyproject-toml/)をご覧ください。

## エントリーポイントの定義

uvは、プロジェクトのエントリーポイントを定義するために標準の`[project.scripts]`テーブルを使用します。

たとえば、`example_package_app`モジュールの`hello`関数を呼び出す`hello`というコマンドを宣言するには：

```toml title="pyproject.toml"
[project.scripts]
hello = "example_package_app:hello"
```

!!! important

    `[project.scripts]`を使用するには、[ビルドシステム](#build-systems)を定義する必要があります。

## ビルドシステム

プロジェクトは`pyproject.toml`に`[build-system]`を定義することができます。ビルドシステムは、プロジェクトをどのようにパッケージ化し、インストールするかを定義します。

uvは、プロジェクトにパッケージが含まれており、プロジェクトの仮想環境にインストールする必要があるかどうかを判断するためにビルドシステムの存在を使用します。ビルドシステムが定義されていない場合、uvはプロジェクト自体をビルドまたはインストールしようとせず、その依存関係のみをビルドおよびインストールします。ビルドシステムが定義されている場合、uvはプロジェクトをビルドし、プロジェクト環境にインストールします。デフォルトでは、プロジェクトは[編集可能モード](https://setuptools.pypa.io/en/latest/userguide/development_mode.html)でインストールされるため、ソースコードの変更が即座に反映され、再インストールは不要です。

### プロジェクトパッケージの設定

uvは、`tool.uv.package`設定を使用してプロジェクトのパッケージ化を手動で宣言することもできます。

`tool.uv.package = true`を設定すると、プロジェクトがビルドされ、プロジェクト環境にインストールされます。ビルドシステムが定義されていない場合、uvはsetuptoolsのレガシーバックエンドを使用します。

`tool.uv.package = false`を設定すると、プロジェクトパッケージがビルドおよびプロジェクト環境にインストールされないように強制されます。uvはプロジェクトと対話する際に宣言されたビルドシステムを無視します。

## プロジェクトの作成

uvは`uv init`を使用してプロジェクトを作成することをサポートしています。

uvは作業ディレクトリにプロジェクトを作成するか、名前を指定してターゲットディレクトリにプロジェクトを作成します。例：`uv init foo`。ターゲットディレクトリにすでにプロジェクトが存在する場合、つまり`pyproject.toml`が存在する場合、uvはエラーで終了します。

プロジェクトを作成する際、uvは[**アプリケーション**](#applications)と[**ライブラリ**](#libraries)の2種類を区別します。

デフォルトでは、uvはアプリケーション用のプロジェクトを作成します。代わりにライブラリ用のプロジェクトを作成するには、`--lib`フラグを使用します。

### アプリケーション

アプリケーションプロジェクトは、Webサーバー、スクリプト、およびコマンドラインインターフェースに適しています。

アプリケーションは`uv init`のデフォルトターゲットですが、`--app`フラグを使用して指定することもできます：

```console
$ uv init --app example-app
$ tree example-app
example-app
├── .python-version
├── README.md
├── hello.py
└── pyproject.toml
```

アプリケーションを作成する際、uvは最小限の`pyproject.toml`を生成します。ビルドシステムは定義されておらず、ソースコードはトップレベルディレクトリにあります。例：`hello.py`。プロジェクトには、プロジェクト環境にビルドおよびインストールされるパッケージは含まれていません。

```toml title="pyproject.toml"
[project]
name = "example-app"
version = "0.1.0"
description = "Add your description here"
readme = "README.md"
requires-python = ">=3.11"
dependencies = []
```

作成されたスクリプトには、いくつかの標準的なボイラープレートを含む`main`関数が定義されています：

```python title="hello.py"
def main():
    print("Hello from example-app!")


if __name__ == "__main__":
    main()
```

そして、`uv run`で実行できます：

```console
$ uv run hello.py
Hello from example-project!
```

### ライブラリ

ライブラリは、Pythonパッケージとしてビルドおよび配布することを目的としたプロジェクトです。たとえば、PyPIにアップロードすることができます。ライブラリは、他のプロジェクトが使用するための関数やオブジェクトを提供します。

ライブラリは、`--lib`フラグを使用して作成できます：

```console
$ uv init --lib example-lib
$ tree example-lib
example-lib
├── .python-version
├── README.md
├── pyproject.toml
└── src
    └── example_lib
        ├── py.typed
        └── __init__.py
```

ライブラリを作成する際、uvはビルドシステムを定義し、ソースコードを`src`ディレクトリに配置します。これにより、プロジェクトルートでの`python`呼び出しからライブラリが分離され、配布されるライブラリコードがプロジェクトの他のソースコードから明確に分離されます。プロジェクトには、プロジェクト環境にビルドおよびインストールされるパッケージが`src/example_lib`に含まれています。

```toml title="pyproject.toml"
[project]
name = "example-lib"
version = "0.1.0"
description = "Add your description here"
readme = "README.md"
requires-python = ">=3.11"
dependencies = []

[build-system]
requires = ["hatchling"]
build-backend = "hatchling.build"
```

!!! note

    uvはまだビルドバックエンドを提供していません。デフォルトでは`hatchling`が使用されますが、他のオプションもあります。プロジェクト構造に合わせて`hatchling`を設定するために[hatch build](https://hatch.pypa.io/1.9/config/build/)オプションを使用する必要があるかもしれません。

    uvビルドバックエンドの進捗状況は[astral-sh/uv#3957](https://github.com/astral-sh/uv/issues/3957)で追跡できます。

作成されたモジュールには、シンプルなAPI関数が定義されています：

```python title="__init__.py"
def hello() -> str:
    return "Hello from example-lib!"
```

そして、`uv run`を使用してインポートおよび実行できます：

```console
$ uv run --directory example-lib python -c "import example_lib; print(example_lib.hello())"
Hello from example-lib!
```

異なるビルドバックエンドテンプレートを選択するには、`--build-backend`を使用して`hatchling`、`flit-core`、`pdm-backend`、`setuptools`、`maturin`、または`scikit-build-core`を指定します。

```console
$ uv init --lib --build-backend maturin example-lib
$ tree example-lib
example-lib
├── .python-version
├── Cargo.toml
├── README.md
├── pyproject.toml
└── src
    ├── lib.rs
    └── example_lib
        ├── py.typed
        ├── __init__.py
        └── _core.pyi
```

そして、`uv run`を使用してインポートおよび実行できます：

```console
$ uv run --directory example-lib python -c "import example_lib; print(example_lib.hello())"
Hello from example-lib!
```

!!! tip

バイナリビルドバックエンド（例：`maturin`および`scikit-build-core`）を使用する場合、`lib.rs`や`main.cpp`の変更には`--reinstall`の実行が必要です。

### パッケージ化されたアプリケーション

`--package`フラグを`uv init`に渡すことで、配布可能なアプリケーションを作成できます。例：PyPI経由でコマンドラインインターフェースを公開する場合。uvはプロジェクトのビルドバックエンドを定義し、`[project.scripts]`エントリーポイントを含め、プロジェクトパッケージをプロジェクト環境にインストールします。

プロジェクト構造はライブラリと同じように見えます：

```console
$ uv init --app --package example-packaged-app
$ tree example-packaged-app
example-packaged-app
├── .python-version
├── README.md
├── pyproject.toml
└── src
    └── example_packaged_app
        └── __init__.py
```

しかし、モジュールにはCLI関数が定義されています：

```python title="__init__.py"
def main() -> None:
    print("Hello from example-packaged-app!")
```

そして、`pyproject.toml`にはスクリプトエントリーポイントが含まれています：

```toml title="pyproject.toml" hl_lines="9 10"
[project]
name = "example-packaged-app"
version = "0.1.0"
description = "Add your description here"
readme = "README.md"
requires-python = ">=3.11"
dependencies = []

[project.scripts]
example-packaged-app = "example_packaged_app:main"

[build-system]
requires = ["hatchling"]
build-backend = "hatchling.build"
```

そして、`uv run`で実行できます：

```console
$ uv run --directory example-packaged-app example-packaged-app
Hello from example-packaged-app!
```

!!! tip

    既存のアプリケーションを配布可能なパッケージとして再定義するには、ビルドシステムを追加します。ただし、ビルドバックエンドによってはプロジェクトディレクトリ構造の変更が必要になる場合があります。

さらに、`--build-backend`を指定してバイナリビルドバックエンド（例：`maturin`）を含むパッケージ化されたアプリケーションのビルドバックエンドをカスタマイズできます。

```console
$ uv init --app --package --build-backend maturin example-packaged-app
$ tree example-packaged-app
example-packaged-app
├── .python-version
├── Cargo.toml
├── README.md
├── pyproject.toml
└── src
    ├── lib.rs
    └── example_packaged_app
        ├── __init__.py
        └── _core.pyi
```

そして、`uv run`で実行できます：

```console
$ uv run --directory example-packaged-app example-packaged-app
Hello from example-packaged-app!
```

## プロジェクト環境

uvでプロジェクトを作業する際、必要に応じて仮想環境を作成します。一部のuvコマンドは一時的な環境を作成します（例：`uv run --isolated`）が、uvはプロジェクトとその依存関係を含む永続的な環境も管理します。この環境は、`pyproject.toml`の隣にある`.venv`ディレクトリに保存されます。エディタがコード補完や型ヒントを提供するために環境を見つけやすくするためにプロジェクト内に保存されます。`.venv`ディレクトリをバージョン管理に含めることは推奨されていません。内部の`.gitignore`ファイルで自動的に除外されます。

プロジェクト環境でコマンドを実行するには、`uv run`を使用します。代わりに、仮想環境を通常の仮想環境としてアクティブ化することもできます。

`uv run`が呼び出されると、プロジェクト環境がまだ存在しない場合は作成され、存在する場合は最新の状態に保たれます。プロジェクト環境は`uv sync`を使用して明示的に作成することもできます。

プロジェクト環境を手動で変更することは推奨されません。例：`uv pip install`を使用する場合。プロジェクトの依存関係には、`uv add`を使用してパッケージを環境に追加します。一時的な要件には、[`uvx`](../guides/tools.md)または[`uv run --with`](#running-commands-with-additional-dependencies)を使用します。

!!! tip

    uvにプロジェクト環境を管理させたくない場合、[`managed = false`](../reference/settings.md#managed)を設定してプロジェクトの自動ロックおよび同期を無効にします。例：

    ```toml title="pyproject.toml"
    [tool.uv]
    managed = false
    ```

デフォルトでは、プロジェクトは編集可能モードでインストールされるため、ソースコードの変更が即座に環境に反映されます。`uv sync`および`uv run`の両方は、`--no-editable`フラグを受け入れ、uvに非編集可能モードでプロジェクトをインストールするよう指示します。`--no-editable`は、プロジェクトを展開環境に含める必要があるが、元のソースコードに依存しない場合（例：Dockerコンテナのビルド）に使用されます。

### プロジェクト環境パスの設定

`UV_PROJECT_ENVIRONMENT`環境変数を使用してプロジェクト仮想環境パスを設定できます（デフォルトは`.venv`）。

相対パスが提供された場合、ワークスペースルートに対して解決されます。絶対パスが提供された場合、そのまま使用されます。つまり、環境のための子ディレクトリは作成されません。提供されたパスに環境が存在しない場合、uvはそれを作成します。

このオプションを使用してシステムPython環境に書き込むことができますが、推奨されません。`uv sync`はデフォルトで不要なパッケージを環境から削除するため、システムが壊れる可能性があります。

!!! important

    絶対パスが提供され、この設定が複数のプロジェクトで使用される場合、環境は各プロジェクトの呼び出しによって上書きされます。この設定は、CIまたはDockerイメージの単一プロジェクトでの使用にのみ推奨されます。

!!! note

    uvはプロジェクト操作中に`VIRTUAL_ENV`環境変数を読み取りません。`VIRTUAL_ENV`がプロジェクトの環境とは異なるパスに設定されている場合、警告が表示されます。

## プロジェクトロックファイル

uvは`pyproject.toml`の隣に`uv.lock`ファイルを作成します。

`uv.lock`は、すべての可能なPythonマーカー（例：オペレーティングシステム、アーキテクチャ、Pythonバージョン）にわたってインストールされるパッケージをキャプチャする_ユニバーサル_または_クロスプラットフォーム_ロックファイルです。

`pyproject.toml`がプロジェクトの広範な要件を指定するのに対し、ロックファイルにはプロジェクト環境にインストールされる正確な解決バージョンが含まれています。このファイルはバージョン管理にチェックインする必要があり、マシン間で一貫性のある再現可能なインストールを可能にします。

ロックファイルは、プロジェクトに取り組む開発者が一貫したパッケージバージョンセットを使用していることを保証します。さらに、アプリケーションとしてプロジェクトを展開する際に、使用される正確なパッケージバージョンセットが既知であることを保証します。

ロックファイルは、プロジェクト環境を使用するuvの呼び出し中に作成および更新されます。例：`uv sync`および`uv run`。ロックファイルは`uv lock`を使用して明示的に更新することもできます。

`uv.lock`は人間が読めるTOMLファイルですが、uvによって管理されているため手動で編集しないでください。このファイルの形式はuvに特有であり、他のツールでは使用できません。

!!! tip

    uvを他のツールやワークフローと統合する必要がある場合、`uv.lock`を`requirements.txt`形式にエクスポートできます。`uv export --format requirements-txt`を使用します。生成された`requirements.txt`ファイルは`uv pip install`を介してインストールするか、`pip`などの他のツールでインストールできます。

    一般的には、`uv.lock`ファイルと`requirements.txt`ファイルの両方を使用することはお勧めしません。`uv.lock`ファイルをエクスポートする場合は、ユースケースについて議論するために問題を開くことを検討してください。

### ロックファイルが最新であるかどうかの確認

`uv sync`および`uv run`の呼び出し中にロックファイルを更新しないようにするには、`--frozen`フラグを使用します。

`uv run`の呼び出し中に環境を更新しないようにするには、`--no-sync`フラグを使用します。

ロックファイルがプロジェクトメタデータと一致していることを確認するには、`--locked`フラグを使用します。ロックファイルが最新でない場合、ロックファイルを更新する代わりにエラーが発生します。

### ロックされたパッケージバージョンのアップグレード

デフォルトでは、uvは`uv sync`および`uv lock`の実行時にパッケージのロックされたバージョンを優先します。パッケージバージョンは、プロジェクトの依存関係制約が以前のロックされたバージョンを除外する場合にのみ変更されます。

すべてのパッケージをアップグレードするには：

```console
$ uv lock --upgrade
```

単一のパッケージを最新バージョンにアップグレードするには、他のすべてのパッケージのロックされたバージョンを保持します：

```console
$ uv lock --upgrade-package <package>
```

特定のバージョンに単一のパッケージをアップグレードするには：

```console
$ uv lock --upgrade-package <package>==<version>
```

!!! note

    すべての場合において、アップグレードはプロジェクトの依存関係制約に制限されます。たとえば、プロジェクトがパッケージの上限を定義している場合、アップグレードはそのバージョンを超えません。

### 制限された解決環境

プロジェクトがより限定されたプラットフォームまたはPythonバージョンのセットをサポートしている場合、`environments`設定を使用して解決されるプラットフォームのセットを制約できます。この設定はPEP 508環境マーカーのリストを受け入れます。たとえば、ロックファイルをmacOSおよびLinuxに制約し、Windowsを除外するには：

```toml title="pyproject.toml"
[tool.uv]
environments = [
    "sys_platform == 'darwin'",
    "sys_platform == 'linux'",
]
```

`environments`設定のエントリは互いに排他的である必要があります（つまり、重複してはなりません）。たとえば、`sys_platform == 'darwin'`と`sys_platform == 'linux'`は排他的ですが、`sys_platform == 'darwin'`と`python_version >= '3.9'`は排他的ではありません。両方が同時に真である可能性があるためです。

### オプションの依存関係

uvは、プロジェクトによって宣言されたすべてのオプションの依存関係（「エクストラ」）が互換性があることを要求し、ロックファイルを作成する際にすべてのオプションの依存関係を一緒に解決します。

あるグループで宣言されたオプションの依存関係が別のグループの依存関係と互換性がない場合、uvはプロジェクトの要件を解決できず、エラーが発生します。

!!! note

    現在、互換性のないオプションの依存関係を宣言する方法はありません。サポートの追跡については[astral.sh/uv#6981](https://github.com/astral-sh/uv/issues/6981)をご覧ください。

## 依存関係の管理

uvはCLIを使用して依存関係の追加、更新、および削除を行うことができます。

依存関係を追加するには：

```console
$ uv add httpx
```

uvは[編集可能な依存関係](./dependencies.md#editable-dependencies)、[開発依存関係](./dependencies.md#development-dependencies)、[オプションの依存関係](./dependencies.md#optional-dependencies)、および代替の[依存関係ソース](./dependencies.md#dependency-sources)の追加をサポートしています。詳細については、[依存関係の指定](./dependencies.md)のドキュメントをご覧ください。

依存関係が解決できない場合、uvはエラーを発生させます。例：

```console
$ uv add 'httpx>9999'
error: Because only httpx<=9999 is available and example==0.1.0 depends on httpx>9999, we can conclude that example==0.1.0 cannot be used.
And because only example==0.1.0 is available and you require example, we can conclude that the requirements are unsatisfiable.
```

依存関係を削除するには：

```console
$ uv remove httpx
```

既存の依存関係を更新するには、例：`httpx`バージョンに下限を追加するには：

```console
$ uv add 'httpx>0.1.0'
```

!!! note

    依存関係の「更新」とは、`pyproject.toml`の依存関係の制約を変更することを指します。依存関係のロックされたバージョンは、新しい制約を満たすために必要な場合にのみ変更されます。パッケージバージョンを制約内の最新バージョンに強制的に更新するには、`--upgrade-package <name>`を使用します。例：

    ```console
    $ uv add 'httpx>0.1.0' --upgrade-package httpx
    ```

    パッケージバージョンのアップグレードの詳細については、[ロックファイル](#upgrading-locked-package-versions)セクションをご覧ください。

または、`httpx`の制約を変更するには：

```console
$ uv add 'httpx<0.2.0'
```

依存関係ソースを追加するには、例：開発中にGitHubから`httpx`を使用するには：

```console
$ uv add git+https://github.com/encode/httpx
```

### プラットフォーム固有の依存関係

特定のプラットフォームまたは特定のPythonバージョンでのみ依存関係をインストールするには、Pythonの標準化された[環境マーカー](https://peps.python.org/pep-0508/#environment-markers)構文を使用します。

たとえば、Linuxで`jax`をインストールし、WindowsやmacOSではインストールしない場合：

```console
$ uv add 'jax; sys_platform == "linux"'
```

結果として得られる`pyproject.toml`には、依存関係定義に環境マーカーが含まれます：

```toml title="pyproject.toml" hl_lines="6"
[project]
name = "project"
version = "0.1.0"
requires-python = ">=3.11"
dependencies = ["jax; sys_platform == 'linux'"]
```

同様に、Python 3.11以降で`numpy`を含めるには：

```console
$ uv add 'numpy; python_version >= "3.11"'
```

利用可能なマーカーと演算子の完全な列挙については、Pythonの[環境マーカー](https://peps.python.org/pep-0508/#environment-markers)ドキュメントを参照してください。

## コマンドの実行

プロジェクトで作業する際、プロジェクトは`.venv`に仮想環境としてインストールされます。この環境はデフォルトで現在のシェルから分離されているため、プロジェクトを必要とする呼び出し（例：`python -c "import example"`）は失敗します。代わりに、`uv run`を使用してプロジェクト環境でコマンドを実行します：

```console
$ uv run python -c "import example"
```

`run`を使用する際、uvは指定されたコマンドを実行する前にプロジェクト環境が最新であることを確認します。

指定されたコマンドはプロジェクト環境によって提供されるか、外部に存在することができます。例：

```console
$ # プロジェクトが`example-cli`を提供していると仮定します
$ uv run example-cli foo

$ # プロジェクトが利用可能であることを必要とする`bash`スクリプトを実行します
$ uv run bash scripts/foo.sh
```

### 追加の依存関係を持つコマンドの実行

追加の依存関係や異なるバージョンの依存関係を呼び出しごとに要求できます。

`--with`オプションを使用して呼び出しに依存関係を含めます。例：異なるバージョンの`httpx`を要求するには：

```console
$ uv run --with httpx==0.26.0 python -c "import httpx; print(httpx.__version__)"
0.26.0
$ uv run --with httpx==0.25.0 python -c "import httpx; print(httpx.__version__)"
0.25.0
```

要求されたバージョンはプロジェクトの要件に関係なく尊重されます。たとえば、プロジェクトが`httpx==0.24.0`を必要とする場合でも、上記の出力は同じです。

### スクリプトの実行

インラインメタデータを宣言するスクリプトは、プロジェクトから分離された環境で自動的に実行されます。詳細については、[スクリプトガイド](../guides/scripts.md#declaring-script-dependencies)をご覧ください。

たとえば、次のスクリプトがある場合：

```python title="example.py"
# /// script
# dependencies = [
#   "httpx",
# ]
# ///

import httpx

resp = httpx.get("https://peps.python.org/api/peps.json")
data = resp.json()
print([(k, v["title"]) for k, v in data.items()][:10])
```

`uv run example.py`の呼び出しは、指定された依存関係のみを持つプロジェクトから分離された環境で実行されます。

## 多くのパッケージを含むプロジェクト

多くのパッケージで構成されるプロジェクトで作業する場合、[ワークスペース](./workspaces.md)のドキュメントを参照してください。

## プロジェクトのビルド

プロジェクトを他の人に配布するには（例：PyPIのようなインデックスにアップロードするには）、配布可能な形式にビルドする必要があります。

Pythonプロジェクトは通常、ソースディストリビューション（sdist）とバイナリディストリビューション（ホイール）の両方として配布されます。前者は通常、プロジェクトのソースコードと追加のメタデータを含む`.tar.gz`または`.zip`ファイルであり、後者は直接インストール可能な事前ビルドのアーティファクトを含む`.whl`ファイルです。

`uv build`を使用して、プロジェクトのソースディストリビューションおよびバイナリディストリビューションをビルドできます。デフォルトでは、`uv build`は現在のディレクトリでプロジェクトをビルドし、ビルドされたアーティファクトを`dist/`サブディレクトリに配置します：

```console
$ uv build
$ ls dist/
example-0.1.0-py3-none-any.whl
example-0.1.0.tar.gz
```

`uv build`にパスを指定することで、別のディレクトリでプロジェクトをビルドできます。例：`uv build path/to/project`。

`uv build`は最初にソースディストリビューションをビルドし、そのソースディストリビューションからバイナリディストリビューション（ホイール）をビルドします。

`uv build`を`uv build --sdist`でソースディストリビューションのビルドに限定するか、`uv build --wheel`でバイナリディストリビューションのビルドに限定するか、`uv build --sdist --wheel`でソースから両方のディストリビューションをビルドすることができます。

`uv build`は`--build-constraint`を受け入れ、ビルドプロセス中に任意のビルド要件のバージョンを制約するために使用できます。`--require-hashes`と組み合わせると、uvはプロジェクトのビルドに使用される要件が特定の既知のハッシュと一致することを強制し、再現性を確保します。

たとえば、次の`constraints.txt`がある場合：

```text
setuptools==68.2.2 --hash=sha256:b454a35605876da60632df1a60f736524eb73cc47bbc9f3f1ef1b644de74fd2a
```

次のコマンドを実行すると、指定されたバージョンの`setuptools`を使用してプロジェクトがビルドされ、ダウンロードされた`setuptools`ディストリビューションが指定されたハッシュと一致することが確認されます：

```console
$ uv build --build-constraint constraints.txt --require-hashes
```

## ビルドの分離

デフォルトでは、uvはすべてのパッケージを分離された仮想環境でビルドします。これは[PEP 517](https://peps.python.org/pep-0517/)に準拠しています。一部のパッケージはビルドの分離と互換性がありません。これは意図的な場合（例：重いビルド依存関係の使用、主にPyTorch）や意図しない場合（例：レガシーパッケージ設定の使用）があります。

特定の依存関係のビルド分離を無効にするには、`pyproject.toml`の`no-build-isolation-package`リストに追加します：

```toml title="pyproject.toml"
[project]
name = "project"
version = "0.1.0"
description = "..."
readme = "README.md"
requires-python = ">=3.12"
dependencies = ["cchardet"]

[tool.uv]
no-build-isolation-package = ["cchardet"]
```

ビルド分離なしでパッケージをインストールするには、パッケージのビルド依存関係がパッケージ自体をインストールする前にプロジェクト環境にインストールされている必要があります。これを達成するために、ビルド依存関係とそれを必要とするパッケージを別々のオプショングループに分けます。例：

```toml title="pyproject.toml"
[project]
name = "project"
version = "0.1.0"
description = "..."
readme = "README.md"
requires-python = ">=3.12"
dependencies = []

[project.optional-dependencies]
build = ["setuptools", "cython"]
compile = ["cchardet"]

[tool.uv]
no-build-isolation-package = ["cchardet"]
```

上記の場合、ユーザーは最初に`build`依存関係を同期します：

```console
$ uv sync --extra build
 + cython==3.0.11
 + foo==0.1.0 (from file:///Users/crmarsh/workspace/uv/foo)
 + setuptools==73.0.1
```

次に`compile`依存関係を同期します：

```console
$ uv sync --extra compile
 + cchardet==2.1.7
 - cython==3.0.11
 - setuptools==73.0.1
```

`uv sync --extra compile`はデフォルトで`cython`および`setuptools`パッケージをアンインストールします。ビルド依存関係を保持するには、2回目の`uv sync`呼び出しで両方のエクストラを含めます：

```console
$ uv sync --extra build
$ uv sync --extra build --extra compile
```

一部のパッケージ（例：上記の`cchardet`）は、`uv sync`のインストールフェーズ中にビルド依存関係を必要とします。他のパッケージ（例：`flash-attn`）は、依存関係の解決フェーズ中にプロジェクトのロックファイルを解決するためにビルド依存関係を必要とします。

このような場合、ビルド依存関係は`uv lock`や`uv sync`コマンドを実行する前にインストールする必要があります。これには、低レベルの`uv pip` APIを使用します。例：

```toml title="pyproject.toml"
[project]
name = "project"
version = "0.1.0"
description = "..."
readme = "README.md"
requires-python = ">=3.12"
dependencies = ["flash-attn"]

[tool.uv]
no-build-isolation-package = ["flash-attn"]
```

次のコマンドシーケンスを実行して`flash-attn`を同期します：

```console
$ uv venv
$ uv pip install torch
$ uv sync
```

または、[`dependency-metadata`](../reference/settings.md#dependency-metadata)設定を使用して`flash-attn`メタデータを事前に提供し、依存関係の解決フェーズ中にパッケージをビルドする必要を回避します。例：`flash-attn`メタデータを事前に提供するには、次の内容を`pyproject.toml`に含めます：

```toml title="pyproject.toml"
[[tool.uv.dependency-metadata]]
name = "flash-attn"
version = "2.6.3"
requires-dist = ["torch", "einops"]
```

!!! tip

    `flash-attn`のようなパッケージのメタデータを特定するには、適切なGitリポジトリに移動するか、[PyPI](https://pypi.org/project/flash-attn)で検索し、パッケージのソースディストリビューションをダウンロードします。パッケージの要件は通常、`setup.py`または`setup.cfg`ファイルに記載されています。

    （パッケージにビルド済みディストリビューションが含まれている場合、それを解凍して`METADATA`ファイルを見つけることができます。ただし、ビルド済みディストリビューションが存在する場合、uvはすでにメタデータを利用できるため、事前にメタデータを提供する必要はありません。）

次に、2ステップの`uv sync`プロセスを使用してビルド依存関係をインストールできます。次の`pyproject.toml`がある場合：

```toml title="pyproject.toml"
[project]
name = "project"
version = "0.1.0"
description = "..."
readme = "README.md"
requires-python = ">=3.12"
dependencies = []

[project.optional-dependencies]
build = ["torch", "setuptools", "packaging"]
compile = ["flash-attn"]

[tool.uv]
no-build-isolation-package = ["flash-attn"]

[[tool.uv.dependency-metadata]]
name = "flash-attn"
version = "2.6.3"
requires-dist = ["torch", "einops"]
```

次のコマンドシーケンスを実行して`flash-attn`を同期します：

```console
$ uv sync --extra build
$ uv sync --extra build --extra compile
```
