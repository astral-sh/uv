# Jupyterをuvで使用する

[Jupyter](https://jupyter.org/)ノートブックは、インタラクティブなコンピューティング、データ分析、および可視化のための人気のツールです。Jupyterをuvと一緒に使用する方法はいくつかあり、プロジェクトと対話するため、またはスタンドアロンツールとして使用することができます。

## プロジェクト内でJupyterを使用する

[プロジェクト](../../concepts/projects.md)内で作業している場合、次のコマンドを使用してプロジェクトの仮想環境にアクセスできるJupyterサーバーを起動できます。

```console
$ uv run --with jupyter jupyter lab
```

デフォルトでは、`jupyter lab`はサーバーを[http://localhost:8888/lab](http://localhost:8888/lab)で起動します。

ノートブック内では、プロジェクトの他のファイルと同様にプロジェクトのモジュールをインポートできます。たとえば、プロジェクトが`requests`に依存している場合、`import requests`はプロジェクトの仮想環境から`requests`をインポートします。

プロジェクトの仮想環境への読み取り専用アクセスを探している場合は、それ以上の設定は必要ありません。ただし、ノートブック内から追加のパッケージをインストールする必要がある場合は、いくつかの追加の詳細を考慮する必要があります。

### カーネルの作成

ノートブック内からパッケージをインストールする必要がある場合、プロジェクト専用のカーネルを作成することをお勧めします。カーネルを使用すると、Jupyterサーバーは1つの環境で実行され、個々のノートブックはそれぞれの独立した環境で実行されます。

uvのコンテキストでは、Jupyter自体を分離された環境にインストールしながら、プロジェクトのカーネルを作成できます。たとえば、`uv run --with jupyter jupyter lab`のように。プロジェクトのカーネルを作成することで、ノートブックが正しい環境に接続され、ノートブック内からインストールされたパッケージがプロジェクトの仮想環境にインストールされることが保証されます。

カーネルを作成するには、`ipykernel`を開発依存関係としてインストールする必要があります。

```console
$ uv add --dev ipykernel
```

次に、`project`のカーネルを作成できます。

```console
$ uv run ipython kernel install --user --name=project
```

その後、サーバーを起動します。

```console
$ uv run --with jupyter jupyter lab
```

ノートブックを作成する際に、ドロップダウンから`project`カーネルを選択します。その後、`!uv add pydantic`を使用して`pydantic`をプロジェクトの依存関係に追加するか、`!uv pip install pydantic`を使用して`pydantic`をプロジェクトの仮想環境にインストールしますが、プロジェクトの`pyproject.toml`や`uv.lock`ファイルには変更を反映しません。どちらのコマンドも、ノートブック内で`import pydantic`を機能させます。

### カーネルなしでパッケージをインストールする

カーネルを作成したくない場合でも、ノートブック内からパッケージをインストールできます。ただし、いくつかの注意点があります。

`uv run --with jupyter`は分離された環境で実行されますが、ノートブック内では`!uv add`および関連コマンドはカーネルなしでもプロジェクトの環境を変更します。

たとえば、ノートブック内で`!uv add pydantic`を実行すると、`pydantic`がプロジェクトの依存関係および仮想環境に追加され、追加の設定やサーバーの再起動なしで`import pydantic`がすぐに機能します。

ただし、Jupyterサーバーが「アクティブ」な環境であるため、`!uv pip install`はパッケージをプロジェクト環境ではなく_Jupyter_環境にインストールします。そのような依存関係はJupyterサーバーの存続期間中は持続しますが、次回の`jupyter`呼び出し時には消える可能性があります。

ノートブックがpipに依存している場合（例：`%pip`マジックを使用）、Jupyterサーバーを起動する前に`uv venv --seed`を実行してプロジェクトの仮想環境にpipを含めることができます。たとえば、次のようにします。

```console
$ uv venv --seed
$ uv run --with jupyter jupyter lab
```

その後、ノートブック内での`%pip install`呼び出しはプロジェクトの仮想環境にパッケージをインストールします。ただし、そのような変更はプロジェクトの`pyproject.toml`や`uv.lock`ファイルには反映されません。

## スタンドアロンツールとしてJupyterを使用する

ノートブックにアドホックにアクセスする必要がある場合（つまり、Pythonスニペットをインタラクティブに実行するため）、`uv tool run jupyter lab`を使用していつでもJupyterサーバーを起動できます。これにより、分離された環境でJupyterサーバーが実行されます。

## プロジェクト環境以外でJupyterを使用する

[プロジェクト](../../concepts/projects.md)に関連付けられていない仮想環境（例：`pyproject.toml`や`uv.lock`がない）でJupyterを実行する必要がある場合、Jupyterを直接環境に追加できます。たとえば、次のようにします。

```console
$ uv venv --seed
$ uv pip install pydantic
$ uv pip install jupyterlab
$ .venv/bin/jupyter lab
```

ここから、ノートブック内で`import pydantic`が機能し、`!uv pip install`や`!pip install`を使用して追加のパッケージをインストールできます。

## VS CodeからJupyterを使用する

VS Codeのようなエディタ内からJupyterノートブックと対話することもできます。VS Code内でuv管理のプロジェクトをJupyterノートブックに接続するには、次のようにプロジェクトのカーネルを作成することをお勧めします。

```console
# プロジェクトを作成します。
$ uv init project
# プロジェクトディレクトリに移動します。
$ cd project
# ipykernelを開発依存関係として追加します。
$ uv add --dev ipykernel
# プロジェクトをVS Codeで開きます。
$ code .
```

プロジェクトディレクトリがVS Codeで開かれたら、コマンドパレットから「Create: New Jupyter Notebook」を選択して新しいJupyterノートブックを作成できます。カーネルを選択するように求められたら、「Python Environments」を選択し、先ほど作成した仮想環境（例：`.venv/bin/python`）を選択します。

!!! note

    VS Codeはプロジェクト環境に`ipykernel`が存在することを要求します。`ipykernel`を開発依存関係として追加したくない場合は、`uv pip install ipykernel`を使用してプロジェクト環境に直接インストールできます。

ノートブック内からプロジェクトの環境を操作する必要がある場合は、`uv`を明示的な開発依存関係として追加する必要があるかもしれません。

```console
$ uv add --dev uv
```

その後、`!uv add pydantic`を使用して`pydantic`をプロジェクトの依存関係に追加するか、`!uv pip install pydantic`を使用して`pydantic`をプロジェクトの仮想環境にインストールしますが、プロジェクトの`pyproject.toml`や`uv.lock`ファイルには変更を反映しません。
