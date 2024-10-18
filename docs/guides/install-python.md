# Pythonのインストール

Pythonがすでにシステムにインストールされている場合、uvは設定なしでそれを[検出して使用](#using-an-existing-python-installation)します。ただし、uvはPythonのバージョンをインストールおよび管理することもできます。

!!! tip

    uvは必要に応じて[自動的にPythonのバージョンを取得](#automatic-python-downloads)します。Pythonをインストールする必要はありません。

## はじめに

最新のPythonバージョンをインストールするには：

```console
$ uv python install
```

これにより、システムにPythonがすでにインストールされている場合でも、uv管理のPythonバージョンがインストールされます。以前にuvでPythonをインストールしている場合、新しいバージョンはインストールされません。

!!! note

    Pythonは公式の配布可能なバイナリを公開していません。そのため、uvは[`python-build-standalone`](https://github.com/indygreg/python-build-standalone)プロジェクトからのサードパーティのディストリビューションを使用します。このプロジェクトは部分的にuvのメンテナによって維持されており、他の著名なPythonプロジェクト（例：[Rye](https://github.com/astral-sh/rye)、[Bazel](https://github.com/bazelbuild/rules_python)）でも使用されています。詳細については、[Pythonディストリビューション](../concepts/python-versions.md#managed-python-distributions)のドキュメントを参照してください。

Pythonがインストールされると、`uv`コマンドで自動的に使用されます。

!!! important

    uvによってインストールされたPythonは、グローバルには利用できません（つまり、`python`コマンドでは利用できません）。この機能のサポートは将来のリリースで予定されています。それまでの間、[`uv run`](../guides/scripts.md#using-different-python-versions)を使用するか、[仮想環境を作成してアクティブ化](../pip/environments.md)して直接`python`を使用してください。

## 特定のバージョンのインストール

特定のPythonバージョンをインストールするには：

```console
$ uv python install 3.12
```

複数のPythonバージョンをインストールするには：

```console
$ uv python install 3.11 3.12
```

代替のPython実装（例：PyPy）をインストールするには：

```console
$ uv python install pypy@3.12
```

詳細については、[`python install`](../concepts/python-versions.md#installing-a-python-version)のドキュメントを参照してください。

## Pythonインストールの表示

利用可能なPythonバージョンとインストール済みのPythonバージョンを表示するには：

```console
$ uv python list
```

詳細については、[`python list`](../concepts/python-versions.md#viewing-available-python-versions)のドキュメントを参照してください。

## 自動Pythonダウンロード

uvを使用するためにPythonを明示的にインストールする必要はありません。デフォルトでは、uvは必要に応じてPythonのバージョンを自動的にダウンロードします。例えば、次のコマンドはPython 3.12がインストールされていない場合にダウンロードします：

```console
$ uv run --python 3.12 python -c 'print("hello world")'
```

特定のPythonバージョンが要求されていない場合でも、uvは必要に応じて最新バージョンをダウンロードします。例えば、次のコマンドは新しい仮想環境を作成し、Pythonが見つからない場合は管理されたPythonバージョンをダウンロードします：

```console
$ uv venv
```

!!! tip

    自動Pythonダウンロードは、Pythonのダウンロードを制御したい場合に[簡単に無効にできます](../concepts/python-versions.md#disabling-automatic-python-downloads)。

## 既存のPythonインストールの使用

uvはシステムに存在する既存のPythonインストールを使用します。この動作には設定は必要ありません：uvはコマンドの呼び出しの要件を満たす場合にシステムPythonを使用します。詳細については、[Pythonの検出](../concepts/python-versions.md#discovery-of-python-versions)のドキュメントを参照してください。

uvにシステムPythonを使用させるには、`--python-preference only-system`オプションを指定します。詳細については、[Pythonバージョンの優先順位](../concepts/python-versions.md#adjusting-python-version-preferences)のドキュメントを参照してください。

## 次のステップ

`uv python`の詳細については、[Pythonバージョンの概念](../concepts/python-versions.md)ページと[コマンドリファレンス](../reference/cli.md#uv-python)を参照してください。

または、[スクリプトの実行](./scripts.md)方法を学び、uvでPythonを呼び出す方法を学んでください。
