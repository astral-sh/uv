# uv

非常に高速なPythonパッケージおよびプロジェクトマネージャー、Rustで書かれています。

<p align="center">
  <img alt="Shows a bar chart with benchmark results." src="https://github.com/astral-sh/uv/assets/1309177/629e59c0-9c6e-4013-9ad4-adb2bcf5080d#only-light">
</p>

<p align="center">
  <img alt="Shows a bar chart with benchmark results." src="https://github.com/astral-sh/uv/assets/1309177/03aa9163-1c79-4a87-a31d-7a9311ed9310#only-dark">
</p>

<p align="center">
  <i>ウォームキャッシュで<a href="https://trio.readthedocs.io/">Trio</a>の依存関係をインストールしています。</i>
</p>

## ハイライト

- 🚀 `pip`、`pip-tools`、`pipx`、`poetry`、`pyenv`、`virtualenv`などを置き換える単一のツール。
- ⚡️ `pip`よりも[10-100倍高速](https://github.com/astral-sh/uv/blob/main/BENCHMARKS.md)。
- 🐍 Pythonバージョンを[インストールおよび管理](#python-management)。
- 🛠️ Pythonアプリケーションを[実行およびインストール](#tool-management)。
- ❇️ [スクリプトを実行](#script-support)、[インライン依存関係メタデータ](./guides/scripts.md#declaring-script-dependencies)をサポート。
- 🗂️ [包括的なプロジェクト管理](#project-management)を提供し、[ユニバーサルロックファイル](./concepts/projects.md#project-lockfile)を持つ。
- 🔩 [pip互換インターフェース](#the-pip-interface)を含み、パフォーマンス向上と馴染みのあるCLIを提供。
- 🏢 スケーラブルなプロジェクトのためのCargoスタイルの[ワークスペース](./concepts/workspaces.md)をサポート。
- 💾 依存関係の重複排除のための[グローバルキャッシュ](./concepts/cache.md)を持ち、ディスクスペース効率が高い。
- ⏬ `curl`または`pip`を介してRustやPythonなしでインストール可能。
- 🖥️ macOS、Linux、Windowsをサポート。

uvは[Ruff](https://github.com/astral-sh/ruff)のクリエイターである[Astral](https://astral.sh)によってバックアップされています。

## はじめに

公式のスタンドアロンインストーラーを使用してuvをインストールします：

=== "macOSとLinux"

    ```console
    $ curl -LsSf https://astral.sh/uv/install.sh | sh
    ```

=== "Windows"

    ```console
    $ powershell -c "irm https://astral.sh/uv/install.ps1 | iex"
    ```

次に、[最初のステップ](./getting-started/first-steps.md)を確認するか、以下の概要を読んでください。

!!! tip

    uvはpip、Homebrewなどでもインストールできます。すべての方法は[インストールページ](./getting-started/installation.md)で確認できます。

## プロジェクト管理

uvはプロジェクトの依存関係と環境を管理し、ロックファイル、ワークスペースなどをサポートします。`rye`や`poetry`に似ています：

```console
$ uv init example
プロジェクト`example`を`/home/user/example`に初期化しました

$ cd example

$ uv add ruff
仮想環境を作成しています：.venv
170msで2つのパッケージを解決しました
   Built example @ file:///home/user/example
627msで2つのパッケージを準備しました
1msで2つのパッケージをインストールしました
 + example==0.1.0 (from file:///home/user/example)
 + ruff==0.5.4

$ uv run ruff check
すべてのチェックが合格しました！
```

[プロジェクトガイド](./guides/projects.md)を参照して始めてください。

## ツール管理

uvはPythonパッケージが提供するコマンドラインツールを実行およびインストールします。`pipx`に似ています。

一時的な環境でツールを実行するには、`uvx`（`uv tool run`のエイリアス）を使用します：

```console
$ uvx pycowsay 'hello world!'
167msで1つのパッケージを解決しました
9msで1つのパッケージをインストールしました
 + pycowsay==0.0.0.2
  """

  ------------
< hello world! >
  ------------
   \   ^__^
    \  (oo)\_______
       (__)\       )\/\
           ||----w |
           ||     ||
```

ツールをインストールするには、`uv tool install`を使用します：

```console
$ uv tool install ruff
6msで1つのパッケージを解決しました
2msで1つのパッケージをインストールしました
 + ruff==0.5.4
1つの実行可能ファイルをインストールしました：ruff

$ ruff --version
ruff 0.5.4
```

[ツールガイド](./guides/tools.md)を参照して始めてください。

## Python管理

uvはPythonをインストールし、バージョン間の迅速な切り替えを可能にします。

複数のPythonバージョンをインストールします：

```console
$ uv python install 3.10 3.11 3.12
Python 3.10に一致するバージョンを検索しています
Python 3.11に一致するバージョンを検索しています
Python 3.12に一致するバージョンを検索しています
3.42秒で3つのバージョンをインストールしました
 + cpython-3.10.14-macos-aarch64-none
 + cpython-3.11.9-macos-aarch64-none
 + cpython-3.12.4-macos-aarch64-none
```

必要に応じてPythonバージョンをダウンロードします：

```console
$ uv venv --python 3.12.0
CPython 3.12.0を使用しています
仮想環境を作成しています：.venv
有効化するには：source .venv/bin/activate

$ uv run --python pypy@3.8 -- python
Python 3.8.16 (a9dbdca6fc3286b0addd2240f11d97d8e8de187a, Dec 29 2022, 11:45:30)
[PyPy 7.3.11 with GCC Apple LLVM 13.1.6 (clang-1316.0.21.2.5)] on darwin
Type "help", "copyright", "credits" or "license" for more information.
>>>>
```

現在のディレクトリで特定のPythonバージョンを使用します：

```console
$ uv python pin pypy@3.11
`.python-version`を`pypy@3.11`に固定しました
```

[Pythonのインストールガイド](./guides/install-python.md)を参照して始めてください。

### スクリプトサポート

uvは単一ファイルスクリプトの依存関係と環境を管理します。

新しいスクリプトを作成し、インラインメタデータを追加してその依存関係を宣言します：

```console
$ echo 'import requests; print(requests.get("https://astral.sh"))' > example.py

$ uv add --script example.py requests
`example.py`を更新しました
```

次に、スクリプトを隔離された仮想環境で実行します：

```console
$ uv run example.py
example.pyからインラインスクリプトメタデータを読み取っています
12msで5つのパッケージをインストールしました
<Response [200]>
```

[スクリプトガイド](./guides/scripts.md)を参照して始めてください。

## pipインターフェース

uvは一般的な`pip`、`pip-tools`、および`virtualenv`コマンドのドロップイン置き換えを提供します。

uvは依存関係のバージョンオーバーライド、プラットフォーム非依存の解決、再現可能な解決、代替解決戦略などの高度な機能を備えたインターフェースを拡張します。

既存のワークフローを変更せずにuvに移行し、`uv pip`インターフェースで10-100倍の速度向上を体験してください。

プラットフォーム非依存の要件ファイルに要件をコンパイルします：

```console
$ uv pip compile docs/requirements.in \
   --universal \
   --output-file docs/requirements.txt
12msで43のパッケージを解決しました
```

仮想環境を作成します：

```console
$ uv venv
CPython 3.12.3を使用しています
仮想環境を作成しています：.venv
有効化するには：source .venv/bin/activate
```

ロックされた要件をインストールします：

```console
$ uv pip sync docs/requirements.txt
11msで43のパッケージを解決しました
208msで43のパッケージをインストールしました
 + babel==2.15.0
 + black==24.4.2
 + certifi==2024.7.4
 ...
```

[pipインターフェースのドキュメント](./pip/index.md)を参照して始めてください。

## 詳細を学ぶ

[最初のステップ](./getting-started/first-steps.md)を確認するか、[ガイド](./guides/index.md)に直接ジャンプしてuvの使用を開始してください。
