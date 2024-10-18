# FastAPIでuvを使用する

[FastAPI](https://github.com/fastapi/fastapi)は、モダンで高性能なPythonウェブフレームワークです。
uvを使用してFastAPIプロジェクトを管理することができます。これには、依存関係のインストール、環境の管理、
FastAPIアプリケーションの実行などが含まれます。

!!! note

    このガイドのソースコードは[uv-fastapi-example](https://github.com/astral-sh/uv-fastapi-example)リポジトリで確認できます。

## 既存のFastAPIプロジェクトの移行

例として、[FastAPIドキュメント](https://fastapi.tiangolo.com/tutorial/bigger-applications/)で定義されているサンプルアプリケーションを考えてみます。
構造は次のようになっています：

```plaintext
project
└── app
    ├── __init__.py
    ├── main.py
    ├── dependencies.py
    ├── routers
    │   ├── __init__.py
    │   ├── items.py
    │   └── users.py
    └── internal
        ├── __init__.py
        └── admin.py
```

このアプリケーションでuvを使用するには、`project`ディレクトリ内で次のコマンドを実行します：

```console
$ uv init --app
```

これにより、`pyproject.toml`ファイルを含む[アプリケーションプロジェクト](../../concepts/projects.md#applications)が作成されます。

次に、FastAPIの依存関係を追加します：

```console
$ uv add fastapi --extra standard
```

これで、次のような構造になります：

```plaintext
project
├── pyproject.toml
└── app
    ├── __init__.py
    ├── main.py
    ├── dependencies.py
    ├── routers
    │   ├── __init__.py
    │   ├── items.py
    │   └── users.py
    └── internal
        ├── __init__.py
        └── admin.py
```

`pyproject.toml`ファイルの内容は次のようになります：

```toml title="pyproject.toml"
[project]
name = "uv-fastapi-example"
version = "0.1.0"
description = "FastAPI project"
readme = "README.md"
requires-python = ">=3.12"
dependencies = [
    "fastapi[standard]",
]
```

ここから、FastAPIアプリケーションを次のコマンドで実行できます：

```console
$ uv run fastapi dev
```

`uv run`はプロジェクトの依存関係を自動的に解決し、ロックし（つまり、`pyproject.toml`の隣に`uv.lock`を作成）、
仮想環境を作成し、その環境でコマンドを実行します。

http://127.0.0.1:8000/?token=jessica をウェブブラウザで開いてアプリケーションをテストします。

## デプロイ

FastAPIアプリケーションをDockerでデプロイするには、次の`Dockerfile`を使用できます：

```dockerfile title="Dockerfile"
FROM python:3.12-slim

# uvをインストールします。
COPY --from=ghcr.io/astral-sh/uv:latest /uv /uvx /bin/

# アプリケーションをコンテナにコピーします。
COPY . /app

# アプリケーションの依存関係をインストールします。
WORKDIR /app
RUN uv sync --frozen --no-cache

# アプリケーションを実行します。
CMD ["/app/.venv/bin/fastapi", "run", "app/main.py", "--port", "80", "--host", "0.0.0.0"]
```

次のコマンドでDockerイメージをビルドします：

```console
$ docker build -t fastapi-app .
```

次のコマンドでDockerコンテナをローカルで実行します：

```console
$ docker run -p 8000:80 fastapi-app
```

ブラウザでhttp://127.0.0.1:8000/?token=jessica にアクセスして、アプリケーションが正しく動作していることを確認します。

!!! tip

    Dockerでuvを使用する方法については、[Dockerガイド](./docker.md)を参照してください。
