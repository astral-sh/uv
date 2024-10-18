# GitHub Actionsでuvを使用する

## インストール

GitHub Actionsで使用する場合、公式の[`astral-sh/setup-uv`](https://github.com/astral-sh/setup-uv)アクションをお勧めします。これにより、uvがインストールされ、PATHに追加され、（オプションで）キャッシュが永続化されるなど、uvがサポートするすべてのプラットフォームに対応しています。

最新バージョンのuvをインストールするには：

```yaml title="example.yml" hl_lines="11-12"
name: Example

jobs:
  uv-example:
    name: python
    runs-on: ubuntu-latest

    steps:
      - uses: actions/checkout@v4

      - name: Install uv
        uses: astral-sh/setup-uv@v3
```

特定のuvバージョンにピン留めすることが推奨されます。例えば：

```yaml title="example.yml" hl_lines="14 15"
name: Example

jobs:
  uv-example:
    name: python
    runs-on: ubuntu-latest

    steps:
      - uses: actions/checkout@v4

      - name: Install uv
        uses: astral-sh/setup-uv@v3
        with:
          # Install a specific version of uv.
          version: "0.4.24"
```

## Pythonのセットアップ

Pythonは`python install`コマンドでインストールできます：

```yaml title="example.yml" hl_lines="14 15"
name: Example

jobs:
  uv-example:
    name: python
    runs-on: ubuntu-latest

    steps:
      - uses: actions/checkout@v4

      - name: Install uv
        uses: astral-sh/setup-uv@v3

      - name: Set up Python
        run: uv python install
```

これにより、プロジェクトでピン留めされたPythonバージョンが尊重されます。

また、マトリックスを使用する場合は、次のように：

```yaml title="example.yml"
strategy:
  matrix:
    python-version:
      - "3.10"
      - "3.11"
      - "3.12"
```

`python install`呼び出しにバージョンを提供します：

```yaml title="example.yml" hl_lines="14 15"
name: Example

jobs:
  uv-example:
    name: python
    runs-on: ubuntu-latest

    steps:
      - uses: actions/checkout@v4

      - name: Install uv
        uses: astral-sh/setup-uv@v3

      - name: Set up Python ${{ matrix.python-version }}
        run: uv python install ${{ matrix.python-version }}
```

または、公式のGitHub `setup-python`アクションを使用することもできます。これは、GitHubがランナーと一緒にPythonバージョンをキャッシュするため、より高速です。

[`python-version-file`](https://github.com/actions/setup-python/blob/main/docs/advanced-usage.md#using-the-python-version-file-input)オプションを設定して、プロジェクトのピン留めバージョンを使用します：

```yaml title="example.yml" hl_lines="14 15 16 17"
name: Example

jobs:
  uv-example:
    name: python
    runs-on: ubuntu-latest

    steps:
      - uses: actions/checkout@v4

      - name: Install uv
        uses: astral-sh/setup-uv@v3

      - name: "Set up Python"
        uses: actions/setup-python@v5
        with:
          python-version-file: ".python-version"
```

または、`pyproject.toml`ファイルを指定して、ピン留めを無視し、プロジェクトの`requires-python`制約に互換性のある最新バージョンを使用します：

```yaml title="example.yml" hl_lines="17"
name: Example

jobs:
  uv-example:
    name: python
    runs-on: ubuntu-latest

    steps:
      - uses: actions/checkout@v4

      - name: Install uv
        uses: astral-sh/setup-uv@v3

      - name: "Set up Python"
        uses: actions/setup-python@v5
        with:
          python-version-file: "pyproject.toml"
```

## 同期と実行

uvとPythonがインストールされたら、`uv sync`でプロジェクトをインストールし、`uv run`で環境内でコマンドを実行できます：

```yaml title="example.yml" hl_lines="17-22"
name: Example

jobs:
  uv-example:
    name: python
    runs-on: ubuntu-latest

    steps:
      - uses: actions/checkout@v4

      - name: Install uv
        uses: astral-sh/setup-uv@v3

      - name: Set up Python
        run: uv python install

      - name: Install the project
        run: uv sync --all-extras --dev

      - name: Run tests
        # For example, using `pytest`
        run: uv run pytest tests
```

!!! tip

    [`UV_PROJECT_ENVIRONMENT`設定](../../concepts/projects.md#configuring-the-project-environment-path)を使用して、仮想環境を作成せずにシステムPython環境にインストールすることができます。

## キャッシュ

ワークフローの実行間でuvのキャッシュを保存することで、CIの時間を短縮できる場合があります。

[`astral-sh/setup-uv`](https://github.com/astral-sh/setup-uv)には、キャッシュを永続化するための組み込みサポートがあります：

```yaml title="example.yml"
- name: Enable caching
  uses: astral-sh/setup-uv@v3
  with:
    enable-cache: true
```

ランナー上でカスタムキャッシュディレクトリを使用するようにアクションを構成できます：

```yaml title="example.yml"
- name: Define a custom uv cache path
  uses: astral-sh/setup-uv@v3
  with:
    enable-cache: true
    cache-local-path: "/path/to/cache"
```

または、ロックファイルが変更されたときに無効にします：

```yaml title="example.yml"
- name: Define a cache dependency glob
  uses: astral-sh/setup-uv@v3
  with:
    enable-cache: true
    cache-dependency-glob: "uv.lock"
```

または、任意のrequirementsファイルが変更されたときに無効にします：

```yaml title="example.yml"
- name: Define a cache dependency glob
  uses: astral-sh/setup-uv@v3
  with:
    enable-cache: true
    cache-dependency-glob: "requirements**.txt"
```

`astral-sh/setup-uv`は、ホストアーキテクチャとプラットフォームごとに自動的に別々のキャッシュキーを使用します。

または、`actions/cache`アクションを使用してキャッシュを手動で管理することもできます：

```yaml title="example.yml"
jobs:
  install_job:
    env:
      # Configure a constant location for the uv cache
      UV_CACHE_DIR: /tmp/.uv-cache

    steps:
      # ... setup up Python and uv ...

      - name: Restore uv cache
        uses: actions/cache@v4
        with:
          path: /tmp/.uv-cache
          key: uv-${{ runner.os }}-${{ hashFiles('uv.lock') }}
          restore-keys: |
            uv-${{ runner.os }}-${{ hashFiles('uv.lock') }}
            uv-${{ runner.os }}

      # ... install packages, run tests, etc ...

      - name: Minimize uv cache
        run: uv cache prune --ci
```

`uv cache prune --ci`コマンドは、キャッシュのサイズを減らすために使用され、CIに最適化されています。そのパフォーマンスへの影響は、インストールされるパッケージに依存します。

!!! tip

    `uv pip`を使用する場合、キャッシュキーには`requirements.txt`を使用してください。

!!! note

    [post-job-hook]: https://docs.github.com/en/actions/hosting-your-own-runners/managing-self-hosted-runners/running-scripts-before-or-after-a-job

    非エフェメラルなセルフホストランナーを使用する場合、デフォルトのキャッシュディレクトリは無制限に成長する可能性があります。この場合、ジョブ間でキャッシュを共有することは最適ではないかもしれません。代わりに、キャッシュをGitHubワークスペース内に移動し、ジョブが終了したら削除します。[Post Job Hook][post-job-hook]を使用します。

    ```yaml
    install_job:
      env:
        # Configure a relative location for the uv cache
        UV_CACHE_DIR: ${{ github.workspace }}/.cache/uv
    ```

    ポストジョブフックを使用するには、セルフホストランナーで`ACTIONS_RUNNER_HOOK_JOB_STARTED`環境変数をクリーンアップスクリプトのパスに設定する必要があります。以下のようなスクリプトです。

    ```sh title="clean-uv-cache.sh"
    #!/usr/bin/env sh
    uv cache clean
    ```

## `uv pip`の使用

uvプロジェクトインターフェースの代わりに`uv pip`インターフェースを使用する場合、uvはデフォルトで仮想環境を必要とします。パッケージをシステム環境にインストールするには、すべての`uv`呼び出しで`--system`フラグを使用するか、`UV_SYSTEM_PYTHON`変数を設定します。

`UV_SYSTEM_PYTHON`変数は、異なるスコープで定義できます。

ワークフロー全体に対してオプトインするには、トップレベルで定義します：

```yaml title="example.yml"
env:
  UV_SYSTEM_PYTHON: 1

jobs: ...
```

または、ワークフロー内の特定のジョブに対してオプトインします：

```yaml title="example.yml"
jobs:
  install_job:
    env:
      UV_SYSTEM_PYTHON: 1
    ...
```

または、ジョブ内の特定のステップに対してオプトインします：

```yaml title="example.yml"
steps:
  - name: Install requirements
    run: uv pip install -r requirements.txt
    env:
      UV_SYSTEM_PYTHON: 1
```

再度オプトアウトするには、任意のuv呼び出しで`--no-system`フラグを使用できます。
