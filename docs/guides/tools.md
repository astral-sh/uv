# ツールの使用

多くのPythonパッケージは、ツールとして使用できるアプリケーションを提供しています。uvは、ツールを簡単に呼び出し、インストールするための専門的なサポートを提供します。

## ツールの実行

`uvx`コマンドは、ツールをインストールせずに呼び出します。

例えば、`ruff`を実行するには：

```console
$ uvx ruff
```

!!! note

    これは正確には次のコマンドと同等です：

    ```console
    $ uv tool run ruff
    ```

    `uvx`は便宜上のエイリアスとして提供されています。

ツール名の後に引数を指定できます：

```console
$ uvx pycowsay hello from uv

  -------------
< hello from uv >
  -------------
   \   ^__^
    \  (oo)\_______
       (__)\       )\/\
           ||----w |
           ||     ||

```

`uvx`を使用する場合、ツールは一時的で隔離された環境にインストールされます。

!!! note

    ツールを[_プロジェクト_](../concepts/projects.md)で実行し、ツールがプロジェクトのインストールを必要とする場合（例：`pytest`や`mypy`を使用する場合）、`uvx`の代わりに[`uv run`](./projects.md#running-commands)を使用することをお勧めします。そうしないと、ツールはプロジェクトから隔離された仮想環境で実行されます。

    プロジェクトがフラットな構造を持っている場合（例：モジュールに`src`ディレクトリを使用せず、プロジェクト自体がインストールを必要としない場合）、`uvx`を使用しても問題ありません。この場合、ツールのバージョンをプロジェクトの依存関係に固定したい場合にのみ`uv run`を使用することが有益です。

## パッケージ名が異なるコマンド

`uvx ruff`が呼び出されると、uvは`ruff`コマンドを提供する`ruff`パッケージをインストールします。ただし、パッケージ名とコマンド名が異なる場合があります。

`--from`オプションを使用して、特定のパッケージからコマンドを呼び出すことができます。例：`httpie`が提供する`http`：

```console
$ uvx --from httpie http
```

## 特定のバージョンの要求

特定のバージョンでツールを実行するには、`command@<version>`を使用します：

```console
$ uvx ruff@0.3.0 check
```

最新バージョンでツールを実行するには、`command@latest`を使用します：

```console
$ uvx ruff@latest check
```

`--from`オプションを使用してパッケージバージョンを指定することもできます。上記のように：

```console
$ uvx --from 'ruff==0.3.0' ruff check
```

または、バージョン範囲を制約するには：

```console
$ uvx --from 'ruff>0.2.0,<0.3.0' ruff check
```

`@`構文は正確なバージョン以外には使用できないことに注意してください。

## 異なるソースの要求

`--from`オプションを使用して、代替ソースからインストールすることもできます。

例えば、gitから取得するには：

```console
$ uvx --from git+https://github.com/httpie/cli httpie
```

## プラグインを持つコマンド

追加の依存関係を含めることができます。例：`mkdocs`を実行する際に`mkdocs-material`を含めるには：

```console
$ uvx --with mkdocs-material mkdocs --help
```

## ツールのインストール

ツールを頻繁に使用する場合、`uvx`を繰り返し呼び出す代わりに、永続的な環境にインストールし、`PATH`に追加することが便利です。

!!! tip

    `uvx`は`uv tool run`の便利なエイリアスです。他のツールと対話するためのコマンドはすべて`uv tool`プレフィックスが必要です。

`ruff`をインストールするには：

```console
$ uv tool install ruff
```

ツールがインストールされると、その実行可能ファイルは`PATH`にある`bin`ディレクトリに配置され、uvなしでツールを実行できるようになります。`PATH`にない場合は警告が表示され、`uv tool update-shell`を使用して`PATH`に追加できます。

`ruff`をインストールした後、次のように利用可能になります：

```console
$ ruff --version
```

`uv pip install`とは異なり、ツールをインストールしてもそのモジュールは現在の環境で利用できません。例えば、次のコマンドは失敗します：

```console
$ python -c "import ruff"
```

この隔離は、ツール、スクリプト、およびプロジェクトの依存関係間の相互作用と競合を減らすために重要です。

`uvx`とは異なり、`uv tool install`は_パッケージ_に対して操作を行い、ツールが提供するすべての実行可能ファイルをインストールします。

例えば、次のコマンドは`http`、`https`、および`httpie`の実行可能ファイルをインストールします：

```console
$ uv tool install httpie
```

さらに、パッケージバージョンを`--from`なしで含めることができます：

```console
$ uv tool install 'httpie>0.1.0'
```

同様に、パッケージソースについても：

```console
$ uv tool install git+https://github.com/httpie/cli
```

`uvx`と同様に、インストールには追加のパッケージを含めることができます：

```console
$ uv tool install mkdocs --with mkdocs-material
```

## ツールのアップグレード

ツールをアップグレードするには、`uv tool upgrade`を使用します：

```console
$ uv tool upgrade ruff
```

ツールのアップグレードは、ツールのインストール時に提供されたバージョン制約を尊重します。例えば、`uv tool install ruff >=0.3,<0.4`の後に`uv tool upgrade ruff`を実行すると、Ruffは`>=0.3,<0.4`の範囲内の最新バージョンにアップグレードされます。

バージョン制約を置き換えるには、`uv tool install`を使用してツールを再インストールします：

```console
$ uv tool install ruff>=0.4
```

すべてのツールをアップグレードするには：

```console
$ uv tool upgrade --all
```

## 次のステップ

uvでツールを管理する方法の詳細については、[ツールの概念](../concepts/tools.md)ページと[コマンドリファレンス](../reference/cli.md#uv-tool)を参照してください。

または、[プロジェクトで作業する](./projects.md)方法を学んでください。
