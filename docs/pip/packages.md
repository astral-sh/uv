# パッケージの管理

## パッケージのインストール

仮想環境にパッケージをインストールするには、例えばFlaskをインストールします：

```console
$ uv pip install flask
```

オプションの依存関係を有効にしてパッケージをインストールするには、例えばFlaskの「dotenv」エクストラを有効にします：

```console
$ uv pip install "flask[dotenv]"
```

複数のパッケージをインストールするには、例えばFlaskとRuffをインストールします：

```console
$ uv pip install flask ruff
```

制約付きでパッケージをインストールするには、例えばRuff v0.2.0以降をインストールします：

```console
$ uv pip install 'ruff>=0.2.0'
```

特定のバージョンのパッケージをインストールするには、例えばRuff v0.3.0をインストールします：

```console
$ uv pip install 'ruff==0.3.0'
```

ディスクからパッケージをインストールするには：

```console
$ uv pip install "ruff @ ./projects/ruff"
```

GitHubからパッケージをインストールするには：

```console
$ uv pip install "git+https://github.com/astral-sh/ruff"
```

特定のリファレンスでGitHubからパッケージをインストールするには：

```console
$ # タグをインストール
$ uv pip install "git+https://github.com/astral-sh/ruff@v0.2.0"

$ # コミットをインストール
$ uv pip install "git+https://github.com/astral-sh/ruff@1fadefa67b26508cc59cf38e6130bde2243c929d"

$ # ブランチをインストール
$ uv pip install "git+https://github.com/astral-sh/ruff@main"
```

プライベートリポジトリからのインストールについては、[Git認証](../configuration/authentication.md#git-authentication)のドキュメントを参照してください。

## 編集可能なパッケージ

編集可能なパッケージは、ソースコードの変更が有効になるために再インストールする必要はありません。

現在のプロジェクトを編集可能なパッケージとしてインストールするには：

```console
$ uv pip install -e .
```

別のディレクトリにあるプロジェクトを編集可能なパッケージとしてインストールするには：

```console
$ uv pip install -e ruff @ ./project/ruff
```

## ファイルからパッケージをインストールする

標準のファイル形式から複数のパッケージを一度にインストールできます。

`requirements.txt`ファイルからインストールするには：

```console
$ uv pip install -r requirements.txt
```

`requirements.txt`ファイルに関する詳細は、[`uv pip compile`](./compile.md)のドキュメントを参照してください。

`pyproject.toml`ファイルからインストールするには：

```console
$ uv pip install -r pyproject.toml
```

オプションの依存関係を有効にして`pyproject.toml`ファイルからインストールするには、例えば「foo」エクストラを有効にします：

```console
$ uv pip install -r pyproject.toml --extra foo
```

すべてのオプションの依存関係を有効にして`pyproject.toml`ファイルからインストールするには：

```console
$ uv pip install -r pyproject.toml --all-extras
```

## パッケージのアンインストール

パッケージをアンインストールするには、例えばFlaskをアンインストールします：

```console
$ uv pip uninstall flask
```

複数のパッケージをアンインストールするには、例えばFlaskとRuffをアンインストールします：

```console
$ uv pip uninstall flask ruff
```
