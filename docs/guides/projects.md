# プロジェクトでの作業

uvは、`pyproject.toml`ファイルに依存関係を定義するPythonプロジェクトの管理をサポートしています。

## 新しいプロジェクトの作成

`uv init`コマンドを使用して新しいPythonプロジェクトを作成できます：

```console
$ uv init hello-world
$ cd hello-world
```

または、作業ディレクトリでプロジェクトを初期化することもできます：

```console
$ mkdir hello-world
$ cd hello-world
$ uv init
```

uvは次のファイルを作成します：

```text
.
├── .python-version
├── README.md
├── hello.py
└── pyproject.toml
```

`hello.py`ファイルには、シンプルな「Hello world」プログラムが含まれています。`uv run`で試してみてください：

```console
$ uv run hello.py
Hello from hello-world!
```

## プロジェクト構造

プロジェクトは、いくつかの重要な部分で構成されており、これらが連携してuvがプロジェクトを管理できるようにします。
`uv init`によって作成されたファイルに加えて、uvはプロジェクトコマンド（例：`uv run`、`uv sync`、`uv lock`）を初めて実行するときに、プロジェクトのルートに仮想環境と`uv.lock`ファイルを作成します。

完全なリストは次のようになります：

```text
.
├── .venv
│   ├── bin
│   ├── lib
│   └── pyvenv.cfg
├── .python-version
├── README.md
├── hello.py
├── pyproject.toml
└── uv.lock
```

### `pyproject.toml`

`pyproject.toml`には、プロジェクトに関するメタデータが含まれています：

```toml title="pyproject.toml"
[project]
name = "hello-world"
version = "0.1.0"
description = "Add your description here"
readme = "README.md"
dependencies = []
```

このファイルを使用して依存関係を指定し、プロジェクトの詳細（例：説明やライセンス）を指定します。このファイルは手動で編集することも、`uv add`や`uv remove`などのコマンドを使用してターミナルからプロジェクトを管理することもできます。

!!! tip

    `pyproject.toml`形式の詳細については、公式の[`pyproject.toml`ガイド](https://packaging.python.org/en/latest/guides/writing-pyproject-toml/)を参照してください。

また、このファイルを使用して、[`[tool.uv]`](../reference/settings.md)セクションでuvの[設定オプション](../configuration/files.md)を指定します。

### `.python-version`

`.python-version`ファイルには、プロジェクトのデフォルトのPythonバージョンが含まれています。このファイルは、プロジェクトの仮想環境を作成するときにuvが使用するPythonバージョンを指定します。

### `.venv`

`.venv`フォルダーには、プロジェクトの仮想環境が含まれています。これは、システムの他の部分から分離されたPython環境です。uvはここにプロジェクトの依存関係をインストールします。

詳細については、[プロジェクト環境](../concepts/projects.md#project-environments)のドキュメントを参照してください。

### `uv.lock`

`uv.lock`は、プロジェクトの依存関係に関する正確な情報を含むクロスプラットフォームのロックファイルです。`pyproject.toml`がプロジェクトの広範な要件を指定するのに対し、ロックファイルにはプロジェクト環境にインストールされる正確な解決バージョンが含まれています。このファイルはバージョン管理にチェックインする必要があり、マシン間で一貫性のある再現可能なインストールを可能にします。

`uv.lock`は人間が読めるTOMLファイルですが、uvによって管理されるため手動で編集しないでください。

詳細については、[ロックファイル](../concepts/projects.md#project-lockfile)のドキュメントを参照してください。

## 依存関係の管理

`uv add`コマンドを使用して、`pyproject.toml`に依存関係を追加できます。これにより、ロックファイルとプロジェクト環境も更新されます：

```console
$ uv add requests
```

バージョン制約や代替ソースを指定することもできます：

```console
$ # バージョン制約を指定する
$ uv add 'requests==2.31.0'

$ # Git依存関係を追加する
$ uv add git+https://github.com/psf/requests
```

パッケージを削除するには、`uv remove`を使用します：

```console
$ uv remove requests
```

パッケージをアップグレードするには、`--upgrade-package`フラグを使用して`uv lock`を実行します：

```console
$ uv lock --upgrade-package requests
```

`--upgrade-package`フラグは、ロックファイルの他の部分をそのままにして、指定されたパッケージを最新の互換バージョンに更新しようとします。

詳細については、[依存関係の管理](../concepts/projects.md#managing-dependencies)のドキュメントを参照してください。

## コマンドの実行

`uv run`を使用して、プロジェクト環境で任意のスクリプトやコマンドを実行できます。

`uv run`の呼び出しの前に、uvは`pyproject.toml`とロックファイルが最新であること、および環境がロックファイルと最新であることを確認し、手動の介入なしでプロジェクトを同期状態に保ちます。`uv run`は、コマンドが一貫性のあるロックされた環境で実行されることを保証します。

例えば、`flask`を使用するには：

```console
$ uv add flask
$ uv run -- flask run -p 3000
```

または、スクリプトを実行するには：

```python title="example.py"
# プロジェクト依存関係を必要とする
import flask

print("hello world")
```

```console
$ uv run example.py
```

または、`uv sync`を使用して環境を手動で更新し、コマンドを実行する前にアクティブ化します：

```console
$ uv sync
$ source .venv/bin/activate
$ flask run -p 3000
$ python example.py
```

!!! note

    プロジェクトで`uv run`を使用せずにスクリプトやコマンドを実行するには、仮想環境がアクティブである必要があります。仮想環境のアクティベーションは、シェルやプラットフォームによって異なります。

詳細については、プロジェクトでの[コマンドの実行](../concepts/projects.md#running-commands)および[スクリプトの実行](../concepts/projects.md#running-scripts)のドキュメントを参照してください。

## 配布物のビルド

`uv build`を使用して、プロジェクトのソースディストリビューションおよびバイナリディストリビューション（ホイール）をビルドできます。

デフォルトでは、`uv build`は現在のディレクトリでプロジェクトをビルドし、ビルドされたアーティファクトを`dist/`サブディレクトリに配置します：

```console
$ uv build
$ ls dist/
hello-world-0.1.0-py3-none-any.whl
hello-world-0.1.0.tar.gz
```

詳細については、[プロジェクトのビルド](../concepts/projects.md#building-projects)のドキュメントを参照してください。

## 次のステップ

uvでのプロジェクト作業の詳細については、[プロジェクトの概念](../concepts/projects.md)ページおよび[コマンドリファレンス](../reference/cli.md#uv)を参照してください。

または、[プロジェクトをパッケージとして公開する](./publish.md)方法を学んでください。
