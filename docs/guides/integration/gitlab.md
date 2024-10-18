# GitLab CI/CDでuvを使用する

## uvイメージの使用

Astralは、uvがプリインストールされた[Dockerイメージ](docker.md#available-images)を提供しています。
ワークフローに適したバリアントを選択してください。

```yaml title="gitlab-ci.yml"
variables:
  UV_VERSION: 0.4
  PYTHON_VERSION: 3.12
  BASE_LAYER: bookworm-slim

stages:
  - analysis

uv:
  stage: analysis
  image: ghcr.io/astral-sh/uv:$UV_VERSION-python$PYTHON_VERSION-$BASE_LAYER
  script:
    # your `uv` commands
```

## キャッシュ

ワークフローの実行間でuvキャッシュを保持することで、パフォーマンスを向上させることができます。

```yaml
uv-install:
  variables:
    UV_CACHE_DIR: .uv-cache
  cache:
    - key:
        files:
          - uv.lock
      paths:
        - $UV_CACHE_DIR
  script:
    # Your `uv` commands
    - uv cache prune --ci
```

キャッシュの設定に関する詳細は、[GitLabキャッシュドキュメント](https://docs.gitlab.com/ee/ci/caching/)を参照してください。

ジョブの最後に`uv cache prune --ci`を使用することをお勧めします。これにより、キャッシュサイズが削減されます。詳細については、[uvキャッシュドキュメント](../../concepts/cache.md#caching-in-continuous-integration)を参照してください。

## `uv pip`の使用

uvプロジェクトインターフェースの代わりに`uv pip`インターフェースを使用する場合、uvはデフォルトで仮想環境を必要とします。システム環境にパッケージをインストールできるようにするには、すべてのuv呼び出しで`--system`フラグを使用するか、`UV_SYSTEM_PYTHON`変数を設定します。

`UV_SYSTEM_PYTHON`変数は、異なるスコープで定義できます。GitLabでの[変数とその優先順位の動作についてはこちら](https://docs.gitlab.com/ee/ci/variables/)を参照してください。

ワークフロー全体でオプトインするには、トップレベルで定義します：

```yaml title="gitlab-ci.yml"
variables:
  UV_SYSTEM_PYTHON: 1

# [...]
```

再度オプトアウトするには、任意のuv呼び出しで`--no-system`フラグを使用できます。

キャッシュを保持する場合、キャッシュキーとして`requirement.txt`や`pyproject.toml`を使用することを検討してください。
