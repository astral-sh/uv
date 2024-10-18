# Dockerでuvを使用する

## はじめに

!!! tip

    Dockerでアプリケーションをビルドする際のベストプラクティスの例として、[`uv-docker-example`](https://github.com/astral-sh/uv-docker-example)プロジェクトを参照してください。

### コンテナ内でuvを実行する

ビルド済みのuvが利用可能なDockerイメージが公開されています。コンテナ内でuvコマンドを実行するには：

```console
$ docker run ghcr.io/astral-sh/uv --help
```

### 利用可能なイメージ

uvは`uv`バイナリを含むdistroless Dockerイメージを提供しています。以下のタグが公開されています：

- `ghcr.io/astral-sh/uv:latest`
- `ghcr.io/astral-sh/uv:{major}.{minor}.{patch}` 例：`ghcr.io/astral-sh/uv:0.4.24`
- `ghcr.io/astral-sh/uv:{major}.{minor}` 例：`ghcr.io/astral-sh/uv:0.4`（最新のパッチバージョン）

さらに、uvは以下のイメージも公開しています：

<!-- prettier-ignore -->
- `alpine:3.20`ベース：
    - `ghcr.io/astral-sh/uv:alpine`
    - `ghcr.io/astral-sh/uv:alpine3.20`
- `debian:bookworm-slim`ベース：
    - `ghcr.io/astral-sh/uv:debian-slim`
    - `ghcr.io/astral-sh/uv:bookworm-slim`
- `buildpack-deps:bookworm`ベース：
    - `ghcr.io/astral-sh/uv:debian`
    - `ghcr.io/astral-sh/uv:bookworm`
- `python3.x-alpine`ベース：
    - `ghcr.io/astral-sh/uv:python3.13-alpine`
    - `ghcr.io/astral-sh/uv:python3.12-alpine`
    - `ghcr.io/astral-sh/uv:python3.11-alpine`
    - `ghcr.io/astral-sh/uv:python3.10-alpine`
    - `ghcr.io/astral-sh/uv:python3.9-alpine`
    - `ghcr.io/astral-sh/uv:python3.8-alpine`
- `python3.x-bookworm`ベース：
    - `ghcr.io/astral-sh/uv:python3.13-bookworm`
    - `ghcr.io/astral-sh/uv:python3.12-bookworm`
    - `ghcr.io/astral-sh/uv:python3.11-bookworm`
    - `ghcr.io/astral-sh/uv:python3.10-bookworm`
    - `ghcr.io/astral-sh/uv:python3.9-bookworm`
    - `ghcr.io/astral-sh/uv:python3.8-bookworm`
- `python3.x-slim-bookworm`ベース：
    - `ghcr.io/astral-sh/uv:python3.13-bookworm-slim`
    - `ghcr.io/astral-sh/uv:python3.12-bookworm-slim`
    - `ghcr.io/astral-sh/uv:python3.11-bookworm-slim`
    - `ghcr.io/astral-sh/uv:python3.10-bookworm-slim`
    - `ghcr.io/astral-sh/uv:python3.9-bookworm-slim`
    - `ghcr.io/astral-sh/uv:python3.8-bookworm-slim`
<!-- prettier-ignore-end -->

distrolessイメージと同様に、各イメージはuvバージョンタグ付きで公開されます：
`ghcr.io/astral-sh/uv:{major}.{minor}.{patch}-{base}`および
`ghcr.io/astral-sh/uv:{major}.{minor}-{base}` 例：`ghcr.io/astral-sh/uv:0.4.24-alpine`

詳細については、[GitHub Container](https://github.com/astral-sh/uv/pkgs/container/uv)ページを参照してください。

### uvのインストール

uvが事前にインストールされた上記のイメージのいずれかを使用するか、公式のdistroless Dockerイメージからバイナリをコピーしてuvをインストールします：

```dockerfile title="Dockerfile"
FROM python:3.12-slim-bookworm
COPY --from=ghcr.io/astral-sh/uv:latest /uv /uvx /bin/
```

または、インストーラーを使用します：

```dockerfile title="Dockerfile"
FROM python:3.12-slim-bookworm

# インストーラーはリリースアーカイブをダウンロードするためにcurl（および証明書）を必要とします
RUN apt-get update && apt-get install -y --no-install-recommends curl ca-certificates

# 最新のインストーラーをダウンロード
ADD https://astral.sh/uv/install.sh /uv-installer.sh

# インストーラーを実行して削除
RUN sh /uv-installer.sh && rm /uv-installer.sh

# インストールされたバイナリが`PATH`にあることを確認
ENV PATH="/root/.cargo/bin/:$PATH"
```

この方法では`curl`が利用可能である必要があります。

いずれの場合も、特定のuvバージョンに固定することがベストプラクティスです。例：

```dockerfile
COPY --from=ghcr.io/astral-sh/uv:0.4.24 /uv /uvx /bin/
```

または、インストーラーを使用する場合：

```dockerfile
ADD https://astral.sh/uv/0.4.24/install.sh /uv-installer.sh
```

### プロジェクトのインストール

uvを使用してプロジェクトを管理している場合、イメージにコピーしてインストールできます：

```dockerfile title="Dockerfile"
# プロジェクトをイメージにコピー
ADD . /app

# 凍結されたロックファイルを使用して新しい環境にプロジェクトを同期
WORKDIR /app
RUN uv sync --frozen
```

!!! important

    プロジェクトの仮想環境はローカルプラットフォームに依存しており、イメージ内で新たに作成する必要があるため、リポジトリ内の[`.dockerignore`ファイル](https://docs.docker.com/build/concepts/context/#dockerignore-files)に`.venv`を追加することがベストプラクティスです。

次に、デフォルトでアプリケーションを起動するには：

```dockerfile title="Dockerfile"
# プロジェクトが提供する`my_app`コマンドを前提としています
CMD ["uv", "run", "my_app"]
```

!!! tip

    Dockerイメージのビルド時間を改善するために、依存関係のインストールとプロジェクト自体のインストールを分離する[中間レイヤー](#intermediate-layers)を使用することがベストプラクティスです。

完全な例は[`uv-docker-example`プロジェクト](https://github.com/astral-sh/uv-docker-example/blob/main/Dockerfile)で確認できます。

### 環境の使用

プロジェクトがインストールされたら、仮想環境のバイナリディレクトリをパスの先頭に配置してプロジェクト仮想環境を_アクティブ化_することができます：

```dockerfile title="Dockerfile"
ENV PATH="/app/.venv/bin:$PATH"
```

または、環境を必要とするコマンドには`uv run`を使用できます：

```dockerfile title="Dockerfile"
RUN uv run some_script.py
```

!!! tip

    代わりに、[プロジェクト環境パスの設定](../../concepts/projects.md#configuring-the-project-environment-path)を行う`UV_PROJECT_ENVIRONMENT`設定を同期前に設定して、システムPython環境にインストールし、環境のアクティブ化をスキップすることもできます。

### インストールされたツールの使用

インストールされたツールを使用するには、[ツールバインディレクトリ](../../concepts/tools.md#the-bin-directory)がパスに含まれていることを確認します：

```dockerfile title="Dockerfile"
ENV PATH=/root/.local/bin:$PATH
RUN uv tool install cowsay
```

```console
$ docker run -it $(docker build -q .) /bin/bash -c "cowsay -t hello"
  _____
| hello |
  =====
     \
      \
        ^__^
        (oo)\_______
        (__)\       )\/\
            ||----w |
            ||     ||
```

!!! note

    ツールバインディレクトリの場所は、コンテナ内で`uv tool dir --bin`コマンドを実行して確認できます。

    代わりに、一定の場所に設定することもできます：

    ```dockerfile title="Dockerfile"
    ENV UV_TOOL_BIN_DIR=/opt/uv-bin/
    ```

### muslベースのイメージでのPythonのインストール

uvは[互換性のあるPythonバージョンをインストール](../install-python.md)しますが、muslベースのディストリビューション用のPythonのインストールはまだサポートしていません。例えば、PythonがインストールされていないAlpine Linuxベースのイメージを使用している場合、システムパッケージマネージャーで追加する必要があります：

```shell
apk add --no-cache python3~=3.12
```

## コンテナでの開発

開発時には、プロジェクトディレクトリをコンテナにマウントすることが有用です。このセットアップでは、プロジェクトへの変更がイメージを再ビルドすることなくコンテナ化されたサービスに即座に反映されます。ただし、プロジェクト仮想環境（`.venv`）をマウントに含めないことが重要です。仮想環境はプラットフォーム固有であり、イメージ用にビルドされたものを保持する必要があります。

### `docker run`でプロジェクトをマウントする

作業ディレクトリ内のプロジェクトを`/app`にバインドマウントし、匿名ボリュームで`.venv`ディレクトリを保持します：

```console
$ docker run --rm --volume .:/app --volume /app/.venv [...]
```

!!! tip

    コンテナが終了したときにコンテナと匿名ボリュームがクリーンアップされるように、`--rm`フラグを含めています。

完全な例は[`uv-docker-example`プロジェクト](https://github.com/astral-sh/uv-docker-example/blob/main/run.sh)で確認できます。

### `docker compose`での`watch`の設定

Docker composeを使用する場合、コンテナ開発のためのより高度なツールが利用可能です。
[`watch`](https://docs.docker.com/compose/file-watch/#compose-watch-versus-bind-mounts)オプションは、バインドマウントよりも細かい粒度での設定が可能であり、ファイルが変更されたときにコンテナ化されたサービスの更新をトリガーすることができます。

!!! note

    この機能は、Docker Desktop 4.24にバンドルされているCompose 2.22.0が必要です。

プロジェクトディレクトリを仮想環境を同期せずにマウントし、構成が変更されたときにイメージを再ビルドするように`watch`を設定します：

```yaml title="compose.yaml"
services:
  example:
    build: .

    # ...

    develop:
      # アプリを更新するための`watch`設定を作成
      #
      watch:
        # 作業ディレクトリをコンテナ内の`/app`ディレクトリと同期
        - action: sync
          path: .
          target: /app
          # プロジェクト仮想環境を除外
          ignore:
            - .venv/

        # `pyproject.toml`の変更時にイメージを再ビルド
        - action: rebuild
          path: ./pyproject.toml
```

次に、開発セットアップでコンテナを実行するには`docker compose watch`を実行します。

完全な例は[`uv-docker-example`プロジェクト](https://github.com/astral-sh/uv-docker-example/blob/main/compose.yml)で確認できます。

## 最適化

### バイトコードのコンパイル

バイトコードへのPythonソースファイルのコンパイルは、通常、インストール時間が増加する代わりに起動時間を改善するため、プロダクションイメージにとって望ましいです。

バイトコードのコンパイルを有効にするには、`--compile-bytecode`フラグを使用します：

```dockerfile title="Dockerfile"
RUN uv sync --compile-bytecode
```

または、`UV_COMPILE_BYTECODE`環境変数を設定して、Dockerfile内のすべてのコマンドがバイトコードをコンパイルするようにします：

```dockerfile title="Dockerfile"
ENV UV_COMPILE_BYTECODE=1
```

### キャッシュ

[キャッシュマウント](https://docs.docker.com/build/guide/mounts/#add-a-cache-mount)を使用して、ビルド間のパフォーマンスを向上させることができます：

```dockerfile title="Dockerfile"
ENV UV_LINK_MODE=copy

RUN --mount=type=cache,target=/root/.cache/uv \
    uv sync
```

デフォルトの[`UV_LINK_MODE`](../../reference/settings.md#link-mode)を変更すると、キャッシュと同期ターゲットが別のファイルシステム上にあるため、ハードリンクを使用できないことに関する警告が表示されなくなります。

キャッシュをマウントしていない場合、`--no-cache`フラグを使用するか`UV_NO_CACHE`を設定して、イメージサイズを削減できます。

!!! note

    キャッシュディレクトリの場所は、コンテナ内で`uv cache dir`コマンドを実行して確認できます。

    代わりに、一定の場所に設定することもできます：

    ```dockerfile title="Dockerfile"
    ENV UV_CACHE_DIR=/opt/uv-cache/
    ```

### 中間レイヤー

uvを使用してプロジェクトを管理している場合、`--no-install`オプションを使用して推移的依存関係のインストールを独自のレイヤーに移動することで、ビルド時間を改善できます。

`uv sync --no-install-project`はプロジェクトの依存関係をインストールしますが、プロジェクト自体はインストールしません。プロジェクトは頻繁に変更されますが、その依存関係は一般的に静的であるため、これは大きな時間の節約になります。

```dockerfile title="Dockerfile"
# uvのインストール
FROM python:3.12-slim
COPY --from=ghcr.io/astral-sh/uv:latest /uv /uvx /bin/

# 作業ディレクトリを`app`ディレクトリに変更
WORKDIR /app

# 依存関係のインストール
RUN --mount=type=cache,target=/root/.cache/uv \
    --mount=type=bind,source=uv.lock,target=uv.lock \
    --mount=type=bind,source=pyproject.toml,target=pyproject.toml \
    uv sync --frozen --no-install-project

# プロジェクトをイメージにコピー
ADD . /app

# プロジェクトの同期
RUN --mount=type=cache,target=/root/.cache/uv \
    uv sync --frozen
```

`pyproject.toml`はプロジェクトのルートと名前を識別するために必要ですが、プロジェクトの_内容_は最終的な`uv sync`コマンドまでイメージにコピーされません。

!!! tip

    [ワークスペース](../../concepts/workspaces.md)を使用している場合、プロジェクト_および_ワークスペースメンバーを除外する`--no-install-workspace`フラグを使用します。

    同期から特定のパッケージを除外する場合は、`--no-install-package <name>`を使用します。

### 非編集可能なインストール

デフォルトでは、uvはプロジェクトとワークスペースメンバーを編集可能モードでインストールし、ソースコードへの変更が環境に即座に反映されるようにします。

`uv sync`および`uv run`はどちらも`--no-editable`フラグを受け入れ、uvにプロジェクトを非編集可能モードでインストールするよう指示し、ソースコードへの依存を削除します。

マルチステージDockerイメージのコンテキストでは、`--no-editable`を使用して、あるステージから同期された仮想環境にプロジェクトを含め、最終イメージには仮想環境のみ（ソースコードは含まない）をコピーできます。

例：

```dockerfile title="Dockerfile"
# uvのインストール
FROM python:3.12-slim AS builder
COPY --from=ghcr.io/astral-sh/uv:latest /uv /uvx /bin/

# 作業ディレクトリを`app`ディレクトリに変更
WORKDIR /app

# 依存関係のインストール
RUN --mount=type=cache,target=/root/.cache/uv \
    --mount=type=bind,source=uv.lock,target=uv.lock \
    --mount=type=bind,source=pyproject.toml,target=pyproject.toml \
    uv sync --frozen --no-install-project --no-editable

# プロジェクトを中間イメージにコピー
ADD . /app

# プロジェクトの同期
RUN --mount=type=cache,target=/root/.cache/uv \
    uv sync --frozen --no-editable

FROM python:3.12-slim

# ソースコードではなく環境をコピー
COPY --from=builder --chown=app:app /app/.venv /app/.venv

# アプリケーションの実行
CMD ["/app/.venv/bin/hello"]
```

### 一時的にuvを使用する

最終イメージでuvが不要な場合、各呼び出しでバイナリをマウントできます：

```dockerfile title="Dockerfile"
RUN --mount=from=ghcr.io/astral-sh/uv,source=/uv,target=/bin/uv \
    uv sync
```

## pipインターフェースの使用

### パッケージのインストール

コンテナはすでに隔離されているため、このコンテキストでシステムPython環境を安全に使用できます。`--system`フラグを使用してシステム環境にインストールします：

```dockerfile title="Dockerfile"
RUN uv pip install --system ruff
```

デフォルトでシステムPython環境を使用するには、`UV_SYSTEM_PYTHON`変数を設定します：

```dockerfile title="Dockerfile"
ENV UV_SYSTEM_PYTHON=1
```

代わりに、仮想環境を作成してアクティブ化できます：

```dockerfile title="Dockerfile"
RUN uv venv /opt/venv
# 仮想環境を自動的に使用
ENV VIRTUAL_ENV=/opt/venv
# エントリーポイントを環境の先頭に配置
ENV PATH="/opt/venv/bin:$PATH"
```

仮想環境を使用する場合、uvの呼び出しから`--system`フラグを省略する必要があります：

```dockerfile title="Dockerfile"
RUN uv pip install ruff
```

### 要件のインストール

要件ファイルをインストールするには、コンテナにコピーします：

```dockerfile title="Dockerfile"
COPY requirements.txt .
RUN uv pip install -r requirements.txt
```

### プロジェクトのインストール

要件と一緒にプロジェクトをインストールする場合、要件のコピーをプロジェクト自体のコピーから分離することがベストプラクティスです。これにより、プロジェクトの依存関係（頻繁には変更されない）をプロジェクト自体（非常に頻繁に変更される）とは別にキャッシュできます。

```dockerfile title="Dockerfile"
COPY pyproject.toml .
RUN uv pip install -r pyproject.toml
COPY . .
RUN uv pip install -e .
```
